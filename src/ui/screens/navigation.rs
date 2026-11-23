use super::*;
use gpui_component::{
    Selectable,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
};

#[derive(Clone)]
struct Link {
    name: SharedString,
    glyph: &'static str,
    screen: Screen,
    active: bool,
}

impl Link {
    fn new(
        name: impl Into<SharedString>,
        glyph: &'static str,
        screen: Screen,
        active: bool,
    ) -> Self {
        Self {
            name: name.into(),
            glyph,
            screen,
            active,
        }
    }

    fn button(
        &self,
        id: ElementId,
        width: f32,
        show_name: bool,
        cx: &Context<Library>,
    ) -> Stateful<Div> {
        let home = matches!(self.screen, Screen::Discover);
        let target = self.screen.clone();
        let button = div()
            .id(id)
            .focusable()
            .tab_stop(true)
            .flex_none()
            .w(px(width))
            .h(px(34.))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(7.))
            .px(px(6.))
            .rounded_md()
            .cursor_pointer()
            .text_color(rgb(if self.active || home { TEXT } else { 0xb8bcc4 }))
            .when(self.active, |d| d.bg(rgb(0x30372b)).text_color(rgb(LIME)))
            .hover(|d| d.bg(rgb(ACTIVE)).text_color(rgb(TEXT)))
            .focus(|d| d.border_1().border_color(rgb(LIME)))
            .child(
                svg()
                    .path(self.glyph)
                    .size(px(if home { 21. } else { 17. }))
                    .flex_none()
                    .text_color(rgb(if home || self.active { LIME } else { 0xb8bcc4 })),
            )
            .when(show_name, |d| {
                d.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(if home { 16. } else { 13. }))
                        .font_weight(if home {
                            FontWeight::BOLD
                        } else {
                            FontWeight::SEMIBOLD
                        })
                        .child(self.name.clone()),
                )
            });
        tip(
            button,
            if home {
                "Home".to_owned()
            } else {
                self.name.to_string()
            },
        )
        .on_click(cx.listener(move |s, _, w, cx| s.navigate(target.clone(), w, cx)))
    }
}

impl Library {
    pub(super) fn top_navigation(&mut self, w: &Window, cx: &mut Context<Self>) -> Div {
        let width = f32::from(w.viewport_size().width);
        let screen = &self.state.screen;
        let image = match screen {
            Screen::Discover => self.featured.get(self.hero),
            Screen::Detail(item, _) => Some(item.as_ref()),
            _ => None,
        }
        .and_then(|item| {
            self.artwork
                .get(marquee_art(item, w))
                .or_else(|| self.artwork.get(background(item)))
        });
        let tint = self.navigation_tint.sample(image.as_deref(), w);
        let scroll = if matches!(screen, Screen::Library(..)) {
            self.state.wall.scroll.0.borrow().base_handle.clone()
        } else {
            self.state.scroll.clone()
        };
        let distance = (f32::from(w.viewport_size().height) * 0.32).clamp(120., 320.);
        let progress = move |y: Pixels| (-f32::from(y) / distance).clamp(0., 1.);
        let docked = progress(scroll.offset().y);
        let floating = 1. - docked;
        let fill = tint.blend(Rgba {
            a: 0.72 * docked,
            ..rgb(0x121619)
        });
        let margin = (width * 0.016).clamp(7., 16.);
        // Keep the originating library highlighted as users drill into titles and their cast.
        let section = if matches!(screen, Screen::Detail(..) | Screen::Copies { .. }) {
            self.history
                .iter()
                .rev()
                .map(|v| &v.screen)
                .find(|s| !matches!(s, Screen::Detail(..) | Screen::Copies { .. }))
                .unwrap_or(screen)
        } else {
            screen
        };
        let library = match section {
            Screen::Library(tab, q) => Some((*tab, q)),
            _ => None,
        };
        let home = Link::new(
            "RustyFin",
            "nav/home",
            Screen::Discover,
            matches!(section, Screen::Discover),
        );
        let mut tabs = vec![
            Link::new(
                "Favourites",
                "nav/heart",
                Screen::Library(Tab::Browse, CatalogQuery::favourites()),
                library.is_some_and(|(tab, q)| tab == Tab::Browse && q.favorites),
            ),
            Link::new(
                "Collections",
                "nav/libraries",
                Screen::Library(Tab::Collections, Default::default()),
                library.is_some_and(|(tab, _)| tab == Tab::Collections),
            ),
        ];
        for lib in &self.libraries {
            if matches!(
                lib.collection_type.as_deref(),
                Some("boxsets" | "playlists")
            ) {
                continue;
            }
            let kind = lib.catalog_kind().unwrap_or("Folder");
            let glyph = match kind {
                "Movie" | "MusicVideo" => "nav/movies",
                "Series" => "nav/shows",
                "MusicAlbum" => "nav/music",
                _ => "nav/folder",
            };
            tabs.push(Link::new(
                lib.name.clone(),
                glyph,
                Screen::Library(
                    Tab::Browse,
                    CatalogQuery {
                        kind,
                        library: Some(lib.id.clone()),
                        ..Default::default()
                    },
                ),
                library.is_some_and(|(tab, q)| {
                    matches!(tab, Tab::Browse | Tab::Recommended)
                        && !q.favorites
                        && q.library.as_ref() == Some(&lib.id)
                }),
            ));
        }
        let inset = (width * 0.014).clamp(6., 22.);
        let mut bar = div()
            .track_focus(&self.navigation_focus)
            .tab_group()
            .tab_stop(false)
            .absolute()
            .top(px(9. * floating))
            .left(px(margin * floating))
            .right(px(margin * floating))
            .block_mouse_except_scroll()
            .h(px(NAV_HEIGHT))
            .px(px(inset))
            .flex()
            .items_center()
            .gap(px(4.))
            .rounded(px(15. * floating))
            .bg(Rgba {
                a: 0.76 + 0.22 * docked,
                ..fill
            })
            .border_1()
            .border_color(Hsla::from(fill.blend(rgba(0xffffff46))).opacity(0.64))
            .child(home.button("nav-home".into(), 112., true, cx));
        let mut overflow = Vec::new();
        // Reserve the home link, More and the three utility buttons before fitting library names.
        // Budget for the floating width even while docked to keep the shortcuts in place.
        let mut remaining = width - 2. * (inset + margin + 1.) - 112. - 76. - 3. * 36. - 32.;
        for (i, link) in tabs.into_iter().enumerate() {
            // Keep the library names legible on small windows. The two fixed shortcuts have distinct icons.
            let show_name = width >= 900. || i >= 2;
            let tab_width = if show_name {
                (45. + link.name.chars().count() as f32 * 8.).clamp(76., 158.)
            } else {
                36.
            };
            if remaining >= tab_width && overflow.is_empty() {
                bar = bar.child(link.button(("nav-tab", i).into(), tab_width, show_name, cx));
                remaining -= tab_width + 4.;
            } else {
                overflow.push(link);
            }
        }
        overflow.extend([
            Link::new(
                "All libraries",
                "nav/libraries",
                Screen::Library(Tab::Browse, Default::default()),
                library.is_some_and(|(tab, q)| {
                    matches!(tab, Tab::Browse | Tab::Recommended | Tab::Playlists)
                        && q.library.is_none()
                        && !q.favorites
                }),
            ),
            Link::new(
                "Explore",
                "nav/explore",
                Screen::Explore,
                matches!(section, Screen::Explore | Screen::Anime),
            ),
            Link::new(
                "Live TV",
                "nav/live",
                Screen::Live(false),
                matches!(
                    section,
                    Screen::Live(_) | Screen::Guide(_) | Screen::Timers(_)
                ),
            ),
        ]);
        let active = overflow.iter().any(|link| link.active);
        let owner = cx.weak_entity();
        bar = bar
            .child(
                Button::new("more-navigation")
                    .ghost()
                    .label("More ▾")
                    .selected(active)
                    .w(px(76.))
                    .h(px(34.))
                    .dropdown_menu(move |mut menu, w, _| {
                        menu = menu
                            .min_w(px(195.))
                            .max_h((w.viewport_size().height - px(80.)).max(px(100.)))
                            .scrollable(true);
                        for link in &overflow {
                            let target = link.screen.clone();
                            let owner = owner.clone();
                            menu = menu.item(
                                PopupMenuItem::new(link.name.clone())
                                    .checked(link.active)
                                    .on_click(move |_, w, cx| {
                                        let _ = owner
                                            .update(cx, |s, cx| s.navigate(target.clone(), w, cx));
                                    }),
                            );
                        }
                        menu
                    }),
            )
            .child(div().flex_1());
        for link in [
            Link::new(
                "Search",
                "nav/search",
                Screen::Search(self.search.read(cx).value().trim().to_owned()),
                matches!(section, Screen::Search(_)),
            ),
            Link::new(
                "Downloads",
                "nav/downloads",
                Screen::Downloads,
                matches!(section, Screen::Downloads),
            ),
            Link::new(
                "Settings",
                "nav/settings",
                Screen::Settings(SettingsPage::Overview),
                matches!(section, Screen::Settings(_)),
            ),
        ] {
            bar = bar.child(link.button(link.glyph.into(), 36., false, cx));
        }
        // Programmatic keyboard/Back scrolls can settle in prepaint, after render read the handle.
        bar.child(
            canvas(
                move |_, w, _| {
                    let actual = progress(scroll.offset().y);
                    if (actual - docked).abs() > 0.001 {
                        w.request_animation_frame();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size(px(1.)),
        )
    }
}
