use super::*;

const ROW: f32 = 48.;
const LIBRARY_ROW: f32 = 36.;
const GAP: f32 = 4.;
const HOVER: u32 = 0x212227;

fn row(
    id: impl Into<ElementId>,
    name: SharedString,
    glyph: &'static str,
    selected: bool,
    child: bool,
    progress: f32,
) -> Stateful<Div> {
    let glyph_size = if child { 19. } else { 23. };
    div()
        .id(id)
        .group("navigation-row")
        .w_full()
        .h(px(if child { LIBRARY_ROW } else { ROW }))
        .flex_none()
        .flex()
        .items_center()
        .pl(px(16.))
        .pr(px(8.))
        .gap(px(10.))
        .rounded_full()
        .overflow_hidden()
        .cursor_pointer()
        .focusable()
        .tab_stop(true)
        .text_color(rgb(if selected { TEXT } else { MUTED }))
        .when(selected, |d| {
            d.bg(rgb(ACTIVE)).font_weight(FontWeight::SEMIBOLD)
        })
        .hover(move |d| {
            d.bg(rgb(if selected { ACTIVE } else { HOVER }))
                .text_color(rgb(TEXT))
        })
        .focus(|d| d.border_1().border_color(rgb(MUTED)))
        .child(
            div()
                .w(px(if child { 20. } else { 24. }))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path(glyph)
                        .size(px(glyph_size))
                        .text_color(rgb(if selected { TEXT } else { MUTED }))
                        .group_hover("navigation-row", |d| d.text_color(rgb(TEXT))),
                ),
        )
        .when(progress > 0., |d| {
            d.child(
                div()
                    .min_w_0()
                    .truncate()
                    .opacity(progress)
                    .text_size(px(if child { 13. } else { 14. }))
                    .child(name.clone()),
            )
        })
        .when(progress == 0., |d| {
            d.tooltip(move |w, cx| Tooltip::new(name.clone()).build(w, cx))
        })
}

impl Library {
    pub(super) fn rail(&self, w: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let phase = self.rail.phase(Instant::now());
        if phase != f32::from(self.rail.expanded) {
            w.request_animation_frame();
        }
        let progress = phase * phase * (3. - 2. * phase);
        let scope = match &self.state.screen {
            Screen::Library(_, q) => q.library.as_deref(),
            _ => None,
        };
        // Collections and playlists already have dedicated tabs in the library browser.
        let shortcuts: Vec<_> = self
            .libraries
            .iter()
            .filter(|library| {
                !matches!(
                    library.collection_type.as_deref(),
                    Some("boxsets" | "playlists")
                )
            })
            .collect();
        let show_children = progress > 0. && !self.libraries_collapsed && !shortcuts.is_empty();
        let screen = &self.state.screen;
        let destination =
            |name: &'static str, glyph, active, target: Screen, cx: &Context<Self>| {
                row(glyph, name.into(), glyph, active, false, progress)
                    .on_click(cx.listener(move |s, _, w, cx| s.navigate(target.clone(), w, cx)))
            };
        let libraries_active = matches!(screen, Screen::Library(..) | Screen::Copies { .. })
            && !(show_children && shortcuts.iter().any(|l| Some(l.id.as_str()) == scope));
        let libraries = div()
            .flex()
            .items_center()
            .rounded_full()
            .when(libraries_active, |d| d.bg(rgb(ACTIVE)))
            .w_full()
            .flex_none()
            .child(
                destination(
                    "Libraries",
                    "nav/libraries",
                    libraries_active,
                    Screen::Library(Tab::Browse, CatalogQuery::default()),
                    cx,
                )
                .w_auto()
                .flex_1()
                .min_w_0(),
            )
            .when(progress > 0.7 && !shortcuts.is_empty(), |d| {
                let hint = if self.libraries_collapsed {
                    "Expand libraries"
                } else {
                    "Collapse libraries"
                };
                d.child(
                    tip(
                        div()
                            .id("library-disclosure")
                            .mr(px(8.))
                            .size(px(36.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .cursor_pointer()
                            .opacity(progress)
                            .text_color(rgb(MUTED))
                            .hover(|d| d.bg(rgb(ACTIVE)).text_color(rgb(TEXT)))
                            .child(
                                icon(
                                    if self.libraries_collapsed {
                                        "\u{e70d}"
                                    } else {
                                        "\u{e70e}"
                                    },
                                    12.,
                                )
                                .font_weight(FontWeight::BOLD),
                            ),
                        hint,
                    )
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.libraries_collapsed = !s.libraries_collapsed;
                        cx.notify();
                    })),
                )
            });
        let items = div()
            .id("navigation-items")
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(GAP))
            .px(px(8. + progress * 4.))
            .py(px(16.))
            .child(destination(
                "Home",
                "nav/home",
                matches!(screen, Screen::Discover),
                Screen::Discover,
                cx,
            ))
            .child(libraries)
            .when(show_children, |d| {
                let height = (LIBRARY_ROW + GAP) * shortcuts.len() as f32 - GAP;
                d.child(
                    div()
                        .w_full()
                        .h(px(height * progress))
                        .flex_none()
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .gap(px(GAP))
                        .opacity(progress)
                        .children(shortcuts.iter().map(|library| {
                            let kind = library.catalog_kind().unwrap_or("Folder");
                            let glyph = match kind {
                                "Movie" | "MusicVideo" => "nav/movies",
                                "Series" => "nav/shows",
                                "MusicAlbum" => "nav/music",
                                _ => "nav/folder",
                            };
                            let id = library.id.clone();
                            row(
                                SharedString::from(format!("nav/library/{id}")),
                                library.name.clone().into(),
                                glyph,
                                scope == Some(id.as_str()),
                                true,
                                progress,
                            )
                            .on_click(cx.listener(
                                move |s, _, w, cx| {
                                    s.navigate(
                                        Screen::Library(
                                            Tab::Browse,
                                            CatalogQuery {
                                                kind,
                                                library: Some(id.clone()),
                                                ..Default::default()
                                            },
                                        ),
                                        w,
                                        cx,
                                    );
                                },
                            ))
                        })),
                )
            })
            .child(destination(
                "Search",
                "nav/search",
                matches!(screen, Screen::Search(_)),
                Screen::Search(self.search.read(cx).value().trim().to_owned()),
                cx,
            ))
            .child(destination(
                "Downloads",
                "nav/downloads",
                matches!(screen, Screen::Downloads),
                Screen::Downloads,
                cx,
            ))
            .child(destination(
                "Settings",
                "nav/settings",
                matches!(screen, Screen::Settings(_)),
                Screen::Settings(SettingsPage::Overview),
                cx,
            ))
            .child(div().flex_1().min_h(px(GAP)))
            .child(destination(
                "Explore",
                "nav/explore",
                matches!(screen, Screen::Explore | Screen::Anime),
                Screen::Explore,
                cx,
            ))
            .child(destination(
                "Live TV",
                "nav/live",
                matches!(
                    screen,
                    Screen::Live(_) | Screen::Guide(_) | Screen::Timers(_)
                ),
                Screen::Live(false),
                cx,
            ));
        div()
            .id("navigation-rail")
            .absolute()
            .left_0()
            .top_0()
            .w(px(RAIL + (WIDE_RAIL - RAIL) * progress))
            .h_full()
            .rounded_tr(px(32. * progress))
            .bg(rgb(SURFACE))
            .overflow_hidden()
            .occlude()
            .track_focus(&self.rail_focus)
            .on_hover(cx.listener(|s, inside, _, cx| {
                s.rail.set_expanded(*inside);
                cx.notify();
            }))
            .child(items)
    }
}
