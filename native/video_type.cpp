#include "media.hpp"
#include <shlwapi.h>
namespace rf
{
// Ask the Windows MP4 demuxer for its actual decoder and Dolby renderer
// attributes, including which NAL units to forward. Only a single compressed
// picture and its matching codec configuration are wrapped, entirely in memory.
winrt::Windows::Media::MediaProperties::VideoEncodingProperties video_properties(AVStream *input, const AVCodecParameters *parameters, AVPacket *first,
                                                                                 const wchar_t *profile,
                                                                                 unsigned dv_profile, const AVPacket *mel_config)
{
    struct Probe
    {
        AVFormatContext *mux{};
        ~Probe()
        {
            if (!mux)
                return;
            if (mux->pb)
            {
                uint8_t *bytes{};
                avio_close_dyn_buf(mux->pb, &bytes);
                av_free(bytes);
            }
            avformat_free_context(mux);
        }
    } probe;
    avcheck(avformat_alloc_output_context2(&probe.mux, nullptr, "mp4", nullptr));
    auto m = probe.mux;
    avcheck(avio_open_dyn_buf(&m->pb));
    auto s = avformat_new_stream(m, nullptr);
    if (!s)
        throw std::bad_alloc();
    avcheck(avcodec_parameters_copy(s->codecpar, parameters));
    if (mel_config)
    {
        // Only change the in-memory MP4 description. The demuxer and Annex B
        // filter retain their original Profile 7 input and parameter sets.
        auto par = s->codecpar;
        auto record = av_packet_side_data_get(par->coded_side_data, par->nb_coded_side_data, AV_PKT_DATA_DOVI_CONF);
        if (!record || record->size < 8)
            winrt::throw_hresult(MF_E_INVALIDMEDIATYPE);
        auto single = reinterpret_cast<AVDOVIDecoderConfigurationRecord *>(record->data);
        single->dv_profile = 8;
        single->el_present_flag = 0;
        single->dv_bl_signal_compatibility_id = 1;
        av_freep(&par->extradata);
        par->extradata_size = mel_config->size;
        par->extradata = static_cast<uint8_t *>(av_mallocz(size_t(mel_config->size) + AV_INPUT_BUFFER_PADDING_SIZE));
        if (!par->extradata)
            throw std::bad_alloc();
        memcpy(par->extradata, mel_config->data, mel_config->size);
    }
    s->time_base = input->time_base;
    s->codecpar->codec_tag = s->codecpar->codec_id == AV_CODEC_ID_HEVC
                                 ? (dv_profile == 5 ? MKTAG('d', 'v', 'h', '1') : MKTAG('h', 'v', 'c', '1'))
                                 : MKTAG('a', 'v', 'c', '1');
    m->strict_std_compliance = FF_COMPLIANCE_UNOFFICIAL;
    avcheck(avformat_write_header(m, nullptr));
    auto one = packet();
    avcheck(av_packet_ref(one.get(), first));
    one->stream_index = 0;
    one->dts = one->pts = 0;
    one->duration = (std::max)(int64_t(1), av_rescale_q(first->duration, input->time_base, s->time_base));
    avcheck(av_write_frame(m, one.get()));
    avcheck(av_write_trailer(m));
    avio_flush(m->pb);
    uint8_t *bytes{};
    int length = avio_get_dyn_buf(m->pb, &bytes);
    avcheck(length);
    com_ptr<IStream> memory;
    memory.attach(SHCreateMemStream(bytes, length));
    if (!memory)
        throw std::bad_alloc();
    com_ptr<IMFByteStream> stream;
    check_hresult(MFCreateMFByteStreamOnStream(memory.get(), stream.put()));
    com_ptr<IMFSourceResolver> resolver;
    check_hresult(MFCreateSourceResolver(resolver.put()));
    MF_OBJECT_TYPE kind{};
    com_ptr<IUnknown> object;
    check_hresult(resolver->CreateObjectFromByteStream(stream.get(), L"sample.mp4", MF_RESOLUTION_MEDIASOURCE, nullptr,
                                                       &kind, object.put()));
    auto source = object.as<IMFMediaSource>();
    struct Shutdown
    {
        IMFMediaSource *s;
        ~Shutdown()
        {
            s->Shutdown();
        }
    } shutdown{source.get()};
    com_ptr<IMFPresentationDescriptor> pd;
    com_ptr<IMFStreamDescriptor> sd;
    com_ptr<IMFMediaTypeHandler> handler;
    com_ptr<IMFMediaType> type;
    BOOL selected;
    check_hresult(source->CreatePresentationDescriptor(pd.put()));
    check_hresult(pd->GetStreamDescriptorByIndex(0, &selected, sd.put()));
    check_hresult(sd->GetMediaTypeHandler(handler.put()));
    check_hresult(handler->GetCurrentMediaType(type.put()));
    wchar_t actual[24]{};
    check_hresult(type->GetString(MF_MT_VIDEO_RENDERER_EXTENSION_PROFILE, actual, 24, nullptr));
    if (wcscmp(profile, actual) != 0)
        winrt::throw_hresult(MF_E_INVALIDMEDIATYPE);
    winrt::Windows::Media::MediaProperties::VideoEncodingProperties v{nullptr};
    check_hresult(MFCreatePropertiesFromMediaType(
        type.get(), winrt::guid_of<winrt::Windows::Media::MediaProperties::IVideoEncodingProperties>(),
        winrt::put_abi(v)));
    return v;
}
} // namespace rf
