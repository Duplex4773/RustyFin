#include "media.hpp"
#include <winrt/Windows.Graphics.DirectX.Direct3D11.h>

namespace rf
{
using namespace winrt::Windows::Media::Core;
using namespace winrt::Windows::Media::MediaProperties;
using winrt::Windows::Foundation::TimeSpan;
using winrt::Windows::Storage::Streams::Buffer;
using winrt::Windows::Storage::Streams::IBuffer;
struct Decoder
{
    AVCodecContext *codec{};
    AVBSFContext *bsf{};
    SwrContext *swr{};
    AVFrame *frame{av_frame_alloc()};
    AVChannelLayout layout{};
    ~Decoder()
    {
        av_channel_layout_uninit(&layout);
        av_frame_free(&frame);
        swr_free(&swr);
        av_bsf_free(&bsf);
        avcodec_free_context(&codec);
    }
};
using AllocatePacket = uint8_t *(__cdecl *)(void *, int);
using ConvertMel = int(__cdecl *)(const uint8_t *, int, int, uint16_t *, void *, AllocatePacket);
struct State
{
    std::atomic_bool closed{false}, raw{false}, failed{false};
    std::atomic_uint generation{};
    std::atomic_int channels{};
    ConvertMel convert_mel{};
    std::atomic_int mel_error{};
    Clock clock;
    std::mutex io_mutex;
    com_ptr<IMFByteStream> io;
    void cancel()
    {
        closed = true;
        com_ptr<IMFByteStream> bytes;
        {
            std::lock_guard lock(io_mutex);
            bytes = std::move(io);
        }
        if (bytes)
            bytes->Close();
    }
    std::mutex endpoint_mutex;
    std::wstring endpoint;
    bool have_endpoint{};
    bool changed()
    {
        std::lock_guard lock(endpoint_mutex);
        if (!have_endpoint)
            return false;
        com_ptr<IMMDeviceEnumerator> devices;
        com_ptr<IMMDevice> device;
        if (FAILED(CoCreateInstance(__uuidof(MMDeviceEnumerator), nullptr, CLSCTX_ALL, IID_PPV_ARGS(devices.put()))) ||
            FAILED(devices->GetDefaultAudioEndpoint(eRender, eMultimedia, device.put())))
            return false;
        LPWSTR id{};
        if (FAILED(device->GetId(&id)))
            return false;
        bool differs = endpoint != id;
        CoTaskMemFree(id);
        return differs;
    }
};
inline IBuffer buffer(const uint8_t *bytes, size_t size)
{
    if (size > UINT32_MAX)
        winrt::throw_hresult(E_INVALIDARG);
    Buffer b(static_cast<uint32_t>(size));
    if (size)
        memcpy(b.data(), bytes, size);
    b.Length(static_cast<uint32_t>(size));
    return b;
}
class Source : public std::enable_shared_from_this<Source>
{
    std::shared_ptr<State> state;
    Format input;
    Decoder dec;
    Packet pending_picture;
    uint16_t mel_mappings{};
    int video{-1}, audio{-1}, rate{};
    int64_t origin{}, next_audio{};
    std::deque<Packet> video_q, audio_q;
    size_t queued{};
    bool eof{}, audio_drained{}, first_picture{true}, first_audio{true};
    std::mutex mutex;
    std::unique_ptr<Bitstream> sink;
    std::thread raw_thread;
    std::atomic_bool stop_audio{false};
    MediaStreamSource mss{nullptr};
    VideoStreamDescriptor picture{nullptr};
    AudioStreamDescriptor sound{nullptr};
    int64_t stamp(int64_t pts, int index) const
    {
        return av_rescale_q(pts, input.p->streams[index]->time_base, ticks) - origin;
    }
    static int interrupted(void *opaque)
    {
        return static_cast<State *>(opaque)->closed.load();
    }
    Packet next(int index)
    {
        auto &q = index == video ? video_q : audio_q;
        while (q.empty() && !eof && !state->closed)
        {
            auto p = packet();
            int rc = av_read_frame(input.p, p.get());
            if (rc == AVERROR_EOF)
            {
                eof = true;
                break;
            }
            avcheck(rc);
            if (p->stream_index == video || (audio >= 0 && p->stream_index == audio))
            {
                queued += p->size;
                if (queued > 256ull * 1024 * 1024)
                    winrt::throw_hresult(MF_E_BUFFERTOOSMALL);
                (p->stream_index == video ? video_q : audio_q).push_back(std::move(p));
            }
        }
        if (q.empty())
            return {};
        auto p = std::move(q.front());
        q.pop_front();
        queued -= p->size;
        return p;
    }
    [[noreturn]] void reject_mel(int reason)
    {
        state->mel_error = reason;
        winrt::throw_hresult(MF_E_INVALID_STREAM_DATA);
    }
    Packet convert_mel(const uint8_t *data, int size, bool configuration)
    {
        auto result = packet();
        auto allocate = [](void *p, int n) noexcept -> uint8_t * {
            auto out = static_cast<AVPacket *>(p);
            return av_new_packet(out, n) < 0 ? nullptr : out->data;
        };
        auto rc = state->convert_mel(data, size, configuration, &mel_mappings, result.get(), allocate);
        if (rc < 0)
            throw std::bad_alloc();
        if (rc)
            reject_mel(rc);
        return result;
    }
    Packet read_picture()
    {
        if (state->mel_error)
            winrt::throw_hresult(MF_E_INVALID_STREAM_DATA);
        auto out = packet();
        for (;;)
        {
            auto rc = av_bsf_receive_packet(dec.bsf, out.get());
            if (rc == 0)
                break;
            if (rc == AVERROR_EOF)
                return nullptr;
            if (rc != AVERROR(EAGAIN))
                avcheck(rc);
            auto p = next(video);
            avcheck(av_bsf_send_packet(dec.bsf, p.get()));
        }
        if (!state->convert_mel)
            return out;
        auto converted = convert_mel(out->data, out->size, false);
        converted->pts = out->pts;
        converted->dts = out->dts;
        converted->duration = out->duration;
        converted->flags = out->flags;
        return converted;
    }
    MediaStreamSample video_sample()
    {
        auto out = pending_picture ? std::move(pending_picture) : read_picture();
        if (!out)
            return nullptr;
        auto pts = out->pts != AV_NOPTS_VALUE ? out->pts : out->dts;
        if (pts == AV_NOPTS_VALUE)
            winrt::throw_hresult(MF_E_INVALID_TIMESTAMP);
        auto s = MediaStreamSample::CreateFromBuffer(buffer(out->data, out->size), TimeSpan{stamp(pts, video)});
        if (out->dts != AV_NOPTS_VALUE)
            s.DecodeTimestamp(TimeSpan{stamp(out->dts, video)});
        if (out->duration > 0)
            s.Duration(TimeSpan{av_rescale_q(out->duration, input.p->streams[video]->time_base, ticks)});
        s.KeyFrame((out->flags & AV_PKT_FLAG_KEY) != 0);
        s.Discontinuous(std::exchange(first_picture, false));
        return s;
    }
    MediaStreamSample audio_sample()
    {
        for (;;)
        {
            auto rc = avcodec_receive_frame(dec.codec, dec.frame);
            if (rc == 0)
                break;
            if (rc == AVERROR_EOF)
                return nullptr;
            if (rc != AVERROR(EAGAIN))
                avcheck(rc);
            auto p = next(audio);
            if (!p && audio_drained)
                return nullptr;
            avcheck(avcodec_send_packet(dec.codec, p.get()));
            if (!p)
                audio_drained = true;
        }
        auto f = dec.frame;
        if (!dec.swr)
        {
            avcheck(swr_alloc_set_opts2(&dec.swr, &dec.layout, AV_SAMPLE_FMT_FLT, rate, &f->ch_layout,
                                        static_cast<AVSampleFormat>(f->format), f->sample_rate, 0, nullptr));
            avcheck(swr_init(dec.swr));
        }
        int n = swr_get_out_samples(dec.swr, f->nb_samples);
        avcheck(n);
        Buffer b(n * dec.layout.nb_channels * sizeof(float));
        uint8_t *dst = b.data();
        n = swr_convert(dec.swr, &dst, n, const_cast<const uint8_t **>(f->extended_data), f->nb_samples);
        avcheck(n);
        b.Length(n * dec.layout.nb_channels * sizeof(float));
        int64_t pts = f->best_effort_timestamp == AV_NOPTS_VALUE ? next_audio : stamp(f->best_effort_timestamp, audio);
        int64_t duration = av_rescale_q(n, {1, rate}, ticks);
        next_audio = pts + duration;
        av_frame_unref(f);
        auto s = MediaStreamSample::CreateFromBuffer(b, TimeSpan{pts});
        s.Duration(TimeSpan{duration});
        s.Discontinuous(std::exchange(first_audio, false));
        return s;
    }
    void seek(int64_t position)
    {
        int64_t timestamp = av_rescale_q(position + origin, ticks, input.p->streams[video]->time_base);
        avcheck(av_seek_frame(input.p, video, timestamp, AVSEEK_FLAG_BACKWARD));
        video_q.clear();
        audio_q.clear();
        queued = 0;
        eof = false;
        audio_drained = false;
        pending_picture.reset();
        mel_mappings = 0;
        av_bsf_flush(dec.bsf);
        if (dec.codec)
            avcodec_flush_buffers(dec.codec);
        swr_free(&dec.swr);
        next_audio = position;
        first_picture = first_audio = true;
        ++state->generation;
    }
    void raw_loop() noexcept
    {
        try
        {
            winrt::init_apartment(winrt::apartment_type::multi_threaded);
            unsigned gen = state->generation;
            while (!stop_audio && !state->closed)
            {
                if (gen != state->generation)
                {
                    sink->reset();
                    gen = state->generation;
                }
                if (!state->clock.read().second)
                {
                    sink->pause();
                    std::this_thread::sleep_for(5ms);
                    continue;
                }
                Packet p;
                int64_t pts{};
                {
                    std::lock_guard lock(mutex);
                    if (gen != state->generation)
                        continue;
                    p = next(audio);
                    if (p)
                    {
                        auto timestamp = p->pts != AV_NOPTS_VALUE ? p->pts : p->dts;
                        if (timestamp == AV_NOPTS_VALUE)
                            winrt::throw_hresult(MF_E_INVALID_TIMESTAMP);
                        pts = stamp(timestamp, audio);
                    }
                }
                if (!p)
                {
                    std::this_thread::sleep_for(5ms);
                    continue;
                }
                auto burst = sink->pack(p.get(), pts);
                if (!burst.empty())
                    sink->write(burst, pts, state->clock, state->closed, state->generation, gen);
            }
        }
        catch (...)
        {
            if (!state->closed)
                state->failed = true;
        }
    }

  public:
    explicit Source(std::shared_ptr<State> s) : state(std::move(s))
    {
    }
    ~Source()
    {
        stop_audio = true;
        ++state->generation;
        if (raw_thread.joinable())
            raw_thread.join();
    }
    void open_video(const std::wstring &url, const std::wstring &expected, int track, bool probe)
    {
        input.p = avformat_alloc_context();
        if (!input.p || !dec.frame)
            throw std::bad_alloc();
        input.p->interrupt_callback = {interrupted, state.get()};
        AVDictionary *options{};
        input.open(url);
        {
            std::lock_guard lock(state->io_mutex);
            if (state->closed)
                winrt::throw_hresult(MF_E_SHUTDOWN);
            state->io = input.bytes;
        }
        av_dict_set(&options, "format_whitelist", "matroska,webm,mov,mp4,m4a,3gp,3g2,mj2,mpegts", 0);
        av_dict_set(&options, "protocol_whitelist", "", 0);
        int opened = avformat_open_input(&input.p, nullptr, nullptr, &options);
        av_dict_free(&options);
        avcheck(opened);
        avcheck(avformat_find_stream_info(input.p, nullptr));
        if (!probe)
            input.configure_buffering();
        video = av_find_best_stream(input.p, AVMEDIA_TYPE_VIDEO, -1, -1, nullptr, 0);
        avcheck(video);
        audio = track >= 0 ? track : av_find_best_stream(input.p, AVMEDIA_TYPE_AUDIO, -1, video, nullptr, 0);
        if (audio >= 0 && (static_cast<unsigned>(audio) >= input.p->nb_streams ||
                           input.p->streams[audio]->codecpar->codec_type != AVMEDIA_TYPE_AUDIO))
            winrt::throw_hresult(MF_E_INVALIDSTREAMNUMBER);
        auto vs = input.p->streams[video];
        auto p = vs->codecpar;
        auto sd = av_packet_side_data_get(p->coded_side_data, p->nb_coded_side_data, AV_PKT_DATA_DOVI_CONF);
        if (!sd || sd->size < 8)
        {
            if (state->convert_mel)
                reject_mel(4);
            winrt::throw_hresult(MF_E_INVALIDMEDIATYPE);
        }
        auto dv = reinterpret_cast<const AVDOVIDecoderConfigurationRecord *>(sd->data);
        bool hevc = p->codec_id == AV_CODEC_ID_HEVC;
        if (!hevc && p->codec_id != AV_CODEC_ID_H264)
            winrt::throw_hresult(MF_E_INVALIDMEDIATYPE);
        wchar_t profile[16]{};
        swprintf_s(profile, L"%ls.%02u", hevc ? L"dvhe" : L"dvav", dv->dv_profile);
        if (state->convert_mel)
        {
            for (unsigned i = 0; i < input.p->nb_streams; ++i)
                if (i != static_cast<unsigned>(video) && input.p->streams[i]->codecpar->codec_type == AVMEDIA_TYPE_VIDEO &&
                    !(input.p->streams[i]->disposition & AV_DISPOSITION_ATTACHED_PIC))
                    reject_mel(3);
            if (!hevc || dv->dv_version_major != 1 || dv->dv_profile != 7 || !dv->rpu_present_flag ||
                !dv->bl_present_flag || !dv->el_present_flag || dv->dv_bl_signal_compatibility_id != 6)
                reject_mel(4);
        }
        else if ((!expected.empty() && expected != profile) || !dv->rpu_present_flag || dv->el_present_flag || dv->dv_profile == 7)
            winrt::throw_hresult(MF_E_INVALIDMEDIATYPE);
        auto filter = av_bsf_get_by_name(hevc ? "hevc_mp4toannexb" : "h264_mp4toannexb");
        if (!filter)
            winrt::throw_hresult(MF_E_TOPO_CODEC_NOT_FOUND);
        avcheck(av_bsf_alloc(filter, &dec.bsf));
        avcheck(avcodec_parameters_copy(dec.bsf->par_in, p));
        dec.bsf->time_base_in = vs->time_base;
        avcheck(av_bsf_init(dec.bsf));
        if (state->convert_mel)
        {
            pending_picture = read_picture();
            if (!pending_picture)
                reject_mel(2);
            if (probe)
                return;
            auto par = dec.bsf->par_out;
            auto configuration = par->extradata_size ? convert_mel(par->extradata, par->extradata_size, true)
                                                     : convert_mel(pending_picture->data, pending_picture->size, true);
            picture = VideoStreamDescriptor(video_properties(vs, par, pending_picture.get(), L"dvhe.08", 8,
                                                             configuration.get()));
        }
        else
        {
            auto first = next(video);
            if (!first)
                winrt::throw_hresult(MF_E_INVALID_STREAM_DATA);
            picture = VideoStreamDescriptor(video_properties(vs, p, first.get(), profile, dv->dv_profile));
            queued += first->size;
            video_q.push_front(std::move(first));
        }
    }
    com_ptr<IMFMediaSource> open(const std::wstring &url, const std::wstring &expected, int track, bool allow_raw)
    {
        open_video(url, expected, track, false);
        if (audio >= 0)
        {
            auto as = input.p->streams[audio];
            auto ap = as->codecpar;
            com_ptr<IMMDeviceEnumerator> devices;
            com_ptr<IMMDevice> device;
            if (SUCCEEDED(
                    CoCreateInstance(__uuidof(MMDeviceEnumerator), nullptr, CLSCTX_ALL, IID_PPV_ARGS(devices.put()))))
                devices->GetDefaultAudioEndpoint(eRender, eMultimedia, device.put());
            LPWSTR id{};
            if (device && SUCCEEDED(device->GetId(&id)))
            {
                std::lock_guard lock(state->endpoint_mutex);
                state->endpoint = id;
                state->have_endpoint = true;
                CoTaskMemFree(id);
            }
            auto output = std::make_unique<Bitstream>();
            bool bitstream_ready = false;
            if (allow_raw)
                try
                {
                    bitstream_ready = output->open(device.get(), as);
                }
                catch (const winrt::hresult_error &)
                {
                    bitstream_ready = false;
                }
            if (bitstream_ready)
            {
                sink = std::move(output);
                state->raw = true;
            }
            else
            {
                output.reset();
                const auto codec = avcodec_find_decoder(ap->codec_id);
                if (!codec)
                    winrt::throw_hresult(MF_E_TOPO_CODEC_NOT_FOUND);
                dec.codec = avcodec_alloc_context3(codec);
                if (!dec.codec)
                    throw std::bad_alloc();
                avcheck(avcodec_parameters_to_context(dec.codec, ap));
                dec.codec->pkt_timebase = as->time_base;
                avcheck(avcodec_open2(dec.codec, codec, nullptr));
                rate = ap->sample_rate;
                int channels = ap->ch_layout.nb_channels;
                com_ptr<IAudioClient> mix;
                WAVEFORMATEX *fmt{};
                if (device &&
                    SUCCEEDED(device->Activate(__uuidof(IAudioClient), CLSCTX_ALL, nullptr, mix.put_void())) &&
                    SUCCEEDED(mix->GetMixFormat(&fmt)))
                {
                    channels = (std::min)(channels, int(fmt->nChannels));
                    CoTaskMemFree(fmt);
                }
                if (channels < 1 || channels > 32 || rate < 1)
                    winrt::throw_hresult(MF_E_INVALIDMEDIATYPE);
                if (channels == ap->ch_layout.nb_channels && ap->ch_layout.order == AV_CHANNEL_ORDER_NATIVE)
                    avcheck(av_channel_layout_copy(&dec.layout, &ap->ch_layout));
                else
                    av_channel_layout_default(&dec.layout, channels);
                auto a = AudioEncodingProperties::CreatePcm(rate, channels, 32);
                a.Subtype(L"Float");
                if (dec.layout.order == AV_CHANNEL_ORDER_NATIVE)
                    a.Properties().Insert(MF_MT_AUDIO_CHANNEL_MASK, winrt::box_value(uint32_t(dec.layout.u.mask)));
                sound = AudioStreamDescriptor(a);
                state->channels = channels;
            }
        }
        origin = input.p->start_time == AV_NOPTS_VALUE ? 0 : av_rescale_q(input.p->start_time, AV_TIME_BASE_Q, ticks);
        mss = sound ? MediaStreamSource(picture, sound) : MediaStreamSource(picture);
        mss.BufferTime(TimeSpan{0});
        if (input.p->duration > 0)
        {
            mss.Duration(TimeSpan{av_rescale_q(input.p->duration, AV_TIME_BASE_Q, ticks)});
            mss.CanSeek(true);
        }
        auto weak = weak_from_this();
        mss.Starting([weak](auto const &sender, auto const &args) {
            if (auto self = weak.lock())
                try
                {
                    std::lock_guard lock(self->mutex);
                    auto r = args.Request();
                    auto p = r.StartPosition();
                    if (p)
                    {
                        auto t = p.Value().count();
                        if (t != 0 || !self->first_picture)
                            self->seek(t);
                        r.SetActualStartPosition(TimeSpan{t});
                    }
                }
                catch (...)
                {
                    sender.NotifyError(MediaStreamSourceErrorStatus::Other);
                }
        });
        mss.SampleRequested([weak](auto const &sender, auto const &args) {
            if (auto self = weak.lock())
                try
                {
                    std::lock_guard lock(self->mutex);
                    auto r = args.Request();
                    if (self->state->closed)
                    {
                        r.Sample(nullptr);
                        return;
                    }
                    r.Sample(r.StreamDescriptor().template try_as<VideoStreamDescriptor>() ? self->video_sample()
                                                                                           : self->audio_sample());
                }
                catch (...)
                {
                    if (!self->state->closed)
                        sender.NotifyError(MediaStreamSourceErrorStatus::DecodeError);
                }
        });
        com_ptr<IMFMediaSource> result;
        check_hresult(mss.as<IMFGetService>()->GetService(MF_MEDIASOURCE_SERVICE, IID_PPV_ARGS(result.put())));
        if (sink)
            raw_thread = std::thread([this] { raw_loop(); });
        return result;
    }
};

struct Extension : winrt::implements<Extension, IMFMediaEngineExtension>
{
    std::wstring url;
    std::wstring profile;
    int audio;
    bool allow_raw;
    std::shared_ptr<State> state = std::make_shared<State>();
    std::atomic_bool opening{false};
    std::shared_ptr<Source> source;
    Extension(const wchar_t *u, const wchar_t *p, int a, bool raw) : url(u), profile(p), audio(a), allow_raw(raw)
    {
    }
    HRESULT __stdcall CanPlayType(BOOL, BSTR, MF_MEDIA_ENGINE_CANPLAY *answer) noexcept final
    {
        if (!answer)
            return E_POINTER;
        *answer = MF_MEDIA_ENGINE_CANPLAY_NOT_SUPPORTED;
        return S_OK;
    }
    HRESULT __stdcall BeginCreateObject(BSTR, IMFByteStream *, MF_OBJECT_TYPE kind, IUnknown **cookie,
                                        IMFAsyncCallback *cb, IUnknown *state_arg) noexcept final
    {
        if (cookie)
            *cookie = nullptr;
        if (kind != MF_OBJECT_MEDIASOURCE)
            return MF_E_UNSUPPORTED_BYTESTREAM_TYPE;
        if (opening.exchange(true))
            return MF_E_INVALIDREQUEST;
        try
        {
            auto self = get_strong();
            com_ptr<IMFAsyncCallback> callback;
            callback.copy_from(cb);
            com_ptr<IUnknown> context;
            context.copy_from(state_arg);
            std::thread([self, callback, context] {
                com_ptr<IMFMediaSource> media;
                HRESULT status = S_OK;
                try
                {
                    winrt::init_apartment(winrt::apartment_type::multi_threaded);
                    if (self->state->closed)
                        winrt::throw_hresult(MF_E_SHUTDOWN);
                    auto src = std::make_shared<Source>(self->state);
                    media = src->open(self->url, self->profile, self->audio, self->allow_raw);
                    self->source = std::move(src);
                }
                catch (...)
                {
                    status = winrt::to_hresult();
                }
                com_ptr<IMFAsyncResult> result;
                if (SUCCEEDED(MFCreateAsyncResult(media.get(), callback.get(), context.get(), result.put())))
                {
                    result->SetStatus(status);
                    MFInvokeCallback(result.get());
                }
            }).detach();
            return S_OK;
        }
        catch (...)
        {
            return winrt::to_hresult();
        }
    }
    HRESULT __stdcall CancelObjectCreation(IUnknown *) noexcept final
    {
        state->cancel();
        return S_OK;
    }
    HRESULT __stdcall EndCreateObject(IMFAsyncResult *r, IUnknown **object) noexcept final
    {
        if (!r || !object)
            return E_POINTER;
        *object = nullptr;
        auto hr = r->GetStatus();
        return FAILED(hr) ? hr : r->GetObject(object);
    }
};
} // namespace rf
// Inspect one compressed picture using the same configuration and RPU checks
// used during native playback. No decoder, renderer or audio endpoint is opened.
extern "C" __declspec(dllexport) HRESULT rf_probe_layer(const wchar_t *url, rf::ConvertMel convert, int *layer_error) noexcept
{
    if (!url || !convert || !layer_error)
        return E_INVALIDARG;
    *layer_error = 0;
    try
    {
        av_log_set_level(AV_LOG_QUIET);
        auto state = std::make_shared<rf::State>();
        state->convert_mel = convert;
        rf::Source source(state);
        try
        {
            source.open_video(url, L"", -1, true);
        }
        catch (...)
        {
            *layer_error = state->mel_error.load();
            if (!*layer_error)
                throw;
        }
        return S_OK;
    }
    catch (...)
    {
        return winrt::to_hresult();
    }
}
extern "C" __declspec(dllexport) HRESULT rf_create_v2(const wchar_t *url, const wchar_t *profile, int audio, int raw,
                                                   rf::ConvertMel convert_mel, IMFMediaEngineExtension **ext) noexcept
{
    try
    {
        av_log_set_level(AV_LOG_QUIET);
        auto p = winrt::make_self<rf::Extension>(url, profile, audio, raw != 0);
        p->state->convert_mel = convert_mel;
        *ext = p.as<IMFMediaEngineExtension>().detach();
        return S_OK;
    }
    catch (...)
    {
        return winrt::to_hresult();
    }
}
// Keep already-running players compatible when the media module is updated.
extern "C" __declspec(dllexport) HRESULT rf_create(const wchar_t *url, const wchar_t *profile, int audio, int raw,
                                                   IMFMediaEngineExtension **ext) noexcept
{
    return rf_create_v2(url, profile, audio, raw, nullptr, ext);
}
extern "C" __declspec(dllexport) int rf_mel_error(IMFMediaEngineExtension *ext) noexcept
{
    return static_cast<rf::Extension *>(ext)->state->mel_error.load();
}
extern "C" __declspec(dllexport) int rf_clock(IMFMediaEngineExtension *ext, double pos, int active) noexcept
{
    auto state = static_cast<rf::Extension *>(ext)->state;
    state->clock.update(pos, active != 0);
    if (state->failed)
        return -2;
    return state->raw ? -1 : state->channels.load();
}
extern "C" __declspec(dllexport) void rf_close(IMFMediaEngineExtension *ext) noexcept
{
    static_cast<rf::Extension *>(ext)->state->cancel();
}

extern "C" __declspec(dllexport) int rf_endpoint_changed(IMFMediaEngineExtension *ext) noexcept
{
    try
    {
        return static_cast<rf::Extension *>(ext)->state->changed();
    }
    catch (...)
    {
        return 0;
    }
}
