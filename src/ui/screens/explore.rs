use super::*;
impl Library {
    pub(super) fn other_services(&self, cx: &mut Context<Self>) -> Div {
        let linked = self.services.ratings.is_some();
        let sharing = self.prefs.discord_app.is_some();
        let ratings_label = if linked {
            "Disconnect MDBList"
        } else {
            "Connect MDBList"
        };
        let presence_label = if sharing {
            "Turn off activity sharing"
        } else {
            "Share playback activity"
        };
        let ratings = pill("mdblist", ratings_label, None, linked)
            .on_click(cx.listener(|s, _, w, cx| s.connect_ratings(w, cx)));
        let presence = pill("discord", presence_label, None, sharing)
            .on_click(cx.listener(|s, _, w, cx| s.connect_discord(w, cx)));
        let mut panel = div()
            .p_4()
            .max_w(px(640.))
            .flex()
            .flex_col()
            .items_start()
            .gap_2()
            .child(title("MDBList ratings", 18.))
            .when(!linked, |d| d.child(Input::new(&self.services.mdb_input)))
            .child(ratings)
            .child(title("Discord Rich Presence", 18.));
        if !sharing {
            let description = "Share the current title through the Discord desktop client using your Discord application ID.";
            panel = panel
                .child(label(description, 14., MUTED))
                .child(Input::new(&self.services.discord_input));
        }
        panel.child(presence)
    }
    pub(super) fn seerr_settings(&self, cx: &mut Context<Self>) -> Div {
        let mut panel = div()
            .p_4()
            .max_w(px(640.))
            .flex()
            .flex_col()
            .items_start()
            .gap_2()
            .child(title("Seerr", 18.));
        if self.explore.client.is_some() {
            return panel.child(
                pill("disconnect-seerr", "Disconnect Seerr", None, false)
                    .on_click(cx.listener(|s, _, w, cx| s.disconnect_seerr(w, cx))),
            );
        }
        panel = panel
            .child(label(
                "Connect a Seerr server to discover and request movies and shows.",
                14.,
                MUTED,
            ))
            .child(Input::new(&self.explore.address))
            .child(Input::new(&self.explore.key))
            .child(
                pill("connect-seerr", "Connect Seerr", None, true)
                    .on_click(cx.listener(|s, _, w, cx| s.connect_seerr(w, cx))),
            );
        panel
    }
    pub(super) fn explore_view(&self, cx: &mut Context<Self>) -> Div {
        let mut body = div()
            .id("explore-scroll")
            .size_full()
            .overflow_y_scroll()
            .p_5()
            .child(title("Explore", 26.))
            .child(self.catalog_sources(false, cx));
        if self.explore.client.is_none() {
            return div()
                .flex_1()
                .min_w_0()
                .child(body.child(self.seerr_settings(cx)));
        }
        if let Some(selected) = &self.explore.selected {
            let mut form = div()
                .flex()
                .flex_col()
                .items_start()
                .gap_3()
                .py_3()
                .child(
                    glyph_button("Back to Explore", BACK)
                        .on_click(cx.listener(|s, _, w, cx| s.back(w, cx))),
                )
                .child(title(selected.name().to_owned(), 30.))
                .child(label(
                    selected.overview.clone().unwrap_or_default(),
                    15.,
                    TEXT,
                ));
            for (i, matched) in self.explore.matching.iter().enumerate() {
                let item = matched.clone();
                form = form.child(
                    pill(
                        ("match", i),
                        format!("Open {} in Jellyfin", item.name),
                        None,
                        true,
                    )
                    .on_click(cx.listener(move |s, _, w, cx| s.open_item(item.clone(), w, cx))),
                );
            }
            let opt = &self.explore.options;
            form = form
                .child(
                    pill("request-4k", "4K", None, opt.uhd).on_click(cx.listener(|s, _, _, cx| {
                        s.explore.options.uhd = !s.explore.options.uhd;
                        s.explore.options.server_id = None;
                        s.explore.options.profile_id = None;
                        s.explore.options.root_folder = None;
                        s.explore.destinations = Default::default();
                        s.explore.sent = false;
                        cx.notify();
                    })),
                )
                .child(label(selected.status_name(opt.uhd), 14., MUTED));
            if selected.media_type == "tv" {
                let mut seasons = div().flex().flex_wrap().gap_2().child(
                    pill("all-seasons", "All seasons", None, opt.seasons.is_empty()).on_click(
                        cx.listener(|s, _, _, cx| {
                            s.explore.options.seasons.clear();
                            cx.notify();
                        }),
                    ),
                );
                for season in &selected.seasons {
                    let n = season.season_number;
                    seasons = seasons.child(
                        pill(
                            ("request-season", n),
                            season.name.clone(),
                            None,
                            opt.seasons.contains(&n),
                        )
                        .on_click(cx.listener(move |s, _, _, cx| {
                            let seasons = &mut s.explore.options.seasons;
                            if seasons.contains(&n) {
                                seasons.retain(|v| *v != n);
                            } else {
                                seasons.push(n);
                            }
                            cx.notify();
                        })),
                    );
                }
                form = form.child(seasons);
            }
            let mut servers = div().flex().flex_wrap().gap_2().child(
                pill(
                    "default-destination",
                    "Server default",
                    None,
                    opt.server_id.is_none(),
                )
                .on_click(cx.listener(|s, _, _, cx| {
                    s.explore.options.server_id = None;
                    s.explore.options.root_folder = None;
                    s.explore.options.profile_id = None;
                    s.explore.destinations = Default::default();
                    cx.notify();
                })),
            );
            for dest in self.explore.servers.iter().filter(|d| d.is4k == opt.uhd) {
                let id = dest.id;
                let kind = selected.media_type.clone();
                servers = servers.child(
                    pill(
                        ("destination", id),
                        dest.name.clone(),
                        None,
                        opt.server_id == Some(id),
                    )
                    .on_click(cx.listener(move |s, _, w, cx| {
                        if let Some(client) = s.explore.client.clone() {
                            let kind = kind.clone();
                            s.work(
                                w,
                                cx,
                                move || client.options(&kind, id),
                                move |s, options, _, _| {
                                    s.explore.destinations = options;
                                    s.explore.options.server_id = Some(id);
                                    s.explore.options.root_folder = None;
                                    s.explore.options.profile_id = None;
                                    Ok(())
                                },
                            );
                        }
                    })),
                );
            }
            if !self.explore.servers.is_empty() {
                form = form.child(servers);
            }
            let mut overrides = div().flex().flex_wrap().gap_2();
            for profile in &self.explore.destinations.profiles {
                let id = profile.id;
                overrides = overrides.child(
                    pill(
                        ("quality-profile", id),
                        profile.name.clone(),
                        None,
                        opt.profile_id == Some(id),
                    )
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.explore.options.profile_id = Some(id);
                        cx.notify();
                    })),
                );
            }
            for (i, folder) in self.explore.destinations.root_folders.iter().enumerate() {
                let path = folder.path.clone();
                overrides = overrides.child(
                    pill(
                        ("root-folder", i),
                        path.clone(),
                        None,
                        opt.root_folder.as_ref() == Some(&path),
                    )
                    .on_click(cx.listener(move |s, _, _, cx| {
                        s.explore.options.root_folder = Some(path.clone());
                        cx.notify();
                    })),
                );
            }
            form = form.child(overrides);
            if self.explore.sent {
                form = form.child(label("Request submitted", 15., TEXT));
            } else if !self.busy
                && (matches!(selected.status(opt.uhd), 0 | 1 | 7)
                    || (selected.media_type == "tv" && selected.status(opt.uhd) == 4))
            {
                let item = selected.clone();
                form = form.child(
                    pill("request-media", "Submit request", None, true).on_click(cx.listener(
                        move |s, _, w, cx| {
                            if s.busy || s.explore.sent {
                                return;
                            }
                            if let Some(client) = s.explore.client.clone() {
                                let item = item.clone();
                                let opt = s.explore.options.clone();
                                s.work(
                                    w,
                                    cx,
                                    move || client.submit(&item, &opt),
                                    |s, _, _, _| {
                                        s.explore.sent = true;
                                        Ok(())
                                    },
                                );
                            }
                        },
                    )),
                );
            }
            body = body.child(form);
        } else {
            let mut tabs = div().flex().gap_2().py_3();
            for (tag, name) in [
                ("trending", "Trending"),
                ("movies", "Popular movies"),
                ("tv", "Popular TV"),
            ] {
                tabs = tabs.child(
                    pill(
                        name,
                        name,
                        None,
                        self.explore.section == tag && self.explore.term.is_empty(),
                    )
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.explore.term.clear();
                        s.explore.search.update(cx, |i, cx| i.set_value("", w, cx));
                        s.explore.section = tag;
                        s.explore.page = 1;
                        s.load_explore(w, cx);
                    })),
                );
            }
            body = body
                .child(tabs)
                .child(Input::new(&self.explore.search).prefix(icon(SEARCH, 16.)));
            if self.explore.results.results.is_empty() {
                body = body.child(self.empty("No titles found."));
            }
            for (i, entry) in self.explore.results.results.iter().enumerate() {
                let item = entry.clone();
                let date = entry
                    .release_date
                    .as_deref()
                    .or(entry.first_air_date.as_deref())
                    .unwrap_or("");
                body = body.child(
                    div()
                        .id(("explore-title", i))
                        .p_3()
                        .rounded_md()
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(SURFACE)))
                        .child(title(entry.name().to_owned(), 17.))
                        .child(label(
                            format!(
                                "{}  ·  {}  ·  {}",
                                if entry.media_type == "movie" {
                                    "Movie"
                                } else {
                                    "TV show"
                                },
                                date,
                                entry.status_name(false)
                            ),
                            13.,
                            MUTED,
                        ))
                        .on_click(
                            cx.listener(move |s, _, w, cx| s.explore_title(item.clone(), w, cx)),
                        ),
                );
            }
            let mut pages = div().flex().gap_2().items_center().py_2();
            for (next, name) in [(false, "Previous"), (true, "Next")] {
                if (!next && self.explore.page > 1)
                    || (next && self.explore.page < self.explore.results.total_pages)
                {
                    pages = pages.child(pill(name, name, None, false).on_click(cx.listener(
                        move |s, _, w, cx| {
                            if next {
                                s.explore.page += 1;
                            } else {
                                s.explore.page = s.explore.page.saturating_sub(1).max(1);
                            }
                            s.load_explore(w, cx);
                        },
                    )));
                }
            }
            body = body.child(pages);
        }
        div().flex_1().min_w_0().min_h_0().child(body)
    }
}
