use super::*;
use crate::{jellyfin::Source, preferences::Shader, ui::playback::PlayerMenu};
use PlayerMenu::*;

const ROW: f32 = 38.;
const HEADER: f32 = 48.;
const SECONDARY: u32 = 0xc8cdce;

impl PlayerMenu {
    fn title(self) -> &'static str {
        match self {
            Settings => "Playback Settings",
            Speed => "Playback Speed",
            Zoom => "Zoom",
            Shaders => "Shaders",
            Quality => "Quality",
            Timer => "Sleep Timer",
            AudioSync => "Audio Sync",
            SubtitleSync => "Subtitle Sync",
            Audio => "Audio Tracks",
            Subtitles => "Subtitles",
            Chapters => "Chapters",
            Queue => "Up Next",
            Info => "File Info",
        }
    }
    fn glyph(self) -> &'static str {
        match self {
            Settings => "\u{e9e9}",
            Speed => "\u{ec4a}",
            Zoom => "\u{e8a3}",
            Shaders => "\u{e790}",
            Quality => "\u{e714}",
            Timer => "\u{e708}",
            AudioSync => "\u{e895}",
            SubtitleSync | Subtitles => "\u{e7f0}",
            Audio => "\u{e8d6}",
            Chapters => "\u{e8fd}",
            Queue => "\u{e8ef}",
            Info => "\u{e946}",
        }
    }
}

fn line() -> Div {
    div()
        .h(px(ROW))
        .flex_none()
        .px_3()
        .w_full()
        .flex()
        .items_center()
        .gap_2()
}

fn action(
    id: impl Into<ElementId>,
    name: impl Into<SharedString>,
    glyph: Option<&'static str>,
) -> Stateful<Div> {
    line()
        .id(id)
        .focusable()
        .tab_stop(true)
        .cursor_pointer()
        .rounded_md()
        .border_1()
        .border_color(rgba(0x00000000))
        .hover(|d| d.bg(rgba(0xffffff16)))
        .focus(|d| d.bg(rgba(0x101713b0)).border_color(rgb(LIME)))
        .when_some(glyph, |d, g| {
            d.child(icon(g, 17.).text_color(rgb(SECONDARY)))
        })
        .child(label(name, 15., TEXT).min_w_0().flex_1().truncate())
}

fn selection(id: impl Into<ElementId>, name: impl Into<SharedString>, on: bool) -> Stateful<Div> {
    action(id, name, None).when(on, |d| d.child(icon("\u{e73e}", 16.).text_color(rgb(LIME))))
}

fn switch(on: bool) -> Div {
    div()
        .relative()
        .flex_none()
        .w(px(38.))
        .h(px(22.))
        .rounded_full()
        .border_2()
        .border_color(if on { rgb(LIME) } else { rgba(0xffffff70) })
        .bg(if on { rgb(LIME) } else { rgba(0x0a0e105e) })
        .child(
            div()
                .absolute()
                .left(px(if on { 17. } else { 2. }))
                .top(px(2.))
                .size(px(14.))
                .rounded_full()
                .bg(rgb(if on { 0x22281b } else { SECONDARY })),
        )
}

fn readout(name: &str, value: impl Into<String>) -> Div {
    let value = value.into();
    div()
        .w_full()
        .min_h(px(ROW))
        .flex_none()
        .px_3()
        .py_2()
        .flex()
        .items_center()
        .gap_3()
        .child(
            label(name.to_owned(), 13., SECONDARY)
                .w(px(82.))
                .flex_none(),
        )
        .child(label(value, 14., TEXT).min_w_0().flex_1())
}

fn speed_label(speed: f64) -> String {
    if speed == 1. {
        "Normal".into()
    } else {
        format!("{speed}×")
    }
}

impl Library {
    pub(super) fn current_source(&self) -> Option<&Source> {
        let sources = self.playing.as_ref()?.media_sources.as_deref()?;
        sources
            .iter()
            .find(|s| s.id == self.controls.source)
            .or_else(|| sources.first())
    }

    fn sleep_label(&self) -> String {
        if self.controls.sleep_at_end {
            "End of title".into()
        } else if let Some(t) = self.controls.sleep {
            format!(
                "{} left",
                clock(t.saturating_duration_since(Instant::now()).as_secs_f64())
            )
        } else {
            "Off".into()
        }
    }

    fn navigate_setting(
        &self,
        page: PlayerMenu,
        value: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        action(page.title(), page.title(), Some(page.glyph()))
            .when(self.menu_focus.settings_row == page, |d| {
                self.initial_menu_button(d)
            })
            .child(
                label(value, 13., SECONDARY)
                    .truncate()
                    .max_w(relative(0.45)),
            )
            .child(icon(NEXT, 12.).text_color(rgb(SECONDARY)))
            .on_click(cx.listener(move |s, _, _, cx| s.enter_player_menu(page, cx)))
    }

    fn settings_rows(&self, p: &Player, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        rows.push(
            self.navigate_setting(Speed, speed_label(self.controls.speed), cx)
                .into_any_element(),
        );
        if p.mpv() && !p.audio_only {
            rows.push(
                self.navigate_setting(Zoom, format!("{:.0}%", self.controls.zoom * 100.), cx)
                    .into_any_element(),
            );
        }
        if let Some(shader) = p.shader() {
            rows.push(
                self.navigate_setting(Shaders, shader.label().to_owned(), cx)
                    .into_any_element(),
            );
        }
        if let Some(source) = self.current_source() {
            let quality = source
                .bitrate
                .map(|b| format!("Original {:.1} Mbps", b as f64 / 1_000_000.))
                .unwrap_or_else(|| "Original".into());
            rows.push(
                self.navigate_setting(Quality, quality, cx)
                    .into_any_element(),
            );
        }
        rows.push(
            self.navigate_setting(Timer, self.sleep_label(), cx)
                .into_any_element(),
        );
        if p.mpv() {
            rows.push(
                self.navigate_setting(
                    AudioSync,
                    format!("{:+.0} ms", self.controls.delay * 1000.),
                    cx,
                )
                .into_any_element(),
            );
        }
        if !p.audio_only && (p.mpv() || !self.controls.captions.is_empty()) {
            rows.push(
                self.navigate_setting(
                    SubtitleSync,
                    format!("{:+.0} ms", self.controls.subtitle_delay * 1000.),
                    cx,
                )
                .into_any_element(),
            );
        }
        rows.push(
            action("autoplay-next", "Auto-Play Next", Some("\u{e893}"))
                .child(switch(self.prefs.autoplay))
                .on_click(cx.listener(|s, _, _, cx| {
                    s.prefs.autoplay = !s.prefs.autoplay;
                    s.save_preferences(cx);
                }))
                .into_any_element(),
        );
        if let Some(source) = self.current_source()
            && self.downloads.playing.is_none()
        {
            let streams = source.media_streams.as_deref().unwrap_or_default();
            if let Some(audio) = source
                .selected_audio(self.controls.choice.audio)
                .ok()
                .flatten()
            {
                rows.push(
                    self.navigate_setting(Audio, audio.label(), cx)
                        .into_any_element(),
                );
            }
            if streams.iter().any(|s| s.kind == "Subtitle") {
                let sub = self
                    .controls
                    .choice
                    .subtitle
                    .or(source.default_subtitle_stream_index);
                let value = streams
                    .iter()
                    .find(|s| s.kind == "Subtitle" && s.index == sub)
                    .map_or_else(|| "Off".into(), |s| s.label());
                rows.push(
                    self.navigate_setting(Subtitles, value, cx)
                        .into_any_element(),
                );
            }
        }
        if let Some(chapters) = self.playing.as_ref().and_then(|i| i.chapters.as_ref())
            && !chapters.is_empty()
            && p.seekable
        {
            rows.push(
                self.navigate_setting(Chapters, chapters.len().to_string(), cx)
                    .into_any_element(),
            );
        }
        if let Some(q) = &self.controls.queue {
            rows.push(
                self.navigate_setting(Queue, q.len().to_string(), cx)
                    .into_any_element(),
            );
        }
        if p.seekable && p.status.ready {
            rows.push(
                action("restart-playback", "Play from beginning", Some("\u{e72c}"))
                    .on_click(cx.listener(|s, _, w, cx| {
                        if let Some(p) = &s.player {
                            let paused = p.status.paused;
                            let rewind = -p.status.duration.max(p.status.position);
                            s.toggle_player_menu(Settings, w, cx);
                            s.control(Some(rewind), cx);
                            if paused {
                                s.control(None, cx);
                            }
                        }
                    }))
                    .into_any_element(),
            );
        }
        if self.next_title().is_some() {
            rows.push(
                action("play-next-title", "Play next", Some("\u{e893}"))
                    .on_click(cx.listener(|s, _, w, cx| s.advance(w, cx)))
                    .into_any_element(),
            );
        }
        rows.push(
            self.navigate_setting(Info, p.label.clone(), cx)
                .into_any_element(),
        );
        rows
    }

    fn initial_menu_button(&self, button: Stateful<Div>) -> Stateful<Div> {
        self.menu_focus.selected.set(true);
        button.track_focus(&self.menu_focus.current)
    }

    fn track_rows(&self, page: PlayerMenu, p: &Player, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        let Some(source) = self.current_source() else {
            return rows;
        };
        let current = &self.controls.choice;
        let audio = page == Audio;
        let index = if audio {
            source
                .selected_audio(current.audio)
                .ok()
                .flatten()
                .and_then(|s| s.index)
        } else {
            current.subtitle.or(source.default_subtitle_stream_index)
        };
        let add = |id, label: String, value: i32| {
            let mut choice = current.clone();
            choice.source = Some(source.id.clone());
            if audio {
                choice.audio = Some(value);
            } else {
                choice.subtitle = Some(value);
            }
            let selected = index.unwrap_or(-1) == value;
            selection(("stream-option", id), label, selected)
                .when(selected, |d| self.initial_menu_button(d))
                .on_click(cx.listener(move |s, _, w, cx| s.switch_tracks(choice.clone(), w, cx)))
                .into_any_element()
        };
        if audio && let Some(output) = p.audio_output() {
            rows.push(readout("Output", output).into_any_element());
        }
        if !audio {
            rows.push(add(0, "Off".into(), -1));
        }
        for (i, t) in source.media_streams.iter().flatten().enumerate() {
            if let Some(index) = t.index
                && ((audio && t.kind == "Audio" && !t.is_external)
                    || (!audio && t.kind == "Subtitle"))
            {
                if let Some(reason) = t.subtitle_limitation(!p.mpv()) {
                    rows.push(
                        line()
                            .flex_col()
                            .items_start()
                            .justify_center()
                            .gap_0()
                            .child(label(t.label(), 14., MUTED).truncate().max_w_full())
                            .child(label(reason, 12., SECONDARY))
                            .into_any_element(),
                    );
                } else {
                    rows.push(add(i + 1, t.label(), index));
                }
            }
        }
        rows
    }

    fn menu_rows(&self, page: PlayerMenu, p: &Player, cx: &mut Context<Self>) -> Vec<AnyElement> {
        match page {
            Settings => self.settings_rows(p, cx),
            Shaders => self.shader_rows(p, cx),
            Audio | Subtitles => self.track_rows(page, p, cx),
            Speed if p.native_bitstream() => vec![
                readout("Normal", "1× · Digital bitstream").into_any_element(),
                label(
                    "The receiver requires normal speed for this audio output.",
                    13.,
                    SECONDARY,
                )
                .px_3()
                .py_2()
                .into_any_element(),
            ],
            Speed | Zoom => {
                let values: &[f64] = if page == Speed {
                    &[0.25, 0.5, 0.75, 1., 1.25, 1.5, 2., 4., 8.]
                } else {
                    &[0.5, 0.75, 1., 1.25, 1.5, 2.]
                };
                values
                    .iter()
                    .copied()
                    .filter(|rate| page != Speed || p.supports_speed(*rate))
                    .enumerate()
                    .map(|(i, v)| {
                        let speed = page == Speed;
                        let current = if speed {
                            self.controls.speed
                        } else {
                            self.controls.zoom
                        };
                        let name = if speed {
                            speed_label(v)
                        } else {
                            format!("{:.0}%", 100. * v)
                        };
                        selection(("player-value", i), name, current == v)
                            .when(current == v, |d| self.initial_menu_button(d))
                            .on_click(cx.listener(move |s, _, _, cx| {
                                if let Some(p) = &s.player {
                                    let result = if speed { p.speed(v) } else { p.zoom(v) };
                                    match result {
                                        Ok(()) => {
                                            if speed {
                                                s.controls.speed = v;
                                            } else {
                                                s.controls.zoom = v;
                                            }
                                            s.controls.warning.clear();
                                        }
                                        Err(e) => s.controls.warning = e.to_string(),
                                    }
                                }
                                cx.notify();
                            }))
                            .into_any_element()
                    })
                    .collect()
            }
            Timer => [
                ("Off", 0),
                ("15 min", 15),
                ("30 min", 30),
                ("60 min", 60),
                ("End of title", -1),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, (name, minutes))| {
                let selected = match minutes {
                    -1 => self.controls.sleep_at_end,
                    0 => !self.controls.sleep_at_end && self.controls.sleep.is_none(),
                    _ => false,
                };
                selection(("timer", i), name, selected)
                    .when(selected, |d| self.initial_menu_button(d))
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.controls.sleep_at_end = minutes < 0;
                        s.controls.sleep = (minutes > 0)
                            .then(|| Instant::now() + Duration::from_secs(minutes as u64 * 60));
                        s.back_player_menu(w, cx);
                    }))
                    .into_any_element()
            })
            .collect(),
            AudioSync | SubtitleSync => {
                let sub = page == SubtitleSync;
                let current = if sub {
                    self.controls.subtitle_delay
                } else {
                    self.controls.delay
                };
                let mut rows = vec![
                    readout("Offset", format!("{:+.0} ms", current * 1000.)).into_any_element(),
                ];
                for (i, (name, delta)) in [("−50 ms", -0.05), ("Reset", 0.), ("+50 ms", 0.05)]
                    .into_iter()
                    .enumerate()
                {
                    rows.push(
                        action(("sync", i), name, None)
                            .on_click(cx.listener(move |s, _, _, cx| {
                                let v = if delta == 0. { 0. } else { current + delta };
                                if let Some(p) = &s.player {
                                    let result = if sub {
                                        p.subtitle_delay(v)
                                    } else {
                                        p.audio_delay(v)
                                    };
                                    match result {
                                        Ok(()) => {
                                            if sub {
                                                s.controls.subtitle_delay = v;
                                            } else {
                                                s.controls.delay = v;
                                            }
                                            s.controls.warning.clear();
                                        }
                                        Err(e) => s.controls.warning = e.to_string(),
                                    }
                                }
                                cx.notify();
                            }))
                            .into_any_element(),
                    );
                }
                rows
            }
            Quality => {
                let mut rows = Vec::new();
                if let Some(source) = self.current_source() {
                    rows.push(readout("Quality", "Original").into_any_element());
                    if let Some(b) = source.bitrate {
                        rows.push(
                            readout("Bitrate", format!("{:.1} Mbps", b as f64 / 1_000_000.))
                                .into_any_element(),
                        );
                    }
                }
                if self.downloads.playing.is_none()
                    && let Some(sources) =
                        self.playing.as_ref().and_then(|i| i.media_sources.as_ref())
                    && sources.len() > 1
                {
                    for (i, v) in sources.iter().enumerate() {
                        let choice = MediaChoice {
                            source: Some(v.id.clone()),
                            ..Default::default()
                        };
                        rows.push(
                            selection(("version", i), v.name(), v.id == self.controls.source)
                                .when(v.id == self.controls.source, |d| {
                                    self.initial_menu_button(d)
                                })
                                .on_click(cx.listener(move |s, _, w, cx| {
                                    s.switch_tracks(choice.clone(), w, cx)
                                }))
                                .into_any_element(),
                        );
                    }
                }
                rows
            }
            Chapters => {
                let chapters = self
                    .playing
                    .as_ref()
                    .and_then(|i| i.chapters.as_deref())
                    .unwrap_or_default();
                let current = chapters
                    .partition_point(|c| {
                        c.start_position_ticks as f64 / 10_000_000. <= p.status.position
                    })
                    .checked_sub(1);
                let mut rows = Vec::new();
                let jump = |row: Stateful<Div>, at: f64, cx: &mut Context<Self>| {
                    row.on_click(cx.listener(move |s, _, w, cx| {
                        if let Some(p) = &s.player {
                            s.control(Some(at - p.status.position), cx);
                            s.toggle_player_menu(Chapters, w, cx);
                        }
                    }))
                    .into_any_element()
                };
                let previous = chapters.iter().enumerate().rev().find(|(_, chapter)| {
                    chapter.start_position_ticks as f64 / 10_000_000. < p.status.position - 2.
                });
                if let Some((i, chapter)) = previous {
                    let name = if current == Some(i) {
                        "Restart chapter"
                    } else {
                        "Previous chapter"
                    };
                    rows.push(jump(
                        action("previous-chapter", name, Some("\u{e892}")),
                        chapter.start_position_ticks as f64 / 10_000_000.,
                        cx,
                    ));
                }
                if let Some(chapter) = chapters.iter().find(|c| {
                    c.start_position_ticks as f64 / 10_000_000. > p.status.position + 0.25
                }) {
                    rows.push(jump(
                        action("next-chapter", "Next chapter", Some("\u{e893}")),
                        chapter.start_position_ticks as f64 / 10_000_000.,
                        cx,
                    ));
                }
                for (i, chapter) in chapters.iter().enumerate() {
                    let at = chapter.start_position_ticks as f64 / 10_000_000.;
                    let name = chapter
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("Chapter {}", i + 1));
                    let row = selection(
                        ("chapter", i),
                        format!("{}    {name}", clock(at)),
                        current == Some(i),
                    )
                    .when(current == Some(i), |d| self.initial_menu_button(d));
                    rows.push(jump(row, at, cx));
                }
                rows
            }
            Queue => {
                let mut rows = Vec::new();
                if let Some(q) = &self.controls.queue {
                    if q.is_empty() {
                        rows.push(readout("Queue", "Empty").into_any_element());
                    }
                    for (i, item) in q.iter().take(50).enumerate() {
                        rows.push(
                            line()
                                .child(label(item.name.clone(), 14., TEXT).flex_1().truncate())
                                .child(pill(("queue-next", i), "Play next", None, false).on_click(
                                    cx.listener(move |s, _, _, cx| {
                                        if let Some(q) = &mut s.controls.queue
                                            && let Some(item) = q.remove(i)
                                        {
                                            q.push_front(item);
                                        }
                                        cx.notify();
                                    }),
                                ))
                                .child(pill(("queue-remove", i), "Remove", None, false).on_click(
                                    cx.listener(move |s, _, _, cx| {
                                        if let Some(q) = &mut s.controls.queue {
                                            q.remove(i);
                                        }
                                        cx.notify();
                                    }),
                                ))
                                .into_any_element(),
                        );
                    }
                }
                rows
            }
            Info => {
                let mut rows = Vec::new();
                if let Some(i) = &self.playing {
                    let name = i
                        .series_name
                        .as_ref()
                        .map_or_else(|| i.name.clone(), |series| format!("{series} · {}", i.name));
                    rows.push(readout("Title", name).into_any_element());
                }
                rows.push(readout("Player", p.label.clone()).into_any_element());
                if let Some(s) = self.current_source() {
                    if let Some(ext) = &s.container {
                        rows.push(readout("Container", ext.to_uppercase()).into_any_element());
                    }
                    for track in s
                        .media_streams
                        .iter()
                        .flatten()
                        .filter(|t| t.kind == "Video" || t.kind == "Audio")
                    {
                        rows.push(readout(&track.kind, track.label()).into_any_element());
                    }
                }
                rows
            }
        }
    }

    fn shader_rows(&self, p: &Player, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(current) = p.shader() else {
            return Vec::new();
        };
        let custom = self
            .prefs
            .mpv
            .custom_shaders
            .iter()
            .cloned()
            .map(Shader::Custom);
        let mut rows = Vec::new();
        for (i, shader) in Shader::PRESETS.iter().cloned().chain(custom).enumerate() {
            let selected = current == &shader;
            rows.push(
                action(("player-shader", i), shader.label().to_owned(), None)
                    .when(selected, |d| self.initial_menu_button(d))
                    .child(
                        label(shader.description(), 12., SECONDARY)
                            .truncate()
                            .max_w(relative(0.5)),
                    )
                    .when(selected, |d| {
                        d.child(icon("\u{e73e}", 16.).text_color(rgb(LIME)))
                    })
                    .on_click(cx.listener(move |s, _, _, cx| s.select_shader(shader.clone(), cx)))
                    .into_any_element(),
            );
        }
        rows.push(
            action("add-shader-file", "Add shader file…", Some("\u{e710}"))
                .on_click(cx.listener(|s, _, w, cx| s.import_shader(w, cx)))
                .into_any_element(),
        );
        rows
    }

    pub(super) fn playback_menu(
        &self,
        p: &Player,
        w: &Window,
        cx: &mut Context<Self>,
    ) -> (Div, f32) {
        let page = self.controls.menu;
        self.menu_focus.selected.set(false);
        let rows = self.menu_rows(page, p, cx);
        let mut scopes = self.menu_focus.rows.borrow_mut();
        scopes.resize_with(rows.len(), || cx.focus_handle());
        if self.menu_focus.entering.replace(false) {
            let weak = cx.weak_entity();
            w.on_next_frame(move |w, cx| {
                let _ = weak.update(cx, |s, cx| {
                    if s.player.as_ref().is_some_and(|p| p.panel_open) && s.controls.menu == page {
                        let focus = &s.menu_focus;
                        if focus.selected.get() {
                            focus.current.focus(w);
                        } else {
                            focus.close.focus(w);
                            w.focus_next();
                            if !focus.scope.contains_focused(w, cx) {
                                focus.close.focus(w);
                            }
                        }
                        focus.reveal(w, cx, !s.controls.warning.is_empty());
                        cx.notify();
                    }
                });
            });
        }
        let warning = !self.controls.warning.is_empty();
        let height =
            (HEADER + 12. + ROW * rows.len() as f32 + if warning { 48. } else { 0. }).min(492.);
        let header = div()
            .h(px(HEADER))
            .w_full()
            .flex_none()
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgba(0xffffff28))
            .when(page == Settings, |d| {
                d.child(icon(page.glyph(), 19.).ml_1())
            })
            .when(page != Settings, |d| {
                d.child(
                    glyph_button("Back", BACK)
                        .on_click(cx.listener(|s, _, w, cx| s.back_player_menu(w, cx))),
                )
            })
            .child(title(page.title(), 18.).flex_1())
            .child(
                glyph_button("Close playback settings", "\u{e711}")
                    .track_focus(&self.menu_focus.close)
                    .on_click(cx.listener(move |s, _, w, cx| s.toggle_player_menu(page, w, cx))),
            );
        let body =
            div()
                .id(("player-menu-body", page as usize))
                .track_scroll(&self.menu_focus.scroll)
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(6.))
                .py(px(6.))
                .when(warning, |d| {
                    d.child(
                        label(self.controls.warning.clone(), 13., 0xffd9b3)
                            .px_3()
                            .py_2(),
                    )
                })
                .children(rows.into_iter().zip(scopes.iter()).enumerate().map(
                    |(i, (row, handle))| {
                        div()
                            .id(("menu-row", i))
                            .track_focus(handle)
                            .tab_stop(false)
                            .child(row)
                    },
                ));
        (
            div()
                .track_focus(&self.menu_focus.scope)
                .tab_stop(false)
                .size_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .rounded(px(16.))
                .bg(rgba(0x101419bf))
                .border_1()
                .border_color(rgba(0xffffff45))
                .child(header)
                .child(body),
            height + 2.,
        )
    }
}
