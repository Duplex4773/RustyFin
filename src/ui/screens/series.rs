use super::*;
use crate::jellyfin::Stream;

pub(super) fn content_width(w: &Window) -> f32 {
    (f32::from(w.viewport_size().width) - 2. * CONTENT_GUTTER).clamp(1., SERIES_MAX_WIDTH)
}

pub(super) fn sidebar_width(w: &Window) -> f32 {
    let width = content_width(w);
    if width >= SERIES_SPLIT_MIN {
        width * SIDEBAR_FRACTION
    } else {
        width
    }
}

fn panel() -> Div {
    glass_panel(div())
        .w_full()
        .min_w_0()
        .px(px(14.))
        .py(px(11.))
        .flex()
        .flex_col()
        .gap(px(7.))
        .rounded(px(12.))
}

fn fact(key: &'static str, value: String) -> Div {
    div()
        .w_full()
        .flex()
        .items_start()
        .gap_2()
        .child(
            label(key, 12., META)
                .w(px(88.))
                .flex_none()
                .line_height(px(18.)),
        )
        .child(
            label(value, 13., TEXT)
                .min_w_0()
                .flex_1()
                .line_height(px(18.)),
        )
}

fn show_details(item: &Item) -> Option<Div> {
    let studios = item
        .studios
        .iter()
        .flatten()
        .map(|s| s.name.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    let facts = [
        ("Network", item.network.clone()),
        // Jellyfin libraries can store the broadcaster in Studios.
        (
            if item.network.is_some() {
                "Studio"
            } else {
                "Studio / network"
            },
            Some(studios),
        ),
        ("Status", item.status.clone()),
        ("Genres", item.genres.as_ref().map(|v| v.join(", "))),
        ("Certification", item.official_rating.clone()),
    ];
    let rows: Vec<_> = facts
        .into_iter()
        .filter_map(|(key, value)| {
            let value = value?;
            (!value.trim().is_empty()).then(|| fact(key, value))
        })
        .collect();
    (!rows.is_empty()).then(|| {
        panel()
            .child(section_heading("Show details", 18.))
            .children(rows)
    })
}

fn spec_row(key: String, values: Vec<String>) -> Option<Div> {
    if values.is_empty() {
        return None;
    }
    let chips = div()
        .min_w_0()
        .flex_1()
        .flex()
        .flex_wrap()
        .gap(px(4.))
        .children(values.into_iter().map(|value| {
            label(value, 12., BODY)
                .flex_none()
                .whitespace_nowrap()
                .line_height(px(17.))
                .px(px(6.))
                .py(px(1.))
                .rounded(px(5.))
                .bg(rgba(MICRO_FILL))
        }));
    Some(
        div()
            .w_full()
            .flex()
            .items_start()
            .gap_2()
            .child(
                label(key, 12., META)
                    .w(px(88.))
                    .flex_none()
                    .line_height(px(19.)),
            )
            .child(chips),
    )
}

fn track_specs(streams: &[Stream], kind: &str, label: &str) -> Option<Div> {
    let mut values = Vec::new();
    let mut count = 0;
    for track in streams.iter().filter(|s| s.kind == kind) {
        count += 1;
        let mut parts = Vec::new();
        if let Some(language) = &track.language {
            parts.push(language.to_uppercase());
        }
        if let Some(codec) = &track.codec {
            parts.push(codec.to_uppercase());
        }
        if kind == "Audio" {
            if let Some(layout) = &track.channel_layout {
                parts.push(layout.clone());
            } else if let Some(n) = track.channels {
                parts.push(format!("{n} ch"));
            }
        }
        let name = if parts.is_empty() {
            track.label()
        } else {
            parts.join(" ")
        };
        for value in std::iter::once(name).chain(track.spatial_audio()) {
            if !values.contains(&value) {
                values.push(value);
            }
        }
    }
    if values.len() > 4 {
        let more = values.len() - 3;
        values.truncate(3);
        values.push(format!("+{more}"));
    }
    spec_row(format!("{label} ({count})"), values)
}

fn technical_specs(episode: &Item) -> Option<Div> {
    let source = episode.detail_source(None);
    let streams = source
        .and_then(|s| s.media_streams.as_deref())
        .filter(|s| !s.is_empty())
        .or(episode.media_streams.as_deref())
        .unwrap_or_default();
    if streams.is_empty() {
        return None;
    }
    let video = crate::jellyfin::formats::Formats::from_streams(streams, false)
        .labels()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let caption = match (episode.parent_index_number, episode.index_number) {
        (Some(s), Some(e)) => format!("S{s} • E{e} · {}", episode.name),
        _ => episode.name.clone(),
    };
    Some(
        panel()
            .child(section_heading("Technical specs", 18.))
            .child(label(caption, 13., META))
            .children(spec_row("Video".into(), video))
            .children(track_specs(streams, "Audio", "Audio"))
            .children(track_specs(streams, "Subtitle", "Subtitles")),
    )
}

impl Library {
    fn season_selector(&self, season: Option<&str>, cx: &mut Context<Self>) -> Div {
        let mut header = div().child(section_heading("Episodes", 22.));
        if !self.state.seasons.is_empty() {
            let seasons = div()
                .id("seasons")
                .track_scroll(&self.state.season_scroll)
                .w_full()
                .overflow_x_scroll()
                .flex()
                .gap_2()
                .py_2()
                .children(self.state.seasons.iter().enumerate().map(|(i, part)| {
                    let id = part.id.clone();
                    let current = season == Some(id.as_str());
                    pill(
                        ("season", i),
                        part.name.clone(),
                        None,
                        PillStyle::Season(current),
                    )
                    .when(
                        current && self.keyboard && self.state.series_focus == SeriesFocus::Seasons,
                        |d| d.border_2().border_color(rgb(META)),
                    )
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.state.series_focus = SeriesFocus::Seasons;
                        s.focus.focus(w);
                        s.select_season(&id, w, cx);
                    }))
                }));
            header = header.child(seasons);
        }
        header
    }

    fn series_episode_row(
        &self,
        episode: &Item,
        index: usize,
        width: f32,
        w: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let image_width = EPISODE_IMAGE_WIDTH.min((width * 0.34).max(72.));
        let selected = self.keyboard
            && self.focus.is_focused(w)
            && self.state.series_focus == SeriesFocus::Episodes
            && self.state.selection == index;
        let play_target = self
            .state
            .series_target
            .as_ref()
            .filter(|ep| ep.id == episode.id)
            .unwrap_or(episode);
        let played = play_target.user_data.as_ref().and_then(|d| d.played) == Some(true);
        let hover = self
            .state
            .hovered_episode
            .as_ref()
            .filter(|(id, _)| id == &episode.id);
        let mut opacity = if selected || hover.is_some() { 1. } else { 0. };
        if !selected
            && let Some((_, since)) = hover
            && scrolling::animations_enabled()
        {
            opacity = (since.elapsed().as_secs_f32() / 0.14).min(1.);
            if opacity < 1. {
                w.request_animation_frame();
            }
        }
        let play = play_target.clone();
        let play_label = format!("{} · {}", episode_action(play_target), episode.name);
        let overlay = tip(
            div()
                .id(("play-episode", index))
                .size(px(40.))
                .rounded_full()
                .bg(rgb(TEXT))
                .text_color(rgb(BG))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .child(icon(PLAY, 16.)),
            play_label,
        )
        .opacity(opacity)
        .focus(|d| d.opacity(1.).border_2().border_color(rgb(META)))
        .on_click(cx.listener(move |s, _, w, cx| {
            cx.stop_propagation();
            s.play_item(play.clone(), w, cx);
        }));
        let still = div()
            .relative()
            .flex_none()
            .w(px(image_width))
            .h(px((image_width * 9. / 16.).round()))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .overflow_hidden()
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgba(REST_EDGE))
            .when_some(self.artwork.get(episode.landscape()), |d, art| {
                d.child(
                    img(art)
                        .absolute()
                        .inset_0()
                        .size_full()
                        .rounded(px(8.))
                        .object_fit(ObjectFit::Cover),
                )
            })
            .child(overlay)
            .children(self.card_format_strip(episode, &("episode", index).into(), image_width, cx));
        let heading = episode.index_number.map_or_else(
            || episode.name.clone(),
            |n| format!("{n}. {}", episode.name),
        );
        let copy = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        title(heading, EPISODE_TITLE_SIZE)
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_clamp(2),
                    )
                    .when_some(episode.run_time_ticks.filter(|n| *n > 0), |d, ticks| {
                        d.child(
                            label(format!("{}m", ticks.div_ceil(600_000_000)), 12., BODY)
                                .px_2()
                                .rounded_full()
                                .bg(rgba(0xffffff0f)),
                        )
                    }),
            )
            .when_some(
                episode.overview.as_ref().filter(|s| !s.trim().is_empty()),
                |d, desc| {
                    d.child(
                        label(desc.clone(), 13., META)
                            .line_height(px(18.))
                            .line_clamp(2),
                    )
                },
            );
        let target = episode.clone();
        let id = episode.id.clone();
        let row = accessibility::title_button(
            div()
                .id(("episode", index))
                .relative()
                .focusable()
                .tab_stop(true),
            episode,
            None,
            selected,
        );
        accessibility::keyboard_focus(row, cx)
            .w_full()
            .flex()
            .items_center()
            .gap_4()
            .p(px(12.))
            .rounded(px(10.))
            .bg(rgba(0xffffff08))
            .border_1()
            .border_color(rgba(0))
            .cursor_pointer()
            .hover(|d| d.bg(rgba(HOVER_FILL)).border_color(rgba(REST_EDGE)))
            .focus(|d| d.border_color(rgb(FOCUS)))
            .when(selected, |d| d.border_color(rgb(FOCUS)))
            .on_hover(cx.listener(move |s, inside, _, cx| {
                if *inside {
                    s.state.hovered_episode = Some((id.clone(), Instant::now()));
                } else if s
                    .state
                    .hovered_episode
                    .as_ref()
                    .is_some_and(|(key, _)| key == &id)
                {
                    s.state.hovered_episode = None;
                } else {
                    return;
                }
                cx.notify();
            }))
            .child(still)
            .child(copy)
            .when(played, |d| {
                d.child(icon("\u{e73e}", 15.).text_color(rgb(0xa7d5c7)))
            })
            .on_click(cx.listener(move |s, _, w, cx| s.open_item(target.clone(), w, cx)))
    }

    pub(super) fn series_detail(
        &self,
        item: &Item,
        season: Option<&str>,
        head: Div,
        supplements: Div,
        w: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let width = content_width(w);
        let wide = width >= SERIES_SPLIT_MIN;
        let page = self.state.page.as_ref();
        let episodes = page.map(|p| p.items.as_slice()).unwrap_or_default();
        let cast = self.cast_cards(item, w, cx).map(|cast| panel().child(cast));
        let info = show_details(item);
        let specs = self
            .state
            .series_target
            .iter()
            .filter(|ep| episodes.iter().any(|it| it.id == ep.id))
            .chain(episodes)
            .find_map(technical_specs);
        let has_sidebar = cast.is_some() || info.is_some() || specs.is_some();
        let gap = if wide && has_sidebar {
            width * SERIES_GAP_FRACTION
        } else {
            0.
        };
        let main = if wide && has_sidebar {
            width - sidebar_width(w) - gap
        } else {
            width
        };
        let mut rows = div()
            .id("series-rows")
            .track_scroll(&self.state.series_rows)
            .w(px(main))
            .flex_none()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.season_selector(season, cx));
        for (i, ep) in episodes.iter().enumerate() {
            rows = rows.child(self.series_episode_row(ep, i, main, w, cx));
        }
        rows = if episodes.is_empty() {
            rows.child(self.empty("No episodes in this selection."))
        } else {
            rows.child(self.paging(cx))
        };
        let layout = div()
            .w(px(width))
            .flex()
            .items_start()
            .gap(px(if wide { gap } else { 24. }))
            .when(!wide, |d| d.flex_col())
            .child(rows)
            .when(has_sidebar, |d| {
                d.child(
                    div()
                        .w(px(sidebar_width(w)))
                        .flex_none()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(cast)
                        .children(specs)
                        .children(info),
                )
            });
        let content = div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .py_4()
            .gap_4()
            .child(layout)
            .child(
                div()
                    .w(px(width))
                    .child(self.extra_cards(item, w, cx))
                    .child(supplements),
            );
        div().size_full().min_h_0().child(
            div()
                .id("title-details")
                .track_scroll(&self.state.scroll)
                .size_full()
                .overflow_y_scroll()
                .child(head)
                .child(content)
                .children(self.similar_section(w, cx)),
        )
    }
}
