use super::*;
use gpui_component::menu::{PopupMenu, PopupMenuItem};

const VIEWS: [(Tab, &str); 4] = [
    (Tab::Recommended, "Recommended"),
    (Tab::Browse, "Browse"),
    (Tab::Collections, "Collections"),
    (Tab::Playlists, "Playlists"),
];
const SORTS: [((&str, bool), &str); 4] = [
    (("SortName", false), "Title A–Z"),
    (("SortName", true), "Title Z–A"),
    (("DateCreated", true), "Recently added"),
    (("CommunityRating", true), "Highest rated"),
];
const KINDS: [(&str, &str); 10] = [
    ("Movie", "Movies"),
    ("Series", "TV shows"),
    ("Episode", "Episodes"),
    ("MusicAlbum", "Albums"),
    ("MusicArtist", "Artists"),
    ("Audio", "Songs"),
    ("MusicVideo", "Music videos"),
    ("Folder", "Folders"),
    ("BoxSet", "Collections"),
    ("Playlist", "Playlists"),
];

fn kind_name(kind: &str) -> &'static str {
    KINDS
        .iter()
        .find(|(k, _)| *k == kind)
        .map_or("All media", |(_, name)| *name)
}

fn tool(
    id: &'static str,
    text: impl Into<SharedString>,
    glyph: Option<&'static str>,
    active: bool,
) -> Stateful<Div> {
    pill(id, text, glyph, active)
        .h(px(30.))
        .px(px(10.))
        .gap(px(5.))
}

impl Library {
    fn browse_choice<T: Clone + PartialEq + 'static, N: Clone + Into<SharedString> + 'static>(
        &self,
        id: &'static str,
        trigger: Stateful<Div>,
        selected: T,
        options: impl IntoIterator<Item = (T, N)> + Clone + 'static,
        choose: fn(&mut Self, T, &mut Window, &mut Context<Self>),
        cx: &Context<Self>,
    ) -> Div {
        self.popup_trigger(
            id.into(),
            trigger.child(icon("\u{e70d}", 9.)),
            move |mut menu, owner| {
                for (value, label) in options.clone() {
                    let owner = owner.clone();
                    let checked = value == selected;
                    menu = menu.item(PopupMenuItem::new(label).checked(checked).on_click(
                        move |_, w, cx| {
                            cx.stop_propagation();
                            if !checked {
                                let _ = owner.update(cx, |s, cx| choose(s, value.clone(), w, cx));
                            }
                        },
                    ));
                }
                menu
            },
            cx,
        )
    }

    pub(super) fn popup_trigger(
        &self,
        id: ElementId,
        trigger: Stateful<Div>,
        build: impl Fn(PopupMenu, WeakEntity<Library>) -> PopupMenu + Clone + 'static,
        cx: &Context<Self>,
    ) -> Div {
        let open = self.popup_menu.as_ref().filter(|(key, _)| *key == id);
        let was_open = open.is_some();
        let toggle = move |s: &mut Self, w: &mut Window, cx: &mut Context<Self>| {
            if was_open {
                s.popup_menu = None;
                cx.notify();
            } else {
                s.show_popup(id.clone(), &build, w, cx);
            }
        };
        // The menu confirms on key-down and returns focus to this button.
        // Handling activation on key-down also prevents key-up from reopening it.
        let key_toggle = toggle.clone();
        let button = trigger
            .on_key_down(cx.listener(move |s, event: &KeyDownEvent, w, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space")
                    && !event.keystroke.modifiers.modified()
                {
                    if !event.is_held {
                        key_toggle(s, w, cx);
                    }
                    w.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .on_click(cx.listener(move |s, event: &gpui::ClickEvent, w, cx| {
                cx.stop_propagation();
                if matches!(event, gpui::ClickEvent::Mouse(_)) {
                    toggle(s, w, cx);
                }
            }));
        div()
            .relative()
            .flex_none()
            .child(button)
            .when_some(open, |d, (_, menu)| {
                d.child(
                    div().absolute().left_0().top(px(34.)).child(
                        deferred(
                            anchored()
                                .snap_to_window_with_margin(px(8.))
                                .child(menu.clone()),
                        )
                        .with_priority(2),
                    ),
                )
            })
    }

    pub(super) fn show_popup(
        &mut self,
        id: ElementId,
        build: impl FnOnce(PopupMenu, WeakEntity<Library>) -> PopupMenu,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = cx.weak_entity();
        let launcher = w.focused(cx).unwrap_or_else(|| self.focus.clone());
        let menu = PopupMenu::build(w, cx, |menu, w, _| {
            build(
                menu.action_context(launcher)
                    .min_w(px(168.))
                    .max_h((w.viewport_size().height - px(170.)).max(px(100.)))
                    .scrollable(true),
                owner,
            )
        });
        cx.subscribe(&menu, |s, source, _: &DismissEvent, cx| {
            if s.popup_menu
                .as_ref()
                .is_some_and(|(_, current)| current == &source)
            {
                s.popup_menu = None;
                cx.notify();
            }
        })
        .detach();
        menu.focus_handle(cx).focus(w);
        self.popup_menu = Some((id, menu));
        cx.notify();
    }

    pub(super) fn browse(
        &self,
        tab: Tab,
        q: &CatalogQuery,
        w: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let scope = self
            .libraries
            .iter()
            .find(|lib| Some(&lib.id) == q.library.as_ref());
        let view_name = VIEWS.iter().find(|(t, _)| *t == tab).unwrap().1;
        let heading = scope
            .map(|lib| lib.name.as_str())
            .unwrap_or_else(|| match tab {
                Tab::Browse if q.favorites => "Favourites",
                Tab::Browse => kind_name(q.kind),
                _ => "Library",
            });
        let compact = w.viewport_size().width < px(800.);
        let title_block = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap_2()
            .child(
                title(heading.to_owned(), if compact { 18. } else { 22. })
                    .min_w_0()
                    .truncate(),
            )
            .when_some(self.state.wall.total, |d, count| {
                d.child(
                    label(
                        format!("{count} {}", if count == 1 { "title" } else { "titles" }),
                        12.,
                        MUTED,
                    )
                    .flex_none(),
                )
            });
        let mut row = div()
            .flex()
            .items_center()
            .gap_1()
            .child(title_block)
            .child(self.browse_choice(
                "view",
                tool("browse-view", view_name, None, false),
                tab,
                VIEWS,
                |s, next, w, cx| {
                    if let Screen::Library(_, q) = &s.state.screen {
                        s.navigate(Screen::Library(next, q.clone()), w, cx);
                    }
                },
                cx,
            ))
            .child(
                tool("filters", "Filters", Some(FILTER), self.filters_open).on_click(cx.listener(
                    |s, _, _, cx| {
                        s.filters_open = !s.filters_open;
                        cx.notify();
                    },
                )),
            );
        if tab != Tab::Recommended {
            row = row.child(self.browse_choice(
                "sort",
                tool("browse-sort", "Sort", Some(SORT), false),
                (q.sort, q.descending),
                SORTS,
                |s, sort, w, cx| s.change_filter(|q| (q.sort, q.descending) = sort, w, cx),
                cx,
            ));
        }
        row = row.child(self.browse_choice(
            "size",
            tool("poster-size", "Poster size", None, false),
            self.prefs.poster_size,
            [
                (PosterSize::Small, "Small"),
                (PosterSize::Medium, "Medium"),
                (PosterSize::Large, "Large"),
            ],
            |s, size, _, cx| {
                s.state.wheel_motion = None;
                s.state.wall.needs_prefetch = true;
                s.prefs.poster_size = size;
                s.save_preferences(cx);
            },
            cx,
        ));
        if tab == Tab::Browse {
            let query = q.clone();
            row = row.child(
                glyph_button("Shuffle library", "\u{e8b1}")
                    .size(px(30.))
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.play_queue(Some(query.clone()), None, true, false, w, cx)
                    })),
            );
        }
        row = row.child(
            glyph_button("Refresh", REFRESH)
                .size(px(30.))
                .on_click(cx.listener(|s, _, w, cx| s.refresh(w, cx))),
        );
        let mut header = div()
            .px(px(PAD))
            .py_2()
            .flex()
            .flex_col()
            .gap_2()
            .flex_none()
            .child(row);
        if self.filters_open {
            header = header.child(self.browse_filters(tab, q, cx));
        } else {
            let mut chips = Vec::new();
            if q.unwatched {
                chips.push(
                    tool("clear-unwatched", "Unwatched  ×", None, false).on_click(
                        cx.listener(|s, _, w, cx| s.change_filter(|q| q.unwatched = false, w, cx)),
                    ),
                );
            }
            if q.favorites {
                chips.push(
                    tool("clear-favourites", "Favourites  ×", None, false).on_click(
                        cx.listener(|s, _, w, cx| s.change_filter(|q| q.favorites = false, w, cx)),
                    ),
                );
            }
            if let Some(letter) = q.initial {
                chips.push(
                    tool(
                        "clear-initial",
                        format!("Starts with {letter}  ×"),
                        None,
                        false,
                    )
                    .on_click(
                        cx.listener(|s, _, w, cx| s.change_filter(|q| q.initial = None, w, cx)),
                    ),
                );
            }
            if matches!(tab, Tab::Browse | Tab::Recommended)
                && let Some(default_kind) = scope.map(|lib| lib.catalog_kind().unwrap_or("Folder"))
                && default_kind != q.kind
            {
                chips.push(
                    tool(
                        "clear-kind",
                        format!("{}  ×", kind_name(q.kind)),
                        None,
                        false,
                    )
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.change_filter(|q| q.kind = default_kind, w, cx)
                    })),
                );
            }
            if !chips.is_empty() {
                header = header.child(div().flex().gap_1().flex_wrap().children(chips));
            }
        }
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(self.catalog_wall(q, w, cx))
    }

    fn browse_filters(&self, tab: Tab, q: &CatalogQuery, cx: &Context<Self>) -> Div {
        let libraries: Vec<_> = std::iter::once((None, "All libraries".to_owned()))
            .chain(
                self.libraries
                    .iter()
                    .map(|lib| (Some(lib.id.clone()), lib.name.clone())),
            )
            .collect();
        let scope = libraries
            .iter()
            .find(|(id, _)| id == &q.library)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "Library".into());
        let mut filters =
            div()
                .flex()
                .items_center()
                .flex_wrap()
                .gap_2()
                .child(self.browse_choice(
                    "scope",
                    tool("filter-library", format!("Library · {scope}"), None, false),
                    q.library.clone(),
                    libraries,
                    |s, id, w, cx| {
                        let kind = s
                            .libraries
                            .iter()
                            .find(|l| Some(&l.id) == id.as_ref())
                            .map(|l| l.catalog_kind().unwrap_or("Folder"));
                        s.change_filter(
                            |q| {
                                q.library = id;
                                if let Some(kind) = kind {
                                    q.kind = kind;
                                }
                            },
                            w,
                            cx,
                        );
                    },
                    cx,
                ));
        if matches!(tab, Tab::Browse | Tab::Recommended) {
            let all = CatalogQuery::favourites().kind;
            let kinds: Vec<_> = std::iter::once((all, "All media")).chain(KINDS).collect();
            filters = filters.child(self.browse_choice(
                "kind",
                tool(
                    "filter-kind",
                    format!("Type · {}", kind_name(q.kind)),
                    None,
                    false,
                ),
                q.kind,
                kinds,
                |s, kind, w, cx| s.change_filter(|q| q.kind = kind, w, cx),
                cx,
            ));
        }
        let letters: Vec<_> = std::iter::once((None, "Any initial".to_owned()))
            .chain(('A'..='Z').map(|c| (Some(c), c.to_string())))
            .collect();
        filters
            .child(
                self.browse_choice(
                    "initial",
                    tool(
                        "filter-initial",
                        q.initial
                            .map_or_else(|| "Starts with".into(), |c| format!("Starts with {c}")),
                        None,
                        false,
                    ),
                    q.initial,
                    letters,
                    |s, initial, w, cx| s.change_filter(|q| q.initial = initial, w, cx),
                    cx,
                ),
            )
            .child(tool("unwatched", "Unwatched", None, q.unwatched).on_click(
                cx.listener(|s, _, w, cx| s.change_filter(|q| q.unwatched = !q.unwatched, w, cx)),
            ))
            .child(tool("favorites", "Favourites", None, q.favorites).on_click(
                cx.listener(|s, _, w, cx| s.change_filter(|q| q.favorites = !q.favorites, w, cx)),
            ))
    }
}
