use super::*;
use crate::{
    player::{Metric, ShaderStatus},
    preferences::Shader,
};

fn metric_group<const N: usize>(
    name: &'static str,
    fields: [(&'static str, Option<String>); N],
) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(3.))
        .child(title(name, 13.).mb(px(2.)))
        .children(fields.into_iter().map(|(key, value)| {
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .child(label(key, 12., MUTED).flex_none())
                .child(
                    label(value.unwrap_or_else(|| "—".into()), 12., TEXT)
                        .min_w_0()
                        .truncate()
                        .font_weight(FontWeight::SEMIBOLD),
                )
        }))
}

fn fps(value: f64) -> String {
    format!("{value:.2}")
}
fn megabytes(bytes: f64) -> String {
    format!("{:.1} MiB", bytes / 1_048_576.)
}

impl Library {
    pub(super) fn performance_overlay(&self, p: &Player, width: f32) -> Div {
        let measured = &p.performance;
        let value = |key| p.metric(key).map(str::to_owned);
        let number = |key| {
            p.metric(key)
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|v| v.is_finite())
        };
        let video = p
            .source
            .media_streams
            .iter()
            .flatten()
            .find(|s| s.kind == "Video" && !s.is_attached_pic);
        let audio = p
            .source
            .selected_audio(self.controls.choice.audio)
            .ok()
            .flatten();
        let resolution = number(Metric::Width)
            .zip(number(Metric::Height))
            .map(|(w, h)| (w as u32, h as u32))
            .or_else(|| video.and_then(|v| v.width.zip(v.height)))
            .filter(|&(w, h)| w > 0 && h > 0)
            .map(|(w, h)| format!("{w} × {h}"));
        let decoder = value(Metric::Decoder)
            .map(|s| if s == "no" { "Software".into() } else { s })
            .or_else(|| (!p.mpv() && p.status.ready).then(|| "Media Foundation".into()));
        let cache_limit = number(Metric::CacheLimit)
            .map(|forward| forward + number(Metric::BackCacheLimit).unwrap_or(0.));
        let group = |body: Div| body.px_2().py_2().rounded_md().bg(rgba(0xffffff08));
        let media_row = div()
            .flex()
            .gap_2()
            .child(group(metric_group(
                "Video",
                [
                    (
                        "Codec",
                        value(Metric::VideoCodec)
                            .map(|s| s.to_uppercase())
                            .or_else(|| video.and_then(|s| s.codec_name())),
                    ),
                    ("Resolution", resolution),
                    (
                        "FPS",
                        number(Metric::SourceFps)
                            .or_else(|| video.and_then(|s| s.frame_rate()))
                            .map(fps),
                    ),
                    ("Decoder", decoder),
                    (
                        "Aspect",
                        value(Metric::AspectName)
                            .or_else(|| number(Metric::Aspect).map(|v| format!("{v:.2}:1")))
                            .or_else(|| video.and_then(|s| s.aspect_ratio.clone())),
                    ),
                ],
            )))
            .child(group(metric_group(
                "Audio",
                [
                    (
                        "Codec",
                        value(Metric::AudioCodec)
                            .map(|s| s.to_uppercase())
                            .or_else(|| audio.and_then(|s| s.codec_name())),
                    ),
                    (
                        "Sample rate",
                        number(Metric::SampleRate)
                            .or_else(|| audio.and_then(|s| s.sample_rate.map(f64::from)))
                            .map(|rate| format!("{:.1} kHz", rate / 1000.)),
                    ),
                    (
                        "Channels",
                        value(Metric::Channels).or_else(|| {
                            audio.and_then(|s| {
                                s.channel_layout
                                    .clone()
                                    .or_else(|| s.channels.map(|ch| format!("{ch} ch")))
                            })
                        }),
                    ),
                    (
                        "Bitrate",
                        number(Metric::AudioBitrate)
                            .or_else(|| audio.and_then(|s| s.bitrate.map(|v| v as f64)))
                            .filter(|n| *n > 0.)
                            .map(|n| format!("{:.0} kbps", n / 1000.)),
                    ),
                ],
            )))
            .child(group(metric_group(
                "Color",
                [
                    (
                        "Pixel format",
                        value(Metric::PixelFormat)
                            .or_else(|| video.and_then(|s| s.pixel_format.clone())),
                    ),
                    ("Hardware format", value(Metric::HardwareFormat)),
                    (
                        "Matrix",
                        value(Metric::Matrix).or_else(|| video.and_then(|s| s.color_space.clone())),
                    ),
                    (
                        "Primaries",
                        value(Metric::Primaries)
                            .or_else(|| video.and_then(|s| s.color_primaries.clone())),
                    ),
                    (
                        "Transfer",
                        value(Metric::Transfer)
                            .or_else(|| video.and_then(|s| s.color_transfer.clone())),
                    ),
                ],
            )));
        let render_fps = measured
            .native_render_fps
            .map(fps)
            .or_else(|| number(Metric::FilterFps).map(|v| format!("{} est.", fps(v))));
        let display = number(Metric::DisplayFps)
            .filter(|v| *v > 0.)
            .or(measured.display_hz.map(f64::from))
            .map(|v| {
                if (v - v.round()).abs() < 0.01 {
                    format!("{v:.0} Hz")
                } else {
                    format!("{v:.2} Hz")
                }
            });
        let performance_row = div()
            .flex()
            .gap_2()
            .child(group(metric_group(
                "Timing",
                [
                    ("Render FPS", render_fps),
                    ("Display", display),
                    (
                        "A/V sync",
                        number(Metric::AvSync).map(|v| format!("{:.0} ms", v * 1000.)),
                    ),
                    (
                        "Dropped frames",
                        value(Metric::DroppedFrames)
                            .or(measured.native_dropped.map(|n| n.to_string())),
                    ),
                ],
            )))
            .child(group(metric_group(
                "Buffer",
                [
                    (
                        "Duration",
                        number(Metric::BufferDuration)
                            .or(measured.native_buffer_seconds)
                            .map(|v| format!("{v:.1} s")),
                    ),
                    (
                        "Cache used",
                        number(Metric::CacheUsed).map(megabytes).or_else(|| {
                            number(Metric::CacheReadAhead)
                                .map(|n| format!("{} ahead", megabytes(n)))
                        }),
                    ),
                    ("Cache limit", cache_limit.map(megabytes)),
                    (
                        "Read speed",
                        number(Metric::CacheSpeed).map(|n| format!("{}/s", megabytes(n))),
                    ),
                ],
            )))
            .child(group(metric_group(
                "App",
                [
                    (
                        "Player",
                        Some(if p.mpv() { "mpv" } else { "Windows" }.into()),
                    ),
                    (
                        "Play method",
                        Some(
                            if p.local_file {
                                "Local file"
                            } else {
                                "Direct Play"
                            }
                            .into(),
                        ),
                    ),
                    ("Memory", measured.memory.map(|v| megabytes(v as f64))),
                    ("UI FPS", measured.ui_fps.map(|v| format!("{v:.1}"))),
                ],
            )));
        let shader = p
            .shader()
            .filter(|shader| **shader != Shader::Off)
            .zip(p.shader_status());
        div()
            .w(px((width - 2. * OSD_MARGIN_X).min(650.)))
            .p_2()
            .rounded(px(12.))
            .border_1()
            .border_color(rgba(0xffffff32))
            .bg(rgba(0x101419df))
            .flex()
            .flex_col()
            .gap_2()
            .child(title("Performance", 15.).px_2().pt_1())
            .child(media_row)
            .child(performance_row)
            .when_some(shader, |d, (shader, status)| {
                let (state, detail, color) =
                    super::super::player_menu::shader_status_details(status);
                d.child(
                    div()
                        .px_2()
                        .py_1()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(title(shader.label().to_owned(), 13.).flex_none())
                        .child(label(state, 12., color).flex_none())
                        .when(!matches!(status, ShaderStatus::Checking), |d| {
                            d.child(label(detail, 12., MUTED).truncate().min_w_0())
                        }),
                )
            })
    }
}
