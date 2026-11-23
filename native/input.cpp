#include "media.hpp"
namespace rf
{
// Windows owns HTTP, TLS trust and ranged reads. libavformat receives a
// seekable byte stream and never opens network or ancillary file URLs itself.
void Format::open(const std::wstring &url)
{
    com_ptr<IMFSourceResolver> resolver;
    check_hresult(MFCreateSourceResolver(resolver.put()));
    MF_OBJECT_TYPE kind{};
    com_ptr<IUnknown> object;
    check_hresult(resolver->CreateObjectFromURL(url.c_str(), MF_RESOLUTION_BYTESTREAM | MF_RESOLUTION_READ, nullptr,
                                                &kind, object.put()));
    bytes = object.as<IMFByteStream>();
    constexpr int size = 128 * 1024;
    auto data = static_cast<uint8_t *>(av_malloc(size));
    if (!data)
        throw std::bad_alloc();
    io = avio_alloc_context(
        data, size, 0, this,
        [](void *f, uint8_t *dst, int size) -> int {
            ULONG n{};
            auto self = static_cast<Format *>(f);
            if (self->p && self->p->interrupt_callback.callback(self->p->interrupt_callback.opaque))
                return AVERROR_EXIT;
            if (FAILED(self->bytes->Read(dst, size, &n)))
                return AVERROR(EIO);
            return n ? static_cast<int>(n) : AVERROR_EOF;
        },
        nullptr,
        [](void *f, int64_t offset, int whence) -> int64_t {
            auto &stream = static_cast<Format *>(f)->bytes;
            QWORD pos{};
            whence &= ~AVSEEK_FORCE;
            if (whence == AVSEEK_SIZE)
            {
                return SUCCEEDED(stream->GetLength(&pos)) && pos <= INT64_MAX ? static_cast<int64_t>(pos)
                                                                              : AVERROR(ENOSYS);
            }
            if (whence == SEEK_CUR)
            {
                if (FAILED(stream->GetCurrentPosition(&pos)))
                    return AVERROR(EIO);
            }
            else if (whence == SEEK_END)
            {
                if (FAILED(stream->GetLength(&pos)))
                    return AVERROR(EIO);
            }
            else if (whence != SEEK_SET)
                return AVERROR(EINVAL);
            if (pos > INT64_MAX || (offset > 0 && pos > uint64_t(INT64_MAX - offset)) ||
                offset < -static_cast<int64_t>(pos))
                return AVERROR(EINVAL);
            auto target = static_cast<int64_t>(pos) + offset;
            return SUCCEEDED(stream->SetCurrentPosition(target)) ? target : AVERROR(EIO);
        });
    if (!io)
    {
        av_free(data);
        throw std::bad_alloc();
    }
    DWORD caps{};
    check_hresult(bytes->GetCapabilities(&caps));
    io->seekable = (caps & MFBYTESTREAM_IS_SEEKABLE) ? AVIO_SEEKABLE_NORMAL : 0;
    p->pb = io;
    p->flags |= AVFMT_FLAG_CUSTOM_IO;
}

void Format::configure_buffering()
{
    auto buffering = bytes.try_as<IMFByteStreamBuffering>();
    if (!buffering || p->bit_rate <= 0)
        return;

    // Windows downloads ahead for the whole interleaved file. Its default
    // rate can starve the decoder once the initial HTTP buffer runs out.
    MF_LEAKY_BUCKET_PAIR bucket{static_cast<DWORD>((std::min)(p->bit_rate, int64_t(UINT32_MAX))), 3000};
    MFBYTESTREAM_BUFFERING_PARAMS params{};
    QWORD length{};
    params.cbTotalFileSize = SUCCEEDED(bytes->GetLength(&length)) ? length : UINT64_MAX;
    params.cbPlayableDataSize = UINT64_MAX;
    params.prgBuckets = &bucket;
    params.cBuckets = 1;
    params.qwPlayDuration = p->duration > 0 ? av_rescale_q(p->duration, AV_TIME_BASE_Q, ticks) : 0;
    params.dRate = 1.0f;
    if (SUCCEEDED(buffering->SetBufferingParams(&params)))
        buffering->EnableBuffering(TRUE);
}
} // namespace rf
