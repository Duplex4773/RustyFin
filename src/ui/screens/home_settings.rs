use super::{
    settings::{group, section_card, section_stack, settings_columns, toggle},
    *,
};

impl Library {
    pub(super) fn home_pin_libraries(&self) -> impl Iterator<Item = &Item> {
        self.libraries.iter().filter(|item| {
            !matches!(
                item.collection_type.as_deref(),
                Some("boxsets" | "playlists")
            )
        })
    }

    pub(super) fn home_settings(&self, wide: bool, cx: &mut Context<Self>) -> Div {
        let layout = &self.prefs.home.layout;
        let order = layout.shelf_order();
        let mut shelves = group();
        for (index, kind) in order.iter().copied().enumerate() {
            let visible = !layout.hidden.contains(&kind);
            let mut controls = div().flex_none().flex().items_center().gap_1().pr_2();
            for (direction, name, available) in [
                (-1, "Move up", index > 0),
                (1, "Move down", index + 1 < order.len()),
            ] {
                if available {
                    let action = SharedString::from(format!("{name} {}", kind.title()));
                    controls = controls.child(
                        self.settings_button(
                            tip(
                                div()
                                    .id(action.clone())
                                    .size(px(34.))
                                    .rounded(px(8.))
                                    .border_1()
                                    .border_color(rgba(REST_EDGE))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .hover(|d| {
                                        d.bg(rgba(HOVER_FILL)).border_color(rgba(HOVER_EDGE))
                                    })
                                    .active(|d| d.bg(rgba(PRESS_FILL)))
                                    .child(icon(
                                        if direction < 0 {
                                            "\u{e70e}"
                                        } else {
                                            "\u{e70d}"
                                        },
                                        13.,
                                    )),
                                action,
                            ),
                            cx,
                        )
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.prefs.home.layout.move_shelf(kind, direction);
                            s.save_home(cx);
                            s.focus_page_button(kind.title(), w);
                        })),
                    );
                } else {
                    // Reserve the slot so switches and arrows align across all shelves.
                    controls = controls.child(div().flex_none().size(px(34.)));
                }
            }
            shelves = shelves.child(
                div()
                    .flex()
                    .items_center()
                    .child(
                        self.settings_row(
                            cx,
                            kind.title(),
                            "\u{e8fd}",
                            kind.title(),
                            if visible {
                                "Shown on Home"
                            } else {
                                "Hidden on Home"
                            },
                            toggle(visible),
                        )
                        .flex_1()
                        .min_w_0()
                        .w_auto()
                        .on_click(cx.listener(move |s, _, _, cx| {
                            let hidden = &mut s.prefs.home.layout.hidden;
                            if hidden.contains(&kind) {
                                hidden.retain(|&value| value != kind);
                            } else {
                                hidden.push(kind);
                            }
                            s.save_home(cx);
                        })),
                    )
                    .child(controls),
            );
        }

        let mut pins = group().child(
            self.settings_row(
                cx,
                "pin-favourites",
                "\u{eb51}",
                "Favourites",
                "",
                toggle(layout.pin_favourites),
            )
            .on_click(cx.listener(|s, _, _, cx| {
                s.prefs.home.layout.pin_favourites = !s.prefs.home.layout.pin_favourites;
                s.save_home(cx);
            })),
        );
        for item in self.home_pin_libraries() {
            let id = item.id.clone();
            pins = pins.child(
                self.settings_row(
                    cx,
                    SharedString::from(format!("pin-library-{id}")),
                    "\u{e8b7}",
                    item.name.clone(),
                    "",
                    toggle(layout.pinned_libraries.contains(&id)),
                )
                .on_click(cx.listener(move |s, _, _, cx| {
                    let pinned = &mut s.prefs.home.layout.pinned_libraries;
                    if pinned.contains(&id) {
                        pinned.retain(|entry| entry != &id);
                    } else {
                        pinned.push(id.clone());
                    }
                    s.save_home(cx);
                })),
            );
        }

        let mut shortcuts = section_stack().child(section_card("Pinned shortcuts", pins));
        let reset = group().child(
            self.settings_row(
                cx,
                "reset-home",
                REFRESH,
                "Restore default layout",
                "",
                div(),
            )
            .on_click(cx.listener(|s, _, _, cx| {
                s.prefs.home.layout = Default::default();
                s.save_home(cx);
            })),
        );
        if !self.prefs.home.dismissed.is_empty() {
            let mut dismissed = group();
            for (index, title) in self.prefs.home.dismissed.iter().enumerate() {
                let title = title.clone();
                dismissed = dismissed.child(
                    self.settings_row(
                        cx,
                        ("restore-home-title", index),
                        REFRESH,
                        title.name.clone(),
                        "",
                        label("Restore", 13., TEXT),
                    )
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.restore_continue(&title, cx);
                        s.focus_page_button("reset-home", w);
                    })),
                );
            }
            shortcuts = shortcuts.child(section_card("Removed from Continue Watching", dismissed));
        }
        div()
            .w_full()
            .child(label("Changes apply to this profile.", 14., META).mb_4())
            .child(settings_columns(
                wide,
                section_stack()
                    .child(section_card("Shelves", shelves))
                    .child(reset),
                shortcuts,
            ))
    }
}
