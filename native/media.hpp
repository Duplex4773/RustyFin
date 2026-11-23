#pragma once
#include <windows.h>


#include <algorithm>
#include <atomic>
#include <audioclient.h>
#include <chrono>
#include <deque>
#include <ks.h>
#include <ksmedia.h>
#include <memory>
#include <mfapi.h>
#include <mferror.h>
#include <mfidl.h>
#include <mfmediaengine.h>
#include <mmdeviceapi.h>
#include <mmreg.h>
#include <mutex>
#include <thread>
#include <vector>
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.Media.Core.h>
#include <winrt/Windows.Media.MediaProperties.h>
#include <winrt/Windows.Storage.Streams.h>
extern "C"
{
#include <libavcodec/avcodec.h>
#include <libavcodec/bsf.h>
#include <libavformat/avformat.h>
#include <libavutil/dovi_meta.h>
#include <libavutil/opt.h>
#include <libswresample/swresample.h>
}
namespace rf
{
using winrt::check_hresult;
using winrt::com_ptr;
using namespace std::chrono_literals;
inline void avcheck(int result)
{
    if (result < 0)
        winrt::throw_hresult(MF_E_INVALID_STREAM_DATA);
}
struct PacketFree
{
    void operator()(AVPacket *p) const
    {
        av_packet_free(&p);
    }
};
using Packet = std::unique_ptr<AVPacket, PacketFree>;
inline Packet packet()
{
    Packet p(av_packet_alloc());
    if (!p)
        throw std::bad_alloc();
    return p;
}
inline constexpr AVRational ticks{1, 10000000};
struct Format
{
    AVFormatContext *p{};
    AVIOContext *io{};
    com_ptr<IMFByteStream> bytes;
    void open(const std::wstring &url);
    void configure_buffering();
    ~Format()
    {
        avformat_close_input(&p);
        if (io)
        {
            av_freep(&io->buffer);
            avio_context_free(&io);
        }
        if (bytes)
            bytes->Close();
    }
};
struct Clock
{
    std::mutex mutex;
    double position{};
    bool playing{};
    std::chrono::steady_clock::time_point updated{};
    void update(double value, bool active)
    {
        std::lock_guard lock(mutex);
        position = value;
        playing = active;
        updated = std::chrono::steady_clock::now();
    }
    std::pair<double, bool> read()
    {
        std::lock_guard lock(mutex);
        auto elapsed = std::chrono::duration<double>(std::chrono::steady_clock::now() - updated).count();
        return {position + (playing ? (std::min)(elapsed, 0.5) : 0), playing && elapsed < 0.7};
    }
};
winrt::Windows::Media::MediaProperties::VideoEncodingProperties video_properties(AVStream *, const AVCodecParameters *, AVPacket *,
                                                                                 const wchar_t *, unsigned, const AVPacket *mel_config = nullptr);
class Bitstream
{
    com_ptr<IAudioClient> client;
    com_ptr<IAudioRenderClient> render;
    AVFormatContext *mux{};
    AVCodecParameters *params{};
    AVRational input_timebase{};
    bool dts_hd{};
    void open_packer();
    void close_packer() noexcept;
    UINT32 capacity{}, frame_bytes{}, rate{};
    bool running{};
    int64_t pending_pts{AV_NOPTS_VALUE};

  public:
    ~Bitstream();
    bool open(IMMDevice *device, AVStream *stream);
    std::vector<uint8_t> pack(AVPacket *packet, int64_t &pts);
    void reset();
    void pause();
    bool write(const std::vector<uint8_t> &bytes, int64_t pts, Clock &clock, const std::atomic_bool &closed,
               const std::atomic_uint &epoch, unsigned generation);
};
} // namespace rf
