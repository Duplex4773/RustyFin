use super::*;
use crate::anilist::{Feed, Update};

impl Library {
    pub(super) fn anime_settings(&self, cx: &mut Context<Self>) -> Div {
        let signed = self.anime.viewer.is_some();
        let mut form = div()
            .max_w(px(640.))
            .p_4()
            .flex()
            .flex_col()
            .items_start()
            .gap_2()
            .child(title("AniList", 18.));
        if !signed {
            form = form
                .child(label(
                    "Connect your account to change your watchlist, progress and ratings.",
                    14.,
                    MUTED,
                ))
                .child(self.settings_input(&self.anime.app_id, "AniList application ID", false, cx))
                .child(
                    self.service_button(cx, "anime-auth", "Authorize PIN application", None, false)
                        .on_click(cx.listener(|s, _, _, cx| s.authorize_anime(cx))),
                )
                .child(self.settings_input(&self.anime.pin, "AniList PIN", true, cx));
        }
        if signed {
            let on = self.prefs.anime_sync;
            form = form.child(
                self.service_button(
                    cx,
                    "ani-sync",
                    if on {
                        "Sync completed playback · On"
                    } else {
                        "Sync completed playback · Off"
                    },
                    None,
                    on,
                )
                .on_click(cx.listener(|s, _, _, cx| {
                    s.prefs.anime_sync = !s.prefs.anime_sync;
                    s.save_preferences(cx);
                })),
            );
        }
        form.child(
            self.service_button(
                cx,
                "anime-connect",
                if signed {
                    "Disconnect AniList"
                } else {
                    "Connect AniList"
                },
                None,
                signed,
            )
            .on_click(cx.listener(|s, _, w, cx| s.pair_anime(w, cx))),
        )
    }
    pub(super) fn anime_view(&self, cx: &mut Context<Self>) -> Div {
        let mut body = div()
            .id("anime-scroll")
            .track_scroll(&self.state.scroll)
            .size_full()
            .overflow_y_scroll()
            .py_5()
            .px(px(CONTENT_GUTTER))
            .child(page_heading("Explore", 26.))
            .child(self.catalog_sources(true, cx));
        if let Some(m) = &self.anime.selected {
            let mut panel = div()
                .flex()
                .flex_col()
                .items_start()
                .gap_3()
                .py_3()
                .child(
                    glyph_button("Back to AniList", BACK)
                        .on_click(cx.listener(|s, _, w, cx| s.back(w, cx))),
                )
                .child(title(m.title.user_preferred.clone(), 24.));
            let mut info = Vec::new();
            if let Some(format) = &m.format {
                info.push(format.replace('_', " "));
            }
            if let Some(n) = m.episodes {
                info.push(format!("{n} episodes"));
            }
            if let Some(score) = m.average_score {
                info.push(format!("AniList {score}%"));
            }
            panel = panel.child(label(info.join(" · "), 15., MUTED));
            for (n, item) in self.anime.matching.iter().enumerate() {
                let item = item.clone();
                panel = panel.child(
                    pill(
                        ("ani-match", n),
                        format!("Open {} in Jellyfin", item.name),
                        None,
                        true,
                    )
                    .on_click(cx.listener(move |s, _, w, cx| s.open_item(item.clone(), w, cx))),
                );
            }
            if self.anime.viewer.is_some() {
                let entry = m.media_list_entry.as_ref();
                let status = entry.and_then(|r| r.status.as_deref());
                if status == Some("PLANNING") || entry.is_none() {
                    let planned = status == Some("PLANNING");
                    panel = panel.child(
                        pill(
                            "anime-plan",
                            if planned {
                                "Remove from watchlist"
                            } else {
                                "Add to watchlist"
                            },
                            None,
                            false,
                        )
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.update_anime(
                                if planned {
                                    Update::Remove
                                } else {
                                    Update::Plan
                                },
                                w,
                                cx,
                            )
                        })),
                    );
                }
                let progress = entry.and_then(|r| r.progress).unwrap_or(0);
                if m.episodes.is_none_or(|n| progress < n) {
                    let next = progress.saturating_add(1);
                    let done = m.episodes == Some(next) || m.format.as_deref() == Some("MOVIE");
                    let name = if done {
                        "Mark completed".into()
                    } else {
                        format!("Mark episode {next} watched")
                    };
                    panel =
                        panel.child(pill("ani-watched", name, None, false).on_click(cx.listener(
                            move |s, _, w, cx| s.update_anime(Update::Watched(next, done), w, cx),
                        )));
                }
                let mut scores = div().flex().items_center().flex_wrap().gap_1().child(label(
                    "Your score",
                    14.,
                    MUTED,
                ));
                for n in 1..=10 {
                    scores = scores.child(
                        pill(
                            ("ani-score", n as usize),
                            n.to_string(),
                            None,
                            entry.and_then(|e| e.score) == Some(n as f64),
                        )
                        .on_click(
                            cx.listener(move |s, _, w, cx| s.update_anime(Update::Rate(n), w, cx)),
                        ),
                    );
                }
                panel = panel.child(scores);
            }
            let url = m.site_url.clone();
            if url.starts_with("https://anilist.co/anime/") {
                panel = panel.child(
                    pill("ani-site", "View on AniList", None, false)
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                );
            }
            return div().flex_1().min_w_0().child(body.child(panel));
        }
        let mut tabs = div().py_3().flex().gap_2();
        for (i, name) in ["Trending", "Popular", "Watchlist"].into_iter().enumerate() {
            if i == 2 && self.anime.viewer.is_none() {
                continue;
            }
            let active = matches!(
                (&self.anime.feed, i),
                (Feed::Trending, 0) | (Feed::Popular, 1) | (Feed::Watchlist, 2)
            ) && self.anime.term.is_empty();
            tabs = tabs.child(
                pill(("ani-feed", i), name, None, active).on_click(cx.listener(
                    move |s, _, w, cx| {
                        s.anime.feed = match i {
                            0 => Feed::Trending,
                            1 => Feed::Popular,
                            _ => Feed::Watchlist,
                        };
                        s.anime.term.clear();
                        s.anime.search.update(cx, |i, cx| i.set_value("", w, cx));
                        s.anime.page = 1;
                        s.load_anime(w, cx);
                    },
                )),
            );
        }
        body = body.child(tabs).child(Input::new(&self.anime.search));
        for (i, m) in self.anime.rows.iter().enumerate() {
            let id = m.id;
            body = body.child(
                pill(
                    ("ani-title", i),
                    m.title.user_preferred.clone(),
                    None,
                    false,
                )
                .my_1()
                .on_click(
                    cx.listener(move |s, _, w, cx| s.navigate(Screen::AnimeTitle(id), w, cx)),
                ),
            );
        }
        let mut pages = div().py_3().flex().gap_2();
        if self.anime.page > 1 {
            pages = pages.child(pill("ani-prev", "Previous page", None, false).on_click(
                cx.listener(|s, _, w, cx| {
                    s.anime.page -= 1;
                    s.load_anime(w, cx);
                }),
            ));
        }
        if self.anime.more {
            pages = pages.child(
                pill("ani-next", "Next page", None, false).on_click(cx.listener(|s, _, w, cx| {
                    s.anime.page += 1;
                    s.load_anime(w, cx);
                })),
            );
        }
        div().flex_1().min_w_0().child(body.child(pages))
    }
    pub(super) fn catalog_sources(&self, anime: bool, cx: &mut Context<Self>) -> Div {
        div()
            .py_2()
            .flex()
            .gap_2()
            .child(
                pill("source-seerr", "Seerr", None, !anime)
                    .on_click(cx.listener(|s, _, w, cx| s.navigate(Screen::Explore, w, cx))),
            )
            .child(
                pill("source-anilist", "AniList", None, anime)
                    .on_click(cx.listener(|s, _, w, cx| s.navigate(Screen::Anime, w, cx))),
            )
    }
}
