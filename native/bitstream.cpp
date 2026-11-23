#include <initguid.h>

#include "media.hpp"
namespace rf
{
Bitstream::~Bitstream()
{
    if (client && running)
        client->Stop();
    close_packer();
    avcodec_parameters_free(&params);
}
void Bitstream::close_packer() noexcept
{
    if (mux)
    {
        if (mux->pb)
        {
            uint8_t *data{};
            avio_close_dyn_buf(mux->pb, &data);
            av_free(data);
        }
        avformat_free_context(mux);
        mux = nullptr;
    }
}
bool Bitstream::open(IMMDevice *device, AVStream *stream)
{
    if (!device)
        return false;
    com_ptr<IPropertyStore> props;
    PROPVARIANT form{};
    if (FAILED(device->OpenPropertyStore(STGM_READ, props.put())) ||
        FAILED(props->GetValue(PKEY_AudioEndpoint_FormFactor, &form)))
        return false;
    // Some display drivers expose a receiver as Speakers or UnknownDigitalPassthrough.
    // The exclusive encoded-format negotiation below establishes actual capability.
    bool can_bitstream =
        form.vt == VT_UI4 && (form.ulVal == DigitalAudioDisplayDevice || form.ulVal == UnknownDigitalPassthrough ||
                              form.ulVal == SPDIF || form.ulVal == Speakers);

    PropVariantClear(&form);
    if (!can_bitstream)
        return false;
    auto p = stream->codecpar;
    GUID sub{};
    unsigned lines = 2, hz = p->sample_rate;

    switch (p->codec_id)
    {
    case AV_CODEC_ID_AC3:
        sub = KSDATAFORMAT_SUBTYPE_IEC61937_DOLBY_DIGITAL;
        break;
    case AV_CODEC_ID_EAC3:
        sub = KSDATAFORMAT_SUBTYPE_IEC61937_DOLBY_DIGITAL_PLUS;
        hz *= 4;
        break;
    case AV_CODEC_ID_TRUEHD:
        sub = KSDATAFORMAT_SUBTYPE_IEC61937_DOLBY_MLP;
        lines = 8;
        hz = hz % 44100 == 0 ? 176400 : 192000;
        break;
    case AV_CODEC_ID_DTS:
        dts_hd = p->profile == AV_PROFILE_DTS_HD_HRA || p->profile == AV_PROFILE_DTS_HD_MA ||
                 p->profile == AV_PROFILE_DTS_HD_MA_X || p->profile == AV_PROFILE_DTS_HD_MA_X_IMAX;
        if (!dts_hd && p->profile != AV_PROFILE_DTS && p->profile != AV_PROFILE_DTS_ES &&
            p->profile != AV_PROFILE_DTS_96_24)
            return false;
        if (dts_hd)
        {
            sub = KSDATAFORMAT_SUBTYPE_IEC61937_DTS_HD;
            lines = 8;
            hz = 192000;
        }
        else
            sub = KSDATAFORMAT_SUBTYPE_IEC61937_DTS;
        break;
    default:
        return false;
    }
    WAVEFORMATEXTENSIBLE_IEC61937 t{};
    auto &x = t.FormatExt;
    auto &w = x.Format;
    w.wFormatTag = WAVE_FORMAT_EXTENSIBLE;
    w.nChannels = static_cast<WORD>(lines);
    w.nSamplesPerSec = hz;
    w.wBitsPerSample = 16;
    w.nBlockAlign = w.nChannels * 2;
    w.nAvgBytesPerSec = hz * w.nBlockAlign;
    w.cbSize = sizeof(t) - sizeof(w);
    x.dwChannelMask = lines == 2 ? KSAUDIO_SPEAKER_STEREO : KSAUDIO_SPEAKER_7POINT1_SURROUND;
    x.Samples.wValidBitsPerSample = 16;
    x.SubFormat = sub;
    t.dwEncodedChannelCount = p->ch_layout.nb_channels;
    t.dwEncodedSamplesPerSec = p->sample_rate;
    t.dwAverageBytesPerSec = p->bit_rate > 0 ? static_cast<DWORD>(p->bit_rate / 8) : 0;
    if (FAILED(device->Activate(__uuidof(IAudioClient), CLSCTX_ALL, nullptr, client.put_void())))
        return false;
    if (client->IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, &w, nullptr) != S_OK)
    {
        w.cbSize = sizeof(x) - sizeof(w);
        if (client->IsFormatSupported(AUDCLNT_SHAREMODE_EXCLUSIVE, &w, nullptr) != S_OK)
            return false;
    }
    // Reserve the real exclusive endpoint before advertising passthrough.
    REFERENCE_TIME period{}, minimum{};
    if (FAILED(client->GetDevicePeriod(&period, &minimum)))
        return false;
    auto hr = client->Initialize(AUDCLNT_SHAREMODE_EXCLUSIVE, 0, (std::max)(period * 4, REFERENCE_TIME(800000)), 0, &w,
                                 nullptr);
    if (hr == AUDCLNT_E_BUFFER_SIZE_NOT_ALIGNED)
    {
        UINT32 aligned{};
        if (FAILED(client->GetBufferSize(&aligned)))
            return false;
        client = nullptr;
        if (FAILED(device->Activate(__uuidof(IAudioClient), CLSCTX_ALL, nullptr, client.put_void())))
            return false;
        hr = client->Initialize(AUDCLNT_SHAREMODE_EXCLUSIVE, 0, (10000000LL * aligned + hz / 2) / hz, 0, &w, nullptr);
    }
    if (FAILED(hr) || FAILED(client->GetService(__uuidof(IAudioRenderClient), render.put_void())) ||
        FAILED(client->GetBufferSize(&capacity)))
        return false;
    rate = hz;
    frame_bytes = w.nBlockAlign;
    params = avcodec_parameters_alloc();
    if (!params)
        throw std::bad_alloc();
    avcheck(avcodec_parameters_copy(params, p));
    input_timebase = stream->time_base;
    open_packer();
    return true;
}
void Bitstream::open_packer()
{
    avcheck(avformat_alloc_output_context2(&mux, nullptr, "spdif", nullptr));
    auto out = avformat_new_stream(mux, nullptr);
    if (!out)
        throw std::bad_alloc();
    avcheck(avcodec_parameters_copy(out->codecpar, params));
    out->time_base = input_timebase;
    if (dts_hd)
        avcheck(av_opt_set_int(mux->priv_data, "dtshd_rate", 768000, 0));
    avcheck(avio_open_dyn_buf(&mux->pb));
    avcheck(avformat_write_header(mux, nullptr));
}
std::vector<uint8_t> Bitstream::pack(AVPacket *p, int64_t &pts)
{
    if (pending_pts == AV_NOPTS_VALUE)
        pending_pts = pts;
    p->stream_index = 0;
    // IEC 61937 carries no packet timestamps. Schedule the burst from its
    // original PTS above (Matroska can round adjacent TrueHD PTS to the same ms).
    p->pts = p->dts = AV_NOPTS_VALUE;
    avcheck(av_write_frame(mux, p));
    avio_flush(mux->pb);
    uint8_t *ptr{};
    int size = avio_get_dyn_buf(mux->pb, &ptr);
    if (size <= 0)
        return {};
    // The DTS-HD packer may strip extensions on overflow. Its type IV
    // wrapper carries the actual payload length after the 10-byte marker.
    if (dts_hd && (size < 20 || (ptr[18] | (int(ptr[19]) << 8)) != p->size))
        winrt::throw_hresult(MF_E_INVALID_STREAM_DATA);
    size = avio_close_dyn_buf(mux->pb, &ptr);
    mux->pb = nullptr;
    std::unique_ptr<uint8_t, decltype(&av_free)> mem(ptr, av_free);
    avcheck(avio_open_dyn_buf(&mux->pb));
    pts = pending_pts;
    pending_pts = AV_NOPTS_VALUE;
    return {ptr, ptr + size};
}
void Bitstream::pause()
{
    if (running)
    {
        check_hresult(client->Stop());
        running = false;
    }
}
void Bitstream::reset()
{
    pause();
    check_hresult(client->Reset());
    close_packer();
    open_packer();
    pending_pts = AV_NOPTS_VALUE;
}
bool Bitstream::write(const std::vector<uint8_t> &bytes, int64_t pts, Clock &clock, const std::atomic_bool &closed,
                      const std::atomic_uint &epoch, unsigned gen)
{
    if (bytes.size() % frame_bytes)
        winrt::throw_hresult(MF_E_INVALID_STREAM_DATA);
    size_t offset = 0;
    while (offset < bytes.size() && !closed && epoch == gen)
    {
        auto [position, active] = clock.read();
        if (!active)
        {
            pause();
            std::this_thread::sleep_for(4ms);
            continue;
        }
        UINT32 padding{};
        check_hresult(client->GetCurrentPadding(&padding));
        if (!running && padding)
        {
            check_hresult(client->Start());
            running = true;
        }
        double due = pts / 1e7 + double(offset / frame_bytes) / rate - position - double(padding) / rate;
        if (offset == 0 && due < -0.10)
            return true; // Drop a whole late burst, never part of a coded frame.
        if (due > 0.003 || padding == capacity)
        {
            std::this_thread::sleep_for(2ms);
            continue;
        }
        auto frames =
            static_cast<UINT32>((std::min)(size_t(capacity - padding), (bytes.size() - offset) / frame_bytes));
        BYTE *out{};
        check_hresult(render->GetBuffer(frames, &out));
        memcpy(out, bytes.data() + offset, size_t(frames) * frame_bytes);
        check_hresult(render->ReleaseBuffer(frames, 0));
        offset += size_t(frames) * frame_bytes;
        if (!running)
        {
            check_hresult(client->Start());
            running = true;
        }
    }
    return offset == bytes.size();
}
} // namespace rf
