use super::*;
use crate::jellyfin::Stream;

type Field = (&'static str, Option<String>);

fn dimensions(stream: &Stream) -> Option<String> {
    let (w, h) = stream.width.zip(stream.height)?;
    (w > 0 && h > 0).then(|| format!("{w} × {h}"))
}

fn number(value: f64, precision: usize) -> String {
    format!("{value:.precision$}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn frame_rate(stream: &Stream) -> Option<String> {
    stream
        .frame_rate()
        .map(|fps| format!("{} fps", number(fps, 3)))
}

fn bitrate(bits: u64) -> String {
    if bits >= 1_000_000 {
        format!("{:.1} Mbps", bits as f64 / 1_000_000.)
    } else {
        format!("{:.0} kbps", bits as f64 / 1000.)
    }
}

fn size(bytes: u64) -> String {
    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;
    if bytes >= GIB {
        format!("{:.2} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MiB", bytes as f64 / MIB as f64)
    } else {
        format!("{:.1} KiB", bytes as f64 / 1024.)
    }
}

fn duration(seconds: u64) -> String {
    let minutes = seconds / 60;
    if minutes >= 60 {
        format!("{}h {}m {}s", minutes / 60, minutes % 60, seconds % 60)
    } else {
        format!("{minutes}m {}s", seconds % 60)
    }
}

fn detail(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("unknown"))
        .map(str::to_owned)
}

fn codec_profile(stream: &Stream) -> Option<String> {
    let values = [stream.codec_name(), detail(&stream.profile)];
    let values: Vec<_> = values.into_iter().flatten().collect();
    (!values.is_empty()).then(|| values.join(" · "))
}

fn dynamic_range(stream: &Stream) -> Option<String> {
    stream.range().map(|range| {
        if range == "DV" {
            "Dolby Vision".into()
        } else {
            range
        }
    })
}

fn channels(stream: &Stream) -> Option<String> {
    match (detail(&stream.channel_layout), stream.channels) {
        (Some(layout), Some(n)) => Some(format!("{layout} ({n} ch)")),
        (Some(layout), None) => Some(layout),
        (None, n) => n.map(|n| format!("{n} ch")),
    }
}

fn yes_no(value: bool) -> String {
    if value { "Yes" } else { "No" }.into()
}

fn badge(value: String) -> Div {
    label(value, 12., TEXT)
        .font_weight(FontWeight::SEMIBOLD)
        .bg(rgba(0xffffff14))
        .rounded_full()
        .px_2()
        .py_1()
}

fn fields(values: impl IntoIterator<Item = Field>) -> Div {
    let values: Vec<_> = values
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .collect();
    div()
        .flex()
        .flex_col()
        .gap_3()
        .children(values.chunks(2).map(|row| {
            div()
                .flex()
                .w_full()
                .gap_4()
                .children(row.iter().map(|(key, value)| {
                    div()
                        .w(relative(0.5))
                        .flex_1()
                        .min_w_0()
                        .child(label(*key, 12., SECONDARY).font_weight(FontWeight::SEMIBOLD))
                        .child(label(value.clone(), 14., TEXT))
                }))
        }))
}

fn section(name: impl Into<SharedString>, glyph: &'static str) -> Div {
    div()
        .m_1()
        .px_3()
        .py_3()
        .rounded(px(11.))
        .border_1()
        .border_color(rgba(0xffffff13))
        .bg(rgba(0xffffff07))
        .flex()
        .flex_col()
        .gap_3()
        .child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(icon(glyph, 16.).text_color(rgb(SECONDARY)))
                .child(section_heading(name, 15.)),
        )
}

fn track(name: String, glyph: &'static str, stream: &Stream) -> Div {
    let mut values = vec![
        ("Codec", stream.codec_name()),
        ("Profile", detail(&stream.profile)),
        ("Language", detail(&stream.language)),
        ("Bitrate", stream.bitrate.filter(|n| *n > 0).map(bitrate)),
    ];
    match stream.kind.as_str() {
        "Video" => values.extend([
            ("Resolution", dimensions(stream)),
            ("Aspect ratio", detail(&stream.aspect_ratio)),
            ("Frame rate", frame_rate(stream)),
            ("Dynamic range", dynamic_range(stream)),
            ("Bit depth", stream.bit_depth.map(|n| format!("{n} bit"))),
            ("Pixel format", detail(&stream.pixel_format)),
            ("Color space", detail(&stream.color_space)),
            ("Color primaries", detail(&stream.color_primaries)),
            ("Color transfer", detail(&stream.color_transfer)),
            (
                "Scan type",
                stream
                    .is_interlaced
                    .map(|i| if i { "Interlaced" } else { "Progressive" }.into()),
            ),
            ("Anamorphic", stream.is_anamorphic.map(yes_no)),
            (
                "Level",
                stream
                    .level
                    .filter(|n| n.is_finite() && *n > 0.)
                    .map(|n| number(n, 2)),
            ),
        ]),
        "Audio" => values.extend([
            ("Channels", channels(stream)),
            (
                "Sample rate",
                stream
                    .sample_rate
                    .filter(|n| *n > 0)
                    .map(|n| format!("{} kHz", number(f64::from(n) / 1000., 3))),
            ),
            ("Spatial audio", stream.spatial_audio()),
        ]),
        "Subtitle" => values.extend([
            ("Text based", stream.is_text_subtitle_stream.map(yes_no)),
            (
                "Can be served separately",
                stream.supports_external_stream.map(yes_no),
            ),
        ]),
        _ => (),
    }
    values.extend([
        ("Track title", detail(&stream.title)),
        (
            "Stream index",
            stream.index.filter(|n| *n >= 0).map(|n| n.to_string()),
        ),
        ("Time base", detail(&stream.time_base)),
    ]);
    let flags = [
        stream.is_default.then_some("Default"),
        stream.is_forced.then_some("Forced"),
        stream.is_external.then_some("External"),
        stream.is_hearing_impaired.then_some("Hearing impaired"),
    ];
    section(name, glyph)
        .when_some(detail(&stream.display_title), |d, name| {
            d.child(label(name, 13., SECONDARY))
        })
        .when(flags.iter().any(Option::is_some), |d| {
            d.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .children(flags.into_iter().flatten().map(|flag| badge(flag.into()))),
            )
        })
        .child(fields(values))
}

impl Library {
    pub(in crate::ui) fn scroll_file_info(&mut self, key: &str, cx: &mut Context<Self>) {
        let scroll = &self.menu_focus.scroll;
        let before = scroll.offset();
        let limit = scroll.max_offset().height;
        let page = (scroll.bounds().size.height - px(36.)).max(px(0.));
        let y = match key {
            "up" => before.y + px(48.),
            "down" => before.y - px(48.),
            "pageup" => before.y + page,
            "pagedown" => before.y - page,
            "home" => px(0.),
            "end" => -limit,
            _ => return,
        };
        self.state.wheel_motion = None;
        scroll.set_offset(point(before.x, y.clamp(-limit, px(0.))));
        cx.notify();
    }

    pub(super) fn file_info_rows(&self, p: &Player, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let source = &p.source;
        let streams = source.media_streams.as_deref().unwrap_or_default();
        let video = streams
            .iter()
            .find(|s| s.kind == "Video" && !s.is_attached_pic);
        let audio = source
            .selected_audio(self.controls.choice.audio)
            .ok()
            .flatten();
        let runtime = source
            .run_time_ticks
            .or_else(|| self.playing.as_ref().and_then(|i| i.run_time_ticks))
            .filter(|n| *n > 0)
            .map(|n| n / 10_000_000)
            .or_else(|| {
                (p.status.duration.is_finite() && p.status.duration > 0.)
                    .then(|| p.status.duration.round() as u64)
            })
            .map(duration);
        let format = detail(&source.container).map(|text| text.to_uppercase());
        let file_size = source.size.filter(|n| *n > 0).map(size);
        let badges = [
            format.clone(),
            video.and_then(dimensions),
            video.and_then(dynamic_range),
            video.and_then(Stream::codec_name),
            audio.and_then(Stream::codec_name),
            audio.and_then(Stream::spatial_audio),
            file_size.clone(),
            runtime.clone(),
        ];
        let heading = self.playing.as_ref().map(|i| {
            i.series_name
                .as_ref()
                .map_or_else(|| i.name.clone(), |series| format!("{series} · {}", i.name))
        });
        let mut overview = section("Overview", "\u{e946}").child(fields([
            ("Container", format.clone()),
            ("Duration", runtime.clone()),
            ("Total size", file_size.clone()),
            (
                "Overall bitrate",
                source.bitrate.filter(|n| *n > 0).map(bitrate),
            ),
            ("Resolution", video.and_then(dimensions)),
            ("Aspect ratio", video.and_then(|s| detail(&s.aspect_ratio))),
            ("Video", video.and_then(codec_profile)),
            ("Frame rate", video.and_then(frame_rate)),
            ("Audio", audio.and_then(codec_profile)),
            ("Channels", audio.and_then(channels)),
            ("Playback engine", Some(p.label.clone())),
            ("Current audio output", p.audio_output()),
        ]));
        if let Some(notice) = p.route_notice {
            overview = overview.child(label(notice, 13., BODY));
        }
        // A remote media URL can contain an access token. Only offer a filesystem path.
        let path = source.path.as_deref().filter(|path| {
            let bytes = path.as_bytes();
            !path.contains("://") && (path.starts_with(['/', '\\']) || matches!(bytes, [drive, b':', b'/' | b'\\', ..] if drive.is_ascii_alphabetic()))
        });
        let name = path
            .and_then(|path| path.rsplit(['/', '\\']).next())
            .map(str::to_owned);
        let mut file = section("File", "\u{e8a5}");
        if let Some(path) = path {
            let text = path.to_owned();
            file = file.child(
                div()
                    .flex()
                    .items_start()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(label("Path", 12., SECONDARY).font_weight(FontWeight::SEMIBOLD))
                            .child(label(text.clone(), 13., TEXT)),
                    )
                    .child(
                        glyph_button("Copy file path", "\u{e8c8}").on_click(cx.listener(
                            move |_, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                            },
                        )),
                    ),
            );
        }
        file = file.child(fields([
            ("File name", name),
            (
                "Source name",
                detail(&source.name).filter(|_| path.is_none()),
            ),
            ("Size", file_size),
            ("Container", format),
            ("Duration", runtime),
            (
                "File ID",
                (!source.id.is_empty()).then(|| source.id.clone()),
            ),
        ]));
        let mut rows = vec![
            div()
                .px_3()
                .py_2()
                .flex()
                .flex_col()
                .gap_2()
                .when_some(heading, |d, name| d.child(title(name, 18.)))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .children(badges.into_iter().flatten().map(badge)),
                )
                .into_any_element(),
            overview.into_any_element(),
            file.into_any_element(),
        ];
        for (kind, name, glyph) in [
            ("Video", "Video", VIDEO),
            ("Audio", "Audio", "\u{e8d6}"),
            ("Subtitle", "Subtitles", "\u{e7f0}"),
        ] {
            let tracks = streams
                .iter()
                .filter(|t| t.kind == kind && !t.is_attached_pic);
            let numbered = kind == "Subtitle" || tracks.clone().nth(1).is_some();
            for (i, stream) in tracks.enumerate() {
                let name = if numbered {
                    format!("{name} {}", i + 1)
                } else {
                    name.into()
                };
                rows.push(track(name, glyph, stream).into_any_element());
            }
        }
        rows
    }
}
