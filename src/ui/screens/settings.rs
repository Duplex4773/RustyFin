use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum SettingsPage {
    Overview,
    Playback,
    Display,
    Services,
    Profiles,
    Shortcuts,
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
        }
    }
}

pub(super) fn group() -> Div {
    div()
        .flex_none()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(2.))
        .rounded(px(16.))
        .overflow_hidden()
}

pub(super) fn rows<const N: usize>(items: [Stateful<Div>; N]) -> Div {
    group().children(items.into_iter().enumerate().map(|(index, item)| {
        item.when(index == 0, |d| d.rounded_t(px(16.)))
            .when(index + 1 == N, |d| d.rounded_b(px(16.)))
    }))
}

pub(super) fn heading(name: &'static str) -> Div {
    label(name, 14., MUTED)
        .font_weight(FontWeight::SEMIBOLD)
        .mt(px(24.))
        .mb(px(7.))
}

pub(super) fn row(
    id: impl Into<ElementId>,
    glyph: &'static str,
    name: impl Into<SharedString>,
    description: impl Into<SharedString>,
    trailing: Div,
) -> Stateful<Div> {
    let description = description.into();
    div()
        .id(id)
        .focusable()
        .tab_stop(true)
        .flex_none()
        .w_full()
        .min_h(px(54.))
        .px_3()
        .py(px(8.))
        .flex()
        .items_center()
        .gap_3()
        .rounded(px(4.))
        .bg(rgb(SURFACE))
        .cursor_pointer()
        .hover(|d| d.bg(rgb(0x202126)))
        .focus(|d| d.bg(rgb(ACTIVE)).border_1().border_color(rgb(MUTED)))
        .child(
            div()
                .w(px(24.))
                .flex_none()
                .flex()
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
                .child(label(name, 15., TEXT))
                .when(!description.is_empty(), |d| {
                    d.child(label(description, 13., 0xbfc0c5))
                }),
        )
        .child(trailing.flex_none())
}

pub(super) fn toggle(on: bool) -> Div {
    div()
        .w(px(39.))
        .h(px(23.))
        .rounded_full()
        .border_2()
        .border_color(rgb(if on { TEXT } else { 0x37393f }))
        .bg(rgb(if on { TEXT } else { SURFACE }))
        .flex()
        .items_center()
        .px(px(3.))
        .when(on, |d| d.justify_end())
        .child(
            div()
                .size(px(15.))
                .rounded_full()
                .bg(rgb(if on { BG } else { 0x6e7076 })),
        )
}

fn arrow() -> Div {
    icon(NEXT, 14.)
}

impl Library {
    fn category(
        &self,
        page: SettingsPage,
        glyph: &'static str,
        description: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        row(page.title(), glyph, page.title(), description, arrow())
            .on_click(cx.listener(move |s, _, w, cx| s.navigate(Screen::Settings(page), w, cx)))
    }

    fn settings_overview(&self, cx: &mut Context<Self>) -> Div {
        use SettingsPage::*;
        let host = self.server.as_ref().map_or("", Server::host);
        div()
            .child(rows([
                self.category(
                    Playback,
                    PLAY,
                    "Dolby Vision, audio output and subtitles",
                    cx,
                ),
                self.category(Display, "\u{e7f4}", "Monitor and Windows HDR settings", cx),
                self.category(Services, REFRESH, "Seerr, AniList, MDBList and Discord", cx),
                row(
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
            ]))
            .child(heading("Connections"))
            .child(rows([
                row(
                    "add-profile",
                    "\u{e710}",
                    "Add connection",
                    "Connect another Jellyfin account or server",
                    arrow(),
                )
                .on_click(cx.listener(|s, _, w, cx| s.clear_session(w, cx))),
                self.category(Profiles, ACCOUNT, format!("Manage accounts · {host}"), cx),
                row(
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
            ]))
            .child(heading("Downloads"))
            .child(rows([row(
                "settings-downloads",
                "\u{e8b7}",
                "Offline library",
                "View saved media and manage downloads",
                arrow(),
            )
            .on_click(cx.listener(|s, _, w, cx| {
                s.navigate(Screen::Downloads, w, cx)
            }))]))
            .child(heading("Controls"))
            .child(rows([self.category(
                Shortcuts,
                "\u{e765}",
                "Customize keyboard shortcuts",
                cx,
            )]))
            .child(heading("Advanced"))
            .child(rows([
                row(
                    "export-prefs",
                    "\u{e898}",
                    "Export settings",
                    "Save this profile’s preferences to a file",
                    arrow(),
                )
                .on_click(cx.listener(|s, _, w, cx| s.settings_file(true, w, cx))),
                row(
                    "import-prefs",
                    "\u{e896}",
                    "Import settings",
                    "Restore preferences from a RustyFin settings file",
                    arrow(),
                )
                .on_click(cx.listener(|s, _, w, cx| s.settings_file(false, w, cx))),
            ]))
    }

    fn profile_settings(&self, cx: &mut Context<Self>) -> Div {
        let active = self.server.as_ref().and_then(Server::identity);
        let mut accounts = group();
        for (i, account) in self.accounts.iter().enumerate() {
            let selected =
                active.is_some_and(|a| a.user == account.user && a.server == account.server);
            let account = account.clone();
            accounts = accounts.child(
                row(
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
            row(
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
                row(id, "\u{e765}", name, "", label(value, 14., TEXT)).on_click(cx.listener(
                    move |s, _, w, cx| {
                        s.rebind = Some(id);
                        s.focus.focus(w);
                        cx.notify();
                    },
                )),
            );
        }
        list.child(
            row(
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

    pub(super) fn settings(&self, page: SettingsPage, cx: &mut Context<Self>) -> Div {
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .mb(px(20.))
            .when(page != SettingsPage::Overview, |d| {
                d.child(
                    glyph_button("Back to settings", BACK)
                        .on_click(cx.listener(|s, _, w, cx| s.back(w, cx))),
                )
            })
            .child(title(page.title(), 21.));
        let card = |form: Div| div().w_full().rounded_lg().bg(rgb(SURFACE)).child(form);
        let body = match page {
            SettingsPage::Overview => self.settings_overview(cx),
            SettingsPage::Profiles => self.profile_settings(cx),
            SettingsPage::Shortcuts => self.shortcuts_settings(cx),
            SettingsPage::Display => card(self.display_settings(cx).px_4().py_3()),
            SettingsPage::Playback => self.playback_settings(cx),
            SettingsPage::Services => group()
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
                .px(px(16.))
                .pt(px(16.))
                .pb(px(20.))
                .child(header)
                .child(body),
        )
    }
}
