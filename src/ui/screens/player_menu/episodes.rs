use super::*;
use crate::ui::episodes::EPISODE_ROW_HEIGHT;

impl Library {
    pub(super) fn episode_season_selector(&self, cx: &mut Context<Self>) -> Option<Div> {
        let drawer = &self.controls.episodes;
        let season = drawer.seasons.get(drawer.selected)?;
        let row = if drawer.seasons.len() > 1 {
            action("choose-episode-season", season.name.clone(), None)
                .child(icon("\u{e70d}", 12.).text_color(rgb(SECONDARY)))
                .on_click(cx.listener(|s, _, _, cx| {
                    s.menu_focus.scroll.set_offset(Point::default());
                    s.enter_player_menu(EpisodeSeasons, cx);
                }))
                .into_any_element()
        } else {
            line()
                .child(title(season.name.clone(), 15.))
                .into_any_element()
        };
        Some(
            div()
                .flex_none()
                .px(px(6.))
                .py(px(4.))
                .border_b_1()
                .border_color(rgba(0xffffff1c))
                .child(row),
        )
    }

    pub(super) fn episode_season_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let drawer = &self.controls.episodes;
        drawer
            .seasons
            .iter()
            .enumerate()
            .map(|(i, season)| {
                let selected = i == drawer.selected;
                selection(("episode-season", i), season.name.clone(), selected)
                    .when(selected, |row| self.initial_menu_button(row))
                    .on_click(cx.listener(move |s, _, w, cx| s.select_episode_season(i, w, cx)))
                    .into_any_element()
            })
            .collect()
    }

    pub(super) fn episode_drawer_rows(
        &self,
        p: &Player,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let items = match &self.controls.episodes.result {
            None => return vec![readout("Loading episodes…", "").into_any_element()],
            Some(Err(error)) => {
                return vec![
                    label(error.clone(), 14., 0xffd9b3).p_3().into_any_element(),
                    action("retry-episodes", "Retry", Some(REFRESH))
                        .on_click(cx.listener(|s, _, w, cx| s.load_episode_drawer(w, cx)))
                        .into_any_element(),
                ];
            }
            Some(Ok(items)) if items.is_empty() => {
                return vec![readout("No episodes in this season", "").into_any_element()];
            }
            Some(Ok(items)) => items,
        };
        let current = self.playing.as_ref().map(|i| i.id.as_str());
        let next = self.next_title().map(|i| i.id.as_str());
        let initial = items
            .iter()
            .position(|i| Some(i.id.as_str()) == current)
            .unwrap_or(0);
        items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let playing = Some(item.id.as_str()) == current;
                let watched = item.user_data.as_ref().and_then(|u| u.played) == Some(true);
                let (duration, progress) = if playing && p.status.duration > 0. {
                    (
                        Some((p.status.duration / 60.).ceil() as u64),
                        Some((p.status.position / p.status.duration) as f32),
                    )
                } else {
                    (
                        item.run_time_ticks
                            .filter(|&ticks| ticks > 0)
                            .map(|ticks| ticks.div_ceil(600_000_000)),
                        if watched {
                            Some(1.)
                        } else {
                            super::super::detail::episode_timing(item).1
                        },
                    )
                };
                let detail = [
                    item.index_number.map(|n| format!("Episode {n}")),
                    duration.map(|minutes| format!("{minutes} min")),
                    (!playing && watched).then(|| "Watched".into()),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                let still = self.artwork.get(item.landscape()).map(|image| img(image));
                let synopsis = item
                    .overview
                    .as_deref()
                    .map(str::trim)
                    .filter(|text| !text.is_empty());
                let cached = self.downloads.playing.as_ref().and_then(|_| {
                    let profile = self.downloads.profile.as_ref()?;
                    let record = self
                        .downloads
                        .records
                        .iter()
                        .find(|d| d.item.id == item.id)?;
                    let path = record.dir(profile).join("cover.jpg");
                    path.is_file().then(|| img(path))
                });
                let image = div()
                    .relative()
                    .flex_none()
                    .w(px(128.))
                    .h(px(72.))
                    .rounded_md()
                    .overflow_hidden()
                    .bg(rgb(SURFACE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .map(|d| match still.or(cached) {
                        Some(image) => d.child(image.size_full().object_fit(ObjectFit::Cover)),
                        None => d.child(icon(VIDEO, 21.).text_color(rgb(MUTED))),
                    })
                    .when_some(progress, |d, progress| {
                        d.child(
                            div()
                                .absolute()
                                .left_0()
                                .right_0()
                                .bottom_0()
                                .h(px(3.))
                                .bg(rgba(0x000000bc))
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(progress.clamp(0., 1.)))
                                        .bg(rgb(LIME)),
                                ),
                        )
                    });
                div()
                    .id(("drawer-episode", i))
                    .focusable()
                    .tab_stop(true)
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap_3()
                    .h(px(EPISODE_ROW_HEIGHT))
                    .px_2()
                    .rounded_md()
                    .border_1()
                    .border_color(rgba(if playing { 0xdafe675c } else { 0x00000000 }))
                    .when(playing, |d| d.bg(rgba(0xdafe6718)))
                    .hover(|d| d.bg(rgba(0xffffff19)))
                    .focus(|d| d.bg(rgba(0x252e28e8)).border_color(rgb(LIME)))
                    .when(i == initial, |d| self.initial_menu_button(d))
                    .child(image)
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .when_some(
                                if playing {
                                    Some("Now playing")
                                } else if Some(item.id.as_str()) == next {
                                    Some("Up next")
                                } else {
                                    None
                                },
                                |d, state| d.child(label(state, 11., LIME)),
                            )
                            .child(title(item.name.clone(), 15.).truncate())
                            .when(!detail.is_empty(), |d| {
                                d.child(label(detail, 12., SECONDARY).truncate())
                            })
                            .when_some(synopsis, |d, text| {
                                d.child(
                                    label(text.to_owned(), 12., SECONDARY)
                                        .mt_1()
                                        .line_height(px(15.))
                                        .line_clamp(3),
                                )
                            }),
                    )
                    .when_some(synopsis.map(str::to_owned), |d, text| {
                        d.tooltip(move |w, cx| Tooltip::new(text.clone()).build(w, cx))
                    })
                    .on_click(cx.listener(move |s, _, w, cx| s.play_drawer_episode(i, w, cx)))
                    .into_any_element()
            })
            .collect()
    }
}
