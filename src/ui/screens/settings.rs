use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum SettingsPage {
    Overview,
    Playback,
    Display,
    Services,
    Profiles,
    Shortcuts,
    Home,
}

impl SettingsPage {
    fn title(self) -> &'static str {
        match self {
            Self::Overview => "Settings",
            Self::Playback => "Video playback",
            Self::Display => "Playback display",
            Self::Services => "Services",
            Self::Profiles => "Profiles",
            Self::Shortcuts => "Video player controls",
            Self::Home => "Customise Home",
        }
    }
}

pub(super) fn group() -> Div {
    glass_panel(div())
        .flex_none()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(2.))
        .p(px(7.))
}

pub(super) fn rows<const N: usize>(items: [Stateful<Div>; N]) -> Div {
    group().children(items)
}

pub(super) fn heading(name: &'static str) -> Div {
    accessibility::heading(label(name, 16., TEXT), name.into(), 2)
        .font_weight(FontWeight::SEMIBOLD)
        .mb(px(9.))
}

pub(super) fn section_stack() -> Div {
    div().w_full().min_w_0().flex().flex_col().gap(px(22.))
}

pub(super) fn section_card(name: &'static str, card: Div) -> Div {
    div().w_full().child(heading(name)).child(card)
}

pub(super) fn settings_columns(wide: bool, first: Div, second: Div) -> Div {
    div()
        .w_full()
        .flex()
        .gap(px(20.))
        .items_start()
        .when(!wide, |d| d.flex_col())
        .child(first.when(wide, |d| d.flex_1().w_auto()))
        .child(second.when(wide, |d| d.flex_1().w_auto()))
}

pub(super) enum SettingAdornment {
    Content(Div),
    Switch(bool),
}

impl From<Div> for SettingAdornment {
    fn from(value: Div) -> Self {
        Self::Content(value)
    }
}

impl Library {
    pub(super) fn settings_row(
        &self,
        cx: &App,
        id: impl Into<ElementId>,
        glyph: &'static str,
        name: impl Into<SharedString>,
        description: impl Into<SharedString>,
        trailing: impl Into<SettingAdornment>,
    ) -> Stateful<Div> {
        let description = description.into();
        let name = name.into();
        let (trailing, state) = match trailing.into() {
            SettingAdornment::Content(div) => (div, None),
            SettingAdornment::Switch(on) => (draw_toggle(on), Some(on)),
        };
        let button = div()
            .id(id)
            .group("setting-row")
            .focusable()
            .tab_stop(true)
            .flex_none()
            .w_full()
            .min_h(px(60.))
            .px(px(13.))
            .py(px(10.))
            .flex()
            .items_center()
            .gap_3()
            .rounded(px(10.))
            .border_1()
            .border_color(transparent_black())
            .cursor_pointer()
            .hover(|d| d.bg(rgba(HOVER_FILL)).border_color(rgba(REST_EDGE)))
            .active(|d| d.bg(rgba(PRESS_FILL)))
            .focus(|d| d.bg(rgba(MICRO_FILL)).border_color(rgb(FOCUS)))
            .child(
                div()
                    .size(px(32.))
                    .rounded(px(8.))
                    .bg(rgba(HOVER_FILL))
                    .text_color(rgb(BODY))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(glyph, 19.)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(title(name.clone(), 15.).line_height(px(20.)))
                    .when(!description.is_empty(), |d| {
                        d.child(label(description, 13., META).line_height(px(17.)))
                    }),
            )
            .child(trailing.flex_none());
        let button = match state {
            Some(on) => accessibility::switch(button, name, on),
            None => accessibility::button(button, name, None, false),
        };
        self.settings_button(button, cx)
    }
}

pub(super) fn toggle(on: bool) -> SettingAdornment {
    SettingAdornment::Switch(on)
}

fn draw_toggle(on: bool) -> Div {
    let target = if on { 1. } else { 0. };
    let animate = scrolling::animations_enabled();
    div().w(px(38.)).h(px(22.)).child(
        canvas(
            |_, _, _| (),
            move |bounds, _, w, _| {
                w.with_global_id("switch-travel".into(), |id, w| {
                    w.with_element_state(id, |state: Option<(f32, f32, Instant)>, w| {
                        let now = Instant::now();
                        let (mut from, previous, mut start) =
                            state.unwrap_or((target, target, now));
                        let t = (now.duration_since(start).as_secs_f32() / 0.15).min(1.);
                        let mut progress = from + (previous - from) * t * t * (3. - 2. * t);
                        if previous != target {
                            from = progress;
                            start = now;
                        }
                        if !animate {
                            progress = target;
                            from = target;
                        }
                        if (target - progress).abs() > 0.001 {
                            w.request_animation_frame();
                        }
                        let mix = |a, b| {
                            rgb(a).blend(Rgba {
                                a: progress,
                                ..rgb(b)
                            })
                        };
                        w.paint_quad(quad(
                            bounds,
                            px(11.),
                            mix(0x242a36, TEXT),
                            px(1.),
                            mix(0x626d7e, TEXT),
                            BorderStyle::Solid,
                        ));
                        let thumb = Bounds::new(
                            bounds.origin + point(px(3. + 16. * progress), px(3.)),
                            size(px(16.), px(16.)),
                        );
                        w.paint_quad(fill(thumb, mix(0xb7c2d2, BG)).corner_radii(px(8.)));
                        ((), (from, target, start))
                    });
                });
            },
        )
        .size_full(),
    )
}

fn arrow() -> Div {
    div()
        .size(px(28.))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(META))
        .group_hover("setting-row", |d| {
            d.bg(rgba(HOVER_FILL)).text_color(rgb(TEXT))
        })
        .child(icon(NEXT, 13.))
}

impl Library {
    fn category(
        &self,
        page: SettingsPage,
        glyph: &'static str,
        description: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.settings_row(cx, page.title(), glyph, page.title(), description, arrow())
            .on_click(cx.listener(move |s, _, w, cx| s.navigate(Screen::Settings(page), w, cx)))
    }

    fn settings_overview(&self, wide: bool, cx: &mut Context<Self>) -> Div {
        use SettingsPage::*;
        let host = self.server.as_ref().map_or("", Server::host);
        let experience = rows([
            self.category(
                Playback,
                PLAY,
                "Dolby Vision, audio output and subtitles",
                cx,
            ),
            self.category(Display, "\u{e7f4}", "Monitor and Windows HDR settings", cx),
            self.category(Home, "\u{e713}", "Arrange shelves and pin shortcuts", cx),
            self.settings_row(
                cx,
                "rotation",
                "\u{e91b}",
                "Rotate featured titles",
                "Cycle through featured titles on Discover",
                toggle(self.rotate),
            )
            .on_click(cx.listener(|s, _, _, cx| {
                s.rotate = !s.rotate;
                s.last_slide = Instant::now();
                s.save_preferences(cx);
            })),
        ]);
        let connections = rows([
            self.category(Services, REFRESH, "Seerr, AniList, MDBList and Discord", cx),
            self.settings_row(
                cx,
                "add-profile",
                "\u{e710}",
                "Add connection",
                "Connect another Jellyfin account or server",
                arrow(),
            )
            .on_click(cx.listener(|s, _, w, cx| s.clear_session(w, cx))),
            self.category(Profiles, ACCOUNT, format!("Manage accounts · {host}"), cx),
            self.settings_row(
                cx,
                "search-all-servers",
                SEARCH,
                "Search all servers",
                "Include connected libraries in Discover and search",
                toggle(self.prefs.all_servers),
            )
            .on_click(cx.listener(|s, _, _, cx| {
                s.prefs.all_servers = !s.prefs.all_servers;
                s.home_dirty = true;
                s.save_preferences(cx);
            })),
        ]);
        let downloads = rows([self
            .settings_row(
                cx,
                "settings-downloads",
                "\u{e8b7}",
                "Offline library",
                "View saved media and manage downloads",
                arrow(),
            )
            .on_click(cx.listener(|s, _, w, cx| s.navigate(Screen::Downloads, w, cx)))]);
        let controls =
            rows([self.category(Shortcuts, "\u{e765}", "Customize keyboard shortcuts", cx)]);
        let advanced = rows([
            self.settings_row(
                cx,
                "export-prefs",
                "\u{e898}",
                "Export settings",
                "Save this profile’s preferences to a file",
                arrow(),
            )
            .on_click(cx.listener(|s, _, w, cx| s.settings_file(true, w, cx))),
            self.settings_row(
                cx,
                "import-prefs",
                "\u{e896}",
                "Import settings",
                "Restore preferences from a RustyFin settings file",
                arrow(),
            )
            .on_click(cx.listener(|s, _, w, cx| s.settings_file(false, w, cx))),
        ]);
        settings_columns(
            wide,
            section_stack()
                .child(section_card("Playback and appearance", experience))
                .child(section_card("Controls", controls))
                .child(section_card("Downloads", downloads)),
            section_stack()
                .child(section_card("Connections", connections))
                .child(section_card("Advanced", advanced)),
        )
    }

    fn profile_settings(&self, cx: &mut Context<Self>) -> Div {
        let active = self.server.as_ref().and_then(Server::identity);
        let mut accounts = group();
        for (i, account) in self.accounts.iter().enumerate() {
            let selected =
                active.is_some_and(|a| a.user == account.user && a.server == account.server);
            let account = account.clone();
            accounts = accounts.child(
                self.settings_row(
                    cx,
                    ("profile", i),
                    ACCOUNT,
                    account
                        .username
                        .clone()
                        .unwrap_or_else(|| "Jellyfin".into()),
                    account.server.clone(),
                    if selected {
                        icon("\u{e73e}", 16.)
                    } else {
                        arrow()
                    },
                )
                .on_click(cx.listener(move |s, _, w, cx| s.choose_account(account.clone(), w, cx))),
            );
        }
        accounts.child(
            self.settings_row(
                cx,
                "sign-out",
                "\u{f3b1}",
                "Sign out",
                "Disconnect the current Jellyfin account",
                arrow(),
            )
            .on_click(cx.listener(|s, _, w, cx| s.sign_out(w, cx))),
        )
    }

    fn shortcuts_settings(&self, cx: &mut Context<Self>) -> Div {
        let mut list = group();
        for &(id, name, default) in crate::ui::customization::SHORTCUTS {
            let value = if self.rebind == Some(id) {
                "Press a key…".into()
            } else {
                self.prefs.binding(id, default)
            };
            list = list.child(
                self.settings_row(cx, id, "\u{e765}", name, "", label(value, 14., TEXT))
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.rebind = Some(id);
                        s.focus.focus(w);
                        cx.notify();
                    })),
            );
        }
        list.child(
            self.settings_row(
                cx,
                "reset-keys",
                REFRESH,
                "Restore default shortcuts",
                "",
                arrow(),
            )
            .on_click(cx.listener(|s, _, _, cx| {
                s.rebind = None;
                s.prefs.shortcuts.clear();
                s.save_preferences(cx);
            })),
        )
    }

    pub(super) fn settings(&self, page: SettingsPage, w: &Window, cx: &mut Context<Self>) -> Div {
        self.state.settings_nav.begin_layout();
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .mb(px(25.))
            .when(page != SettingsPage::Overview, |d| {
                d.child(
                    self.settings_button(glyph_button("Back", BACK), cx)
                        .on_click(cx.listener(|s, _, w, cx| s.back(w, cx))),
                )
            })
            .child(page_heading(page.title(), 28.));
        let wide = w.viewport_size().width >= px(1020.);
        let card = |form: Div| glass_panel(div()).w_full().p_2().child(form);
        let body = match page {
            SettingsPage::Overview => self.settings_overview(wide, cx),
            SettingsPage::Profiles => self.profile_settings(cx),
            SettingsPage::Shortcuts => self.shortcuts_settings(cx),
            SettingsPage::Home => self.home_settings(wide, cx),
            SettingsPage::Display => card(self.display_settings(cx).px_4().py_3()),
            SettingsPage::Playback => self.playback_settings(cx),
            SettingsPage::Services => section_stack()
                .child(card(self.seerr_settings(cx)))
                .child(card(self.anime_settings(cx)))
                .child(card(self.other_services(cx))),
        };
        div().flex_1().min_w_0().min_h_0().child(
            div()
                .id("settings-scroll")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.state.scroll)
                .px(px(CONTENT_GUTTER))
                .pt(px(22.))
                .pb(px(36.))
                .child(
                    div().flex().justify_center().child(
                        div()
                            .w_full()
                            .min_w_0()
                            .max_w(px(
                                if matches!(page, SettingsPage::Overview | SettingsPage::Home) {
                                    1040.
                                } else {
                                    800.
                                },
                            ))
                            .child(header)
                            .child(body),
                    ),
                ),
        )
    }
}
