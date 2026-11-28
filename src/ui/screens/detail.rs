use super::*;

pub(super) fn detail_tool(item: &Item, name: &'static str, glyph: &'static str) -> Stateful<Div> {
    if matches!(item.kind.as_str(), "Movie" | "Series") {
        pill(name, name, Some(glyph), false)
    } else {
        glyph_button(name, glyph)
    }
}

pub(super) fn episode_timing(ep: &Item) -> (Option<String>, Option<f32>) {
    let Some(duration) = ep.run_time_ticks.filter(|n| *n > 0) else {
        return (None, None);
    };
    let position = ep.position().min(duration);
    if position == 0 {
        (
            Some(format!("{} min", duration.div_ceil(600_000_000))),
            None,
        )
    } else {
        (
            Some(format!(
                "{} min left",
                (duration - position).div_ceil(600_000_000)
            )),
            Some(position as f32 / duration as f32),
        )
    }
}

impl Library {
    pub(super) fn cinematic_header(&self, item: &Item, w: &Window, cx: &mut Context<Self>) -> Div {
        let width = f32::from(w.viewport_size().width);
        let height = (f32::from(w.viewport_size().height) * 0.54).clamp(280., 760.);
        let has_backdrop = self.artwork.detail_has_backdrop(item, w);
        let inset = CONTENT_GUTTER;
        let compact = !has_backdrop || height < 340.;
        let is_series = item.kind == "Series";
        let target = self.state.series_target.as_ref();
        let mut meta = Vec::new();
        if let Some(year) = item.production_year {
            meta.push(year.to_string());
        }
        if let Some(rating) = &item.official_rating {
            meta.push(rating.clone());
        }
        if let Some(genres) = &item.genres {
            meta.extend(genres.iter().take(2).cloned());
        }
        if is_series {
            let seasons = self
                .state
                .seasons
                .iter()
                .filter(|s| s.index_number != Some(0))
                .count();
            if seasons > 0 {
                meta.push(format!(
                    "{seasons} season{}",
                    if seasons == 1 { "" } else { "s" }
                ));
            }
        } else if let Some(ticks) = item.run_time_ticks.filter(|t| *t > 0) {
            meta.push(format!("{} min", ticks.div_ceil(600_000_000)));
        }
        let mut scores = score_badges(item, self.imdb_score()).peekable();
        let metadata = (!meta.is_empty() || scores.peek().is_some()).then(|| {
            div()
                .max_w_full()
                .flex()
                .items_center()
                .gap_2()
                .when(!meta.is_empty(), |d| {
                    d.child(label(meta.join("   ·   "), 14., BODY).min_w_0().truncate())
                })
                .children(scores)
        });

        let mut actions = div().flex().items_center().flex_wrap().gap_2();
        let mut episode_context = None;
        if let Some(ep) = target.filter(|_| is_series) {
            let (time, progress) = episode_timing(ep);
            let caption = match time {
                Some(time) => format!("{} · {time}", ep.name),
                None => ep.name.clone(),
            };
            let play_label = episode_play_label(ep);
            let button = named_pill(
                "resume-series",
                play_label.clone(),
                Some(PLAY),
                PillStyle::Primary,
                format!("{play_label}, {caption}"),
            );
            episode_context = Some(
                div()
                    .max_w_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when_some(progress, |row, fraction| {
                        row.child(
                            div()
                                .w(px(56.))
                                .h(px(4.))
                                .flex_none()
                                .rounded_full()
                                .overflow_hidden()
                                .bg(rgba(0xffffff55))
                                .child(div().h_full().w(relative(fraction)).bg(rgb(TEXT))),
                        )
                    })
                    .child(label(caption, 14., BODY).min_w_0().truncate()),
            );
            let ep = ep.clone();
            actions = actions.child(
                button
                    .h(px(42.))
                    .px_5()
                    .when(
                        self.keyboard
                            && self.focus.is_focused(w)
                            && self.state.series_focus == SeriesFocus::Hero,
                        |b| b.border_2().border_color(rgb(MUTED)),
                    )
                    .on_click(cx.listener(move |s, _, w, cx| s.play_item(ep.clone(), w, cx))),
            );
        } else if is_series && self.state.series_target_error {
            actions = actions
                .child(
                    label("Next Up unavailable", 14., MUTED)
                        .relative()
                        .children(accessibility::status("Next Up unavailable")),
                )
                .child(
                    pill("retry-next-up", "Retry Next Up", Some(REFRESH), false)
                        .on_click(cx.listener(|s, _, w, cx| s.load_series_target(false, w, cx))),
                );
        } else if !is_series {
            actions = actions.child(
                self.play_button(item, cx)
                    .when(self.keyboard && self.focus.is_focused(w), |b| {
                        b.border_2().border_color(rgb(MUTED))
                    }),
            );
            let sources = item.media_sources.as_deref().unwrap_or_default();
            let selected = item.detail_source(self.state.choice.source.as_deref());
            if sources.len() > 1 {
                let name = selected
                    .map(|s| s.name())
                    .unwrap_or_else(|| "Choose edition".into());
                let button = div()
                    .id("edition-picker")
                    .track_focus(&self.edition_focus)
                    .tab_stop(true)
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(34.))
                    .px_3()
                    .rounded_full()
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(ACTIVE)))
                    .focus(|d| d.border_2().border_color(rgb(TEXT)))
                    .max_w(px((width * 0.23).clamp(130., 280.)))
                    .min_w_0()
                    .child(label(name.clone(), 13., TEXT).min_w_0().truncate())
                    .bg(rgb(SURFACE))
                    .child(icon("\u{e70d}", 11.).text_color(rgb(MUTED)));
                actions = actions.child(tip(button, format!("Choose edition · {name}")).on_click(
                    cx.listener(|s, _, w, cx| s.toggle_detail_panel(DetailPanel::Editions, w, cx)),
                ));
            }
            let formats = item.file_formats(self.state.choice.source.as_deref());
            if let Some(strip) = format_strip::strip(&formats, false) {
                actions = actions.child(strip);
            }
        }
        actions = actions.child(
            pill("detail-more", "More", Some("\u{e712}"), PillStyle::Glass)
                .track_focus(&self.more_focus)
                .on_click(
                    cx.listener(|s, _, w, cx| s.toggle_detail_panel(DetailPanel::More, w, cx)),
                ),
        );
        let overview = item.overview.as_deref().unwrap_or("").trim();
        let copy = div()
            .min_w_0()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(if compact { 7. } else { 10. }))
            .child(self.wordmark(
                item,
                (width * 0.4).clamp(210., 470.),
                if has_backdrop {
                    (height * 0.22).clamp(50., 145.)
                } else {
                    64.
                },
            ))
            .children(metadata)
            .when(
                !overview.is_empty() && self.state.detail_panel != Some(DetailPanel::More),
                |d| {
                    d.child(
                        label(overview.to_owned(), if compact { 14. } else { 16. }, BODY)
                            .max_w(px((width * 0.54).clamp(430., 740.)))
                            .line_height(px(if compact { 19. } else { 22. }))
                            .line_clamp(2),
                    )
                },
            )
            .child(actions)
            .children(episode_context);
        let back = glyph_button("Back", BACK)
            .size(px(36.))
            .rounded_full()
            .bg(rgba(GLASS_WHITE_SOFT))
            .border_1()
            .border_color(rgba(GLASS_EDGE))
            .hover(|d| d.bg(rgba(GLASS_FILL)))
            .on_click(cx.listener(|s, _, w, cx| s.back(w, cx)));
        let hero = if has_backdrop {
            div()
                .relative()
                .w_full()
                .h(px(height + NAV_HEIGHT))
                .overflow_hidden()
                .bg(rgb(BG))
                .child(self.marquee_backdrop(item, w, size(px(width), px(height + NAV_HEIGHT))))
                .child(hero_scrim())
                .child(
                    div()
                        .absolute()
                        .left(px(inset))
                        .right(px(inset))
                        .bottom(px(if compact { 16. } else { 26. }))
                        .flex()
                        .items_end()
                        .gap_4()
                        .children(
                            self.detail_catalog_poster(item, (height * 0.32).clamp(78., 174.)),
                        )
                        .child(copy),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(inset))
                        .top(px(NAV_HEIGHT + NAV_TOP_INSET + 24.))
                        .child(back),
                )
        } else {
            div()
                .w_full()
                .pt(px(NAV_HEIGHT + NAV_TOP_INSET + 24.))
                .px(px(inset))
                .pb_4()
                .flex()
                .flex_col()
                .items_start()
                .gap_3()
                .child(back)
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_4()
                        .children(self.detail_catalog_poster(item, 104.).or_else(|| {
                            self.artwork.get(item.art(ArtKind::Poster)).map(|poster| {
                                div().flex_none().child(
                                    img(poster)
                                        .w(px(104.))
                                        .h(px(156.))
                                        .rounded_md()
                                        .object_fit(ObjectFit::Cover),
                                )
                            })
                        }))
                        .child(copy),
                )
        };
        let mut head = div().child(hero);
        match self.state.detail_panel {
            Some(DetailPanel::More) => {
                head = head.child(reveal_menu(
                    div()
                        .track_focus(&self.detail_panel_focus)
                        .tab_stop(false)
                        .child(
                            self.secondary_actions(
                                item,
                                div()
                                    .px(px(inset))
                                    .py_3()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap_2(),
                                cx,
                            ),
                        ),
                    "detail-actions-reveal",
                    point(px(0.), px(-4.)),
                ));
                if !overview.is_empty() {
                    head = head.child(
                        label(overview.to_owned(), 15., TEXT)
                            .px(px(inset))
                            .pb_3()
                            .max_w(px(850.)),
                    );
                }
            }
            Some(DetailPanel::Editions) => {
                head = head.child(reveal_menu(
                    div()
                        .track_focus(&self.detail_panel_focus)
                        .tab_stop(false)
                        .child(self.edition_choices(item, cx).px(px(inset)).py_3()),
                    "detail-editions-reveal",
                    point(px(0.), px(-4.)),
                ))
            }
            None => {}
        }
        head
    }

    fn toggle_detail_panel(&mut self, panel: DetailPanel, w: &mut Window, cx: &mut Context<Self>) {
        self.state.detail_panel = (self.state.detail_panel != Some(panel)).then_some(panel);
        self.state.info_open = false;
        if self.state.detail_panel.is_some() {
            let view = cx.weak_entity();
            w.on_next_frame(move |w, cx| {
                let _ = view.update(cx, |s, cx| {
                    if s.state.detail_panel == Some(panel) {
                        s.detail_current_focus.focus(w);
                        cx.notify();
                    }
                });
            });
        } else {
            self.detail_launcher(panel).focus(w);
        }
        cx.notify();
    }

    pub(in crate::ui) fn detail_launcher(&self, panel: DetailPanel) -> &FocusHandle {
        match panel {
            DetailPanel::More => &self.more_focus,
            DetailPanel::Editions => &self.edition_focus,
        }
    }

    pub(in crate::ui) fn detail_action_key(
        &mut self,
        key: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Screen::Detail(item, _) = &self.state.screen else {
            return false;
        };
        if !matches!(item.kind.as_str(), "Movie" | "Series") || self.state.editor.is_some() {
            return false;
        }
        if let Some(panel) = self.state.detail_panel {
            if matches!(key, "left" | "right" | "up" | "down") {
                if matches!(key, "left" | "up") {
                    w.focus_prev();
                } else {
                    w.focus_next();
                }
                if !self.detail_panel_focus.contains_focused(w, cx) {
                    self.state.detail_panel = None;
                    self.detail_launcher(panel).focus(w);
                }
                return true;
            }
            return key == "enter" && self.focus.is_focused(w);
        }
        if self.state.info_open || self.state.series_focus != SeriesFocus::Hero {
            return false;
        }
        let editions =
            item.kind == "Movie" && item.media_sources.as_ref().is_some_and(|v| v.len() > 1);
        let more = self.more_focus.is_focused(w);
        let edition = self.edition_focus.is_focused(w);
        match key {
            "right" if edition || (self.focus.is_focused(w) && !editions) => {
                self.more_focus.focus(w)
            }
            "right" if editions && self.focus.is_focused(w) => self.edition_focus.focus(w),
            "left" if more && editions => self.edition_focus.focus(w),
            "left" if more || edition => self.focus.focus(w),
            _ => return false,
        }
        self.keyboard = true;
        true
    }

    pub(in crate::ui) fn select_season(
        &mut self,
        id: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Screen::Detail(_, season) = &mut self.state.screen else {
            return;
        };
        if season.as_deref() != Some(id) {
            *season = Some(id.to_owned());
            self.state.offset = 0;
            self.state.selection = 0;
            self.state.page = None;
            self.load(w, cx);
        }
    }

    pub(in crate::ui) fn series_key(
        &mut self,
        key: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Screen::Detail(item, season) = &self.state.screen else {
            return false;
        };
        if item.kind != "Series" || self.state.detail_panel.is_some() || self.state.editor.is_some()
        {
            return false;
        }
        match (self.state.series_focus, key) {
            (SeriesFocus::Hero, "enter") => {
                if let Some(ep) = self.state.series_target.clone() {
                    self.play_item(ep, w, cx);
                }
            }
            (SeriesFocus::Hero, "down") if !self.state.seasons.is_empty() => {
                self.state.series_focus = SeriesFocus::Seasons;
                self.state.reveal_row(0);
            }
            (SeriesFocus::Seasons, "up") | (SeriesFocus::Episodes, "up")
                if self.state.series_focus == SeriesFocus::Seasons || self.state.selection == 0 =>
            {
                self.state.series_focus = if self.state.series_focus == SeriesFocus::Episodes
                    && !self.state.seasons.is_empty()
                {
                    SeriesFocus::Seasons
                } else {
                    SeriesFocus::Hero
                };
                if self.state.series_focus == SeriesFocus::Hero {
                    self.state.scroll.set_offset(point(px(0.), px(0.)));
                } else {
                    self.state.reveal_row(0);
                }
            }
            (SeriesFocus::Seasons, "left" | "right") => {
                if let Some(i) = self
                    .state
                    .seasons
                    .iter()
                    .position(|s| Some(&s.id) == season.as_ref())
                {
                    let next = if key == "left" {
                        i.saturating_sub(1)
                    } else {
                        (i + 1).min(self.state.seasons.len() - 1)
                    };
                    let id = self.state.seasons[next].id.clone();
                    self.select_season(&id, w, cx);
                }
            }
            (SeriesFocus::Hero | SeriesFocus::Seasons, "down")
            | (SeriesFocus::Seasons, "enter") => {
                if self
                    .state
                    .page
                    .as_ref()
                    .is_some_and(|p| !p.items.is_empty())
                {
                    self.state.series_focus = SeriesFocus::Episodes;
                    self.state.reveal_row(self.state.selection + 1);
                }
            }
            (SeriesFocus::Hero, "up" | "left" | "right") => {}
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::episode_timing;
    use crate::jellyfin::Item;

    #[test]
    fn episode_card_uses_the_saved_position_and_handles_missing_runtime() {
        let mut ep: Item = serde_json::from_value(serde_json::json!({
            "Id":"e", "Name":"Episode", "RunTimeTicks": 3_600_000_000_u64,
            "UserData":{"PlaybackPositionTicks":900_000_000}
        }))
        .unwrap();
        assert_eq!(episode_timing(&ep), (Some("5 min left".into()), Some(0.25)));
        ep.run_time_ticks = Some(1);
        assert_eq!(episode_timing(&ep), (Some("0 min left".into()), Some(1.)));
        ep.user_data = None;
        assert_eq!(episode_timing(&ep), (Some("1 min".into()), None));
        ep.run_time_ticks = None;
        assert_eq!(episode_timing(&ep), (None, None));
    }
}
