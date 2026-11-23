use super::*;
use crate::jellyfin::MediaChoice;

fn choices() -> Div {
    div().flex().items_center().flex_wrap().gap_2()
}
fn section(text: &str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .py_2()
        .child(title(text.to_owned(), 17.))
}
fn choice(id: impl Into<ElementId>, text: impl Into<SharedString>, active: bool) -> Stateful<Div> {
    pill(id, text, None, active).h(px(30.)).px_3()
}

impl Library {
    pub(super) fn edition_choices(&self, item: &Item, cx: &mut Context<Self>) -> Div {
        let sources = item.media_sources.as_deref().unwrap_or_default();
        let selected = item.detail_source(self.state.choice.source.as_deref());
        let mut versions = choices();
        for (i, source) in sources.iter().enumerate() {
            let id = source.id.clone();
            let active = selected.is_some_and(|s| s.id == id);
            versions = versions.child(
                choice(("version", i), source.name(), active)
                    .when(
                        self.state.detail_panel == Some(DetailPanel::Editions)
                            && (active || (selected.is_none() && i == 0)),
                        |b| b.track_focus(&self.detail_current_focus),
                    )
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.choose(
                            MediaChoice {
                                source: Some(id.clone()),
                                ..Default::default()
                            },
                            cx,
                        );
                        if s.state.detail_panel == Some(DetailPanel::Editions)
                            && s.state.choice.source.as_ref() == Some(&id)
                        {
                            s.state.detail_panel = None;
                            s.edition_focus.focus(w);
                        }
                    })),
            );
        }
        versions
    }

    pub(super) fn file_info(&self, item: &Item, cx: &mut Context<Self>) -> Div {
        let mut view = div().px_5().pb_3();
        let sources = item.media_sources.as_deref().unwrap_or_default();
        let selected = &self.state.choice;
        if sources.is_empty() {
            return view.child(label("Jellyfin supplied no file details.", 14., MUTED));
        }
        view = view.child(section("Version").child(self.edition_choices(item, cx)));
        let Some(source) = item.detail_source(selected.source.as_deref()) else {
            return view.child(label("Choose an available edition.", 14., MUTED));
        };
        let mut audio = choices().child(
            choice("server-audio", "Server default", selected.audio.is_none()).on_click(
                cx.listener(|s, _, _, cx| {
                    s.choose(
                        MediaChoice {
                            audio: None,
                            ..s.state.choice.clone()
                        },
                        cx,
                    );
                }),
            ),
        );
        let mut subtitles = choices();
        for (i, index, name) in [(0, None, "Server default"), (1, Some(-1), "Off")] {
            subtitles = subtitles.child(
                choice(
                    ("sub-default", i as usize),
                    name,
                    selected.subtitle == index,
                )
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.choose(
                        MediaChoice {
                            subtitle: index,
                            ..s.state.choice.clone()
                        },
                        cx,
                    );
                })),
            );
        }
        let mut streams = section("File info");
        let info = [
            source.container.as_ref().map(|s| s.to_uppercase()),
            source
                .size
                .map(|n| format!("{:.2} GiB", n as f64 / 1_073_741_824.)),
            source
                .bitrate
                .map(|n| format!("{:.1} Mb/s", n as f64 / 1_000_000.)),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
        streams = streams.child(label(info, 14., TEXT));
        if let Some(path) = &source.path
            && !path.contains("://")
        {
            streams = streams.child(label(path.clone(), 13., MUTED));
        }
        let native = source.known_native_dolby();
        for (i, track) in source.media_streams.iter().flatten().enumerate() {
            let text = track.label();
            streams = streams.child(label(format!("{}   {}", track.kind, text), 13., MUTED));
            let Some(index) = track.index else { continue };
            if track.kind == "Audio" && !track.is_external {
                audio = audio.child(
                    choice(("audio-track", i), text, selected.audio == Some(index)).on_click(
                        cx.listener(move |s, _, _, cx| {
                            s.choose(
                                MediaChoice {
                                    audio: Some(index),
                                    ..s.state.choice.clone()
                                },
                                cx,
                            );
                        }),
                    ),
                );
            } else if track.kind == "Subtitle" {
                if let Some(reason) = track.subtitle_limitation(native) {
                    subtitles =
                        subtitles.child(label(format!("{text} · {reason}"), 13., MUTED).px_2());
                    continue;
                }
                subtitles = subtitles.child(
                    choice(
                        ("subtitle-track", i),
                        text,
                        selected.subtitle == Some(index),
                    )
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.choose(
                            MediaChoice {
                                subtitle: Some(index),
                                ..s.state.choice.clone()
                            },
                            cx,
                        );
                    })),
                );
            }
        }
        view = view
            .child(section("Audio").child(audio))
            .child(
                section("Subtitles")
                    .child(subtitles)
                    .child(self.subtitle_lookup(item, cx)),
            )
            .child(streams);
        if let Some(chapters) = &item.chapters
            && !chapters.is_empty()
        {
            let mut group = choices();
            for (i, c) in chapters.iter().enumerate() {
                let at = c.start_position_ticks as f64 / 10_000_000.;
                let item = item.clone();
                let text = format!(
                    "{}  ·  {}",
                    clock(at),
                    c.name
                        .clone()
                        .unwrap_or_else(|| format!("Chapter {}", i + 1))
                );
                group = group.child(choice(("start-chapter", i), text, false).on_click(
                    cx.listener(move |s, _, w, cx| s.play_item_at(item.clone(), Some(at), w, cx)),
                ));
            }
            view = view.child(section("Chapters").child(group));
        }
        view
    }
    pub(super) fn credits_extras(&self, item: &Item, w: &Window, cx: &mut Context<Self>) -> Div {
        if matches!(item.kind.as_str(), "Series" | "Movie") {
            return self.credit_cards(item, w, cx);
        }
        let mut view = div().px_5();
        if !self.state.extras.is_empty() {
            let mut group = choices();
            for (i, extra) in self.state.extras.iter().enumerate() {
                let extra = extra.clone();
                group =
                    group.child(choice(("extra", i), extra.name.clone(), false).on_click(
                        cx.listener(move |s, _, w, cx| s.open_item(extra.clone(), w, cx)),
                    ));
            }
            view = view.child(section("Trailers and extras").child(group));
        }
        if let Some(people) = &item.people
            && !people.is_empty()
        {
            let mut group = choices();
            for (i, person) in people.iter().enumerate() {
                let id = person.id.clone();
                let role = person
                    .role
                    .as_deref()
                    .filter(|r| !r.is_empty())
                    .or(person.kind.as_deref())
                    .unwrap_or("");
                let text = if role.is_empty() {
                    person.name.clone()
                } else {
                    format!("{} · {role}", person.name)
                };
                group = group.child(choice(("person", i), text, false).on_click(cx.listener(
                    move |s, _, w, cx| {
                        s.open_person(&id, w, cx);
                    },
                )));
            }
            view = view.child(section("Cast and crew").child(group));
        }
        view
    }

    pub(super) fn copy_picker(&self, item: &Item, play: bool, cx: &mut Context<Self>) -> Div {
        let mut panel = div()
            .flex_1()
            .p_5()
            .child(title(item.name.clone(), 26.))
            .child(label("Choose a copy", 16., MUTED))
            .child(glyph_button("Back", BACK).on_click(cx.listener(|s, _, w, cx| s.back(w, cx))));
        for (i, copy) in std::iter::once(item).chain(&item.copies).enumerate() {
            let mut target = copy.clone();
            target.copies.clear();
            let account = self
                .accounts
                .iter()
                .find(|a| Some(a.key()) == copy.origin)
                .or_else(|| self.server.as_ref().and_then(Server::identity));
            let host = account
                .and_then(|a| url::Url::parse(&a.server).ok())
                .and_then(|u| u.host_str().map(str::to_owned))
                .unwrap_or_default();
            panel = panel.child(
                div().py_2().child(
                    choice(
                        ("library-copy", i),
                        format!("{host} · {} · {}", copy.name, copy.badges().join(" · ")),
                        false,
                    )
                    .on_click(cx.listener(move |s, _, w, cx| {
                        if play {
                            s.play_item(target.clone(), w, cx)
                        } else {
                            s.open_item(target.clone(), w, cx)
                        }
                    })),
                ),
            );
        }
        panel
    }
    pub(super) fn display_settings(&self, cx: &mut Context<Self>) -> Div {
        let mut settings = div().flex().flex_col().gap_2();
        let mut buttons = choices();
        if let Some(hdr) = &self.hdr {
            settings = settings.child(title(hdr.name.clone(), 16.)).child(label(
                hdr.description.clone(),
                14.,
                MUTED,
            ));
            if hdr.supported {
                buttons = buttons.child(
                    choice(
                        "system-hdr",
                        if hdr.enabled {
                            "Turn Windows HDR off"
                        } else {
                            "Turn Windows HDR on"
                        },
                        false,
                    )
                    .on_click(cx.listener(|s, _, w, cx| s.change_hdr(w, cx))),
                );
            }
        }
        settings.child(
            buttons
                .child(
                    choice("check-display", "Refresh display status", false)
                        .on_click(cx.listener(|s, _, w, cx| s.read_display(w, cx))),
                )
                .child(
                    choice("windows-display", "Windows display settings", false)
                        .on_click(|_, _, cx| cx.open_url("ms-settings:display")),
                ),
        )
    }
    pub(super) fn playback_settings(&self, cx: &mut Context<Self>) -> Div {
        use super::settings::{group, row, rows, toggle};
        use crate::preferences::Preferences;
        type Flag = fn(&mut Preferences) -> &mut bool;
        let flags: [(_, _, _, _, Flag); 5] = [
            (
                PLAY,
                "Play next episode",
                "Continue automatically when an episode ends",
                self.prefs.autoplay,
                |p| &mut p.autoplay,
            ),
            (
                "\u{e8d6}",
                "Audio passthrough",
                "Send supported audio formats directly to the receiver",
                self.prefs.passthrough,
                |p| &mut p.passthrough,
            ),
            (
                "\u{e7f4}",
                "Match fullscreen refresh rate",
                "Match the display refresh rate to the video",
                self.prefs.match_refresh,
                |p| &mut p.match_refresh,
            ),
            (
                NEXT,
                "Skip intros and credits",
                "Use the skip segments provided by Jellyfin",
                self.prefs.auto_skip,
                |p| &mut p.auto_skip,
            ),
            (
                VIDEO,
                "Play Dolby Vision Profile 7 MEL as Profile 8.1",
                "Turn off to play the HDR10 base layer",
                self.prefs.mel_to_81,
                |p| &mut p.mel_to_81,
            ),
        ];
        let playback = rows(flags.map(|(glyph, name, description, on, flag)| {
            row(name, glyph, name, description, toggle(on)).on_click(cx.listener(
                move |s, _, _, cx| {
                    let value = flag(&mut s.prefs);
                    *value = !*value;
                    s.save_preferences(cx);
                },
            ))
        }));
        let mut sizes = choices().child(label("Subtitle size", 15., TEXT));
        for (i, n) in [24, 30, 36, 42, 48].into_iter().enumerate() {
            sizes = sizes.child(
                choice(
                    ("subtitle-size", i),
                    format!("{n}"),
                    self.prefs.subtitle_size == n,
                )
                .on_click(cx.listener(move |s, _, _, cx| {
                    s.prefs.subtitle_size = n;
                    s.save_preferences(cx);
                })),
            );
        }
        use crate::preferences::{Channels, Shader, SubtitleStyle};
        let mut channels = choices().child(label("mpv PCM output", 15., TEXT));
        for (i, (value, name)) in [
            (Channels::System, "System layout"),
            (Channels::Stereo, "Stereo"),
            (Channels::Surround, "Up to 5.1"),
        ]
        .into_iter()
        .enumerate()
        {
            channels = channels.child(
                choice(("output-layout", i), name, self.prefs.mpv.channels == value).on_click(
                    cx.listener(move |s, _, _, cx| {
                        s.prefs.mpv.channels = value;
                        s.save_preferences(cx);
                    }),
                ),
            );
        }
        let mut audio = group().child(
            row(
                "normalize-pcm",
                "\u{e995}",
                "Normalize PCM volume",
                "Even out changes in loudness during PCM playback",
                toggle(self.prefs.mpv.normalize),
            )
            .on_click(cx.listener(|s, _, _, cx| {
                s.prefs.mpv.normalize = !s.prefs.mpv.normalize;
                s.save_preferences(cx);
            })),
        );
        if self.prefs.mpv.channels == Channels::Stereo {
            audio = audio.child(
                row(
                    "boost-center",
                    "\u{e767}",
                    "Boost center in stereo mix",
                    "Raise the center channel when downmixing to stereo",
                    toggle(self.prefs.mpv.center),
                )
                .on_click(cx.listener(|s, _, _, cx| {
                    s.prefs.mpv.center = !s.prefs.mpv.center;
                    s.save_preferences(cx);
                })),
            );
        }
        let mut shaders = choices().child(label("mpv shader", 15., TEXT));
        for (i, (value, name)) in [
            (Shader::Off, "Off"),
            (Shader::Sharpen, "Sharpen"),
            (Shader::Soften, "Soften"),
        ]
        .into_iter()
        .enumerate()
        {
            shaders = shaders.child(
                choice(("shader", i), name, self.prefs.mpv.shader == value).on_click(cx.listener(
                    move |s, _, _, cx| {
                        s.prefs.mpv.shader = value;
                        s.save_preferences(cx);
                    },
                )),
            );
        }
        let mut style = choices().child(label("mpv subtitle style", 15., TEXT));
        for (i, (value, name)) in [
            (SubtitleStyle::Authored, "Original"),
            (SubtitleStyle::WhiteOutline, "White outline"),
            (SubtitleStyle::YellowOutline, "Yellow outline"),
        ]
        .into_iter()
        .enumerate()
        {
            style = style.child(
                choice(("caption-style", i), name, self.prefs.mpv.subtitle == value).on_click(
                    cx.listener(move |s, _, _, cx| {
                        s.prefs.mpv.subtitle = value;
                        s.save_preferences(cx);
                    }),
                ),
            );
        }
        let options = |choices: Div| {
            choices
                .min_h(px(54.))
                .p_3()
                .bg(rgb(SURFACE))
                .rounded(px(4.))
        };
        div()
            .child(playback)
            .child(super::settings::heading("Subtitles"))
            .child(group().child(options(sizes)).child(options(style)))
            .child(super::settings::heading("Audio output"))
            .child(group().child(options(channels)).child(audio))
            .child(super::settings::heading("Video processing"))
            .child(group().child(options(shaders)))
    }
}
