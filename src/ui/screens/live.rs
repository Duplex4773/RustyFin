use super::*;
use crate::jellyfin::live::local_time;

impl Library {
    pub(super) fn live_view(&self, cx: &mut Context<Self>) -> Div {
        let mut tabs = div().flex().gap_2().flex_wrap();
        for (i, (name, screen, active)) in [
            (
                "Channels",
                Screen::Live(false),
                matches!(self.state.screen, Screen::Live(false)),
            ),
            (
                "Favorites",
                Screen::Live(true),
                matches!(self.state.screen, Screen::Live(true)),
            ),
            (
                "Scheduled",
                Screen::Timers(false),
                matches!(self.state.screen, Screen::Timers(false)),
            ),
            (
                "Series rules",
                Screen::Timers(true),
                matches!(self.state.screen, Screen::Timers(true)),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            tabs = tabs.child(
                pill(("live-tab", i), name, None, active)
                    .on_click(cx.listener(move |s, _, w, cx| s.navigate(screen.clone(), w, cx))),
            );
        }
        let mut list = div()
            .id("tv-guide")
            .overflow_y_scroll()
            .track_scroll(&self.state.scroll)
            .flex_1()
            .min_h_0()
            .px_5();
        if let Screen::Guide(channel) = &self.state.screen {
            let target = *channel.clone();
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .py_3()
                    .child(
                        glyph_button("Back to channels", BACK)
                            .on_click(cx.listener(|s, _, w, cx| s.back(w, cx))),
                    )
                    .child(title(channel.name.clone(), 21.))
                    .child(pill("tune", "Watch live", Some(PLAY), true).on_click(
                        cx.listener(move |s, _, w, cx| s.play_item(target.clone(), w, cx)),
                    )),
            );
        }
        if let Screen::Timers(series) = &self.state.screen {
            let series = *series;
            for (i, timer) in self.state.timers.iter().enumerate() {
                let id = timer.id.clone();
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .py_3()
                        .gap_3()
                        .child(
                            div()
                                .flex_1()
                                .child(title(timer.name.clone().unwrap_or_default(), 16.))
                                .child(label(
                                    format!(
                                        "{}   {}",
                                        local_time(timer.start_date.as_deref()),
                                        timer.channel_name.as_deref().unwrap_or("")
                                    ),
                                    13.,
                                    MUTED,
                                )),
                        )
                        .child(
                            pill(
                                ("cancel-record", i),
                                if series {
                                    "Cancel series rule"
                                } else {
                                    "Cancel recording"
                                },
                                None,
                                false,
                            )
                            .on_click(cx.listener(
                                move |s, _, w, cx| {
                                    if let Some(server) = s.server.clone() {
                                        let id = id.clone();
                                        s.work(
                                            w,
                                            cx,
                                            move || server.cancel_timer(&id, series),
                                            |s, _, w, cx| {
                                                s.load(w, cx);
                                                Ok(())
                                            },
                                        );
                                    }
                                },
                            )),
                        ),
                );
            }
            if self.state.timers.is_empty() {
                list = list.child(self.empty("No recordings scheduled."));
            }
        } else if let Some(page) = &self.state.page
            && !page.items.is_empty()
        {
            for (i, item) in page.items.iter().enumerate() {
                let mut row = div()
                    .id(("tv-entry", i))
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .py_3();
                if matches!(self.state.screen, Screen::Guide(_)) {
                    let time = local_time(item.start_date.as_deref());
                    row = row.child(
                        div()
                            .flex_1()
                            .child(title(item.name.clone(), 16.))
                            .child(label(time, 13., MUTED)),
                    );
                    for (series, name) in [(false, "Record"), (true, "Record series")] {
                        if series && item.is_series != Some(true) {
                            continue;
                        }
                        let active = if series {
                            item.series_timer_id.is_some()
                        } else {
                            item.timer_id.is_some()
                        };
                        if active {
                            row = row.child(label("Scheduled", 13., TEXT));
                            continue;
                        }
                        let id = item.id.clone();
                        row = row.child(
                            pill(
                                (if series { "record-series" } else { "record" }, i),
                                name,
                                None,
                                false,
                            )
                            .on_click(cx.listener(
                                move |s, _, w, cx| {
                                    if s.busy {
                                        return;
                                    }
                                    if let Some(server) = s.server.clone() {
                                        let id = id.clone();
                                        s.work(
                                            w,
                                            cx,
                                            move || server.record(&id, series),
                                            |s, _, w, cx| {
                                                s.load(w, cx);
                                                Ok(())
                                            },
                                        );
                                    }
                                },
                            )),
                        );
                    }
                } else {
                    let channel = item.clone();
                    let favorite =
                        item.user_data.as_ref().and_then(|d| d.is_favorite) == Some(true);
                    row = row
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(
                                    title(
                                        format!(
                                            "{}  {}",
                                            item.channel_number.as_deref().unwrap_or(""),
                                            item.name
                                        ),
                                        16.,
                                    )
                                    .truncate(),
                                )
                                .child(
                                    label(
                                        item.current_program
                                            .as_ref()
                                            .map(|p| p.name.clone())
                                            .unwrap_or_default(),
                                        14.,
                                        MUTED,
                                    )
                                    .truncate(),
                                ),
                        )
                        .child(
                            pill(("tune-channel", i), "Watch", Some(PLAY), true).on_click(
                                cx.listener(move |s, _, w, cx| s.play_item(channel.clone(), w, cx)),
                            ),
                        );
                    let channel = item.clone();
                    row = row.child(pill(("guide", i), "Guide", None, false).on_click(
                        cx.listener(move |s, _, w, cx| {
                            if s.adopt_server(&channel, w, cx) {
                                s.navigate(Screen::Guide(Box::new(channel.clone())), w, cx)
                            }
                        }),
                    ));
                    let id = item.id.clone();
                    let origin = item.origin.clone();
                    row = row.child(
                        pill(
                            ("channel-favorite", i),
                            if favorite { "Favorited" } else { "Favorite" },
                            None,
                            favorite,
                        )
                        .on_click(cx.listener(move |s, _, w, cx| {
                            if let Some(server) = s.source_server(origin.as_deref()) {
                                let id = id.clone();
                                s.work(
                                    w,
                                    cx,
                                    move || server.set_user_flag(&id, true, !favorite),
                                    |s, _, w, cx| {
                                        s.load(w, cx);
                                        Ok(())
                                    },
                                );
                            }
                        })),
                    );
                }
                list = list.child(row);
            }
            list = list.child(self.paging(cx));
        } else {
            list = list.child(
                self.empty(if matches!(self.state.screen, Screen::Guide(_)) {
                    "No upcoming programmes in this guide."
                } else {
                    "No channels available from this Jellyfin server."
                }),
            );
        }
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .p_5()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(title("Live TV", 26.))
                    .child(tabs),
            )
            .child(list)
    }
}
