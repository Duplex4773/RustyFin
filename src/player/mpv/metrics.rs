use super::*;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum Metric {
    VideoCodec,
    Width,
    Height,
    SourceFps,
    Decoder,
    AspectName,
    Aspect,
    AudioCodec,
    SampleRate,
    Channels,
    AudioBitrate,
    PixelFormat,
    HardwareFormat,
    Matrix,
    Primaries,
    Transfer,
    FilterFps,
    DisplayFps,
    AvSync,
    DroppedFrames,
    BufferDuration,
    CacheUsed,
    CacheReadAhead,
    CacheLimit,
    BackCacheLimit,
    CacheSpeed,
}

const REPLY_BASE: u64 = 100;
const PROPERTIES: &[(Metric, &CStr)] = &[
    (Metric::VideoCodec, c"current-tracks/video/codec"),
    (Metric::Width, c"video-params/w"),
    (Metric::Height, c"video-params/h"),
    (Metric::SourceFps, c"container-fps"),
    (Metric::Decoder, c"hwdec-current"),
    (Metric::AspectName, c"video-params/aspect-name"),
    (Metric::Aspect, c"video-params/aspect"),
    (Metric::AudioCodec, c"current-tracks/audio/codec"),
    (Metric::SampleRate, c"audio-params/samplerate"),
    (Metric::Channels, c"audio-params/channels"),
    (Metric::AudioBitrate, c"audio-bitrate"),
    (Metric::PixelFormat, c"video-params/pixelformat"),
    (Metric::HardwareFormat, c"video-params/hw-pixelformat"),
    (Metric::Matrix, c"video-params/colormatrix"),
    (Metric::Primaries, c"video-params/primaries"),
    (Metric::Transfer, c"video-params/gamma"),
    (Metric::FilterFps, c"estimated-vf-fps"),
    (Metric::DisplayFps, c"display-fps"),
    (Metric::AvSync, c"avsync"),
    (Metric::DroppedFrames, c"frame-drop-count"),
    (Metric::BufferDuration, c"demuxer-cache-duration"),
    (Metric::CacheUsed, c"demuxer-cache-state"),
    (Metric::CacheLimit, c"options/demuxer-max-bytes"),
    (Metric::BackCacheLimit, c"options/demuxer-max-back-bytes"),
    (Metric::CacheSpeed, c"cache-speed"),
];

#[derive(Default)]
pub(super) struct Metrics {
    at: Option<Instant>,
    pending: usize,
    values: HashMap<Metric, String>,
}

impl Mpv {
    pub(in crate::player) fn probe_performance(&mut self) {
        let metrics = &mut self.metrics;
        if metrics.pending > 0
            || metrics
                .at
                .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        metrics.at = Some(Instant::now());
        for (i, &(metric, property)) in PROPERTIES.iter().enumerate() {
            let format = if metric == Metric::CacheUsed {
                FORMAT_NODE
            } else {
                FORMAT_STRING
            };
            let status = unsafe {
                (self.property_async)(
                    self.handle.as_ptr(),
                    REPLY_BASE + i as u64,
                    property.as_ptr(),
                    format,
                )
            };
            if status >= 0 {
                metrics.pending += 1;
            } else {
                metrics.values.remove(&metric);
                if metric == Metric::CacheUsed {
                    metrics.values.remove(&Metric::CacheReadAhead);
                }
            }
        }
    }

    pub(super) fn performance_reply(&mut self, event: &Event) {
        let Some((metric, _)) = event
            .reply
            .checked_sub(REPLY_BASE)
            .and_then(|i| PROPERTIES.get(i as usize))
        else {
            return;
        };
        self.metrics.pending = self.metrics.pending.saturating_sub(1);
        if *metric == Metric::CacheUsed {
            self.metrics.cache_reply(event);
            return;
        }
        match event
            .property_string()
            .map(CStr::to_string_lossy)
            .filter(|s| !s.is_empty() && s.as_ref() != "unknown")
        {
            Some(value) => {
                self.metrics.values.insert(*metric, value.into_owned());
            }
            None => {
                self.metrics.values.remove(metric);
            }
        }
    }

    pub(in crate::player) fn metric(&self, metric: Metric) -> Option<&str> {
        self.metrics.values.get(&metric).map(String::as_str)
    }
}

impl Metrics {
    fn cache_reply(&mut self, event: &Event) {
        self.values.remove(&Metric::CacheUsed);
        self.values.remove(&Metric::CacheReadAhead);
        if event.error < 0 || event.data.is_null() {
            return;
        }
        // libmpv owns the returned node and all of its children until the next event.
        let property = unsafe { &*event.data.cast::<EventProperty>() };
        if property.format != FORMAT_NODE || property.data.is_null() {
            return;
        }
        let node = unsafe { &*property.data.cast::<Node>() };
        if node.format != FORMAT_NODE_MAP {
            return;
        }
        let Some(list) = (unsafe { node.value.list.as_ref() }) else {
            return;
        };
        for index in 0..list.count.max(0) as usize {
            let entry = unsafe { &*list.values.add(index) };
            if entry.format != FORMAT_INT64 {
                continue;
            }
            let key = unsafe { CStr::from_ptr(*list.keys.add(index)) };
            let metric = match key.to_bytes() {
                b"total-bytes" => Metric::CacheUsed,
                b"fw-bytes" => Metric::CacheReadAhead,
                _ => continue,
            };
            let bytes = unsafe { entry.value.integer };
            if bytes >= 0 {
                self.values.insert(metric, bytes.to_string());
            }
        }
    }
}
