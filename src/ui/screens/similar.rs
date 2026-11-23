use super::*;
use std::ops::Range;

impl ViewState {
    fn similar_id(&self) -> Option<&str> {
        let Screen::Detail(item, _) = &self.screen else {
            return None;
        };
        match item.kind.as_str() {
            "Movie" | "Series" => Some(&item.id),
            "Episode" => item.series_id.as_deref(),
            _ => None,
        }
    }

    pub(in crate::ui) fn similar_cards(&self, w: &Window) -> (CatalogGrid, Range<usize>, &[Item]) {
        let width = f32::from(w.viewport_size().width) - 2. * detail::detail_inset(w);
        let layout = CatalogGrid::fit(width, w.scale_factor(), 192., 11);
        let items = self
            .similar
            .as_ref()
            .and_then(|v| v.as_ref().ok())
            .map(Vec::as_slice)
            .unwrap_or_default();
        let start = self
            .similar_start
            .min(items.len().saturating_sub(layout.columns));
        let range = start..(start + layout.columns).min(items.len());
        (layout, range, items)
    }
}

impl Library {
    pub(super) fn load_similar(&mut self, w: &Window, cx: &mut Context<Self>) {
        if self.state.similar.is_some() || self.similar_job.is_some() {
            return;
        }
        let (Some(id), Some(server)) = (self.state.similar_id(), self.server.clone()) else {
            return;
        };
        let id = id.to_owned();
        let worker = cx
            .background_executor()
            .spawn(async move { server.similar_titles(&id) });
        // Navigation drops this task. Each history entry retains its results and shelf position.
        self.similar_job = Some(cx.spawn_in(w, async move |view, cx| {
            let result = worker.await;
            let _ = view.update(cx, |s, cx| {
                s.similar_job = None;
                s.state.similar_focus = (0..result.as_ref().map_or(0, Vec::len))
                    .map(|_| cx.focus_handle())
                    .collect();
                s.state.similar = Some(result.map_err(|e| e.to_string()));
                cx.notify();
            });
        }));
    }

    pub(in crate::ui) fn focus_similar(&mut self, index: usize, w: &mut Window) {
        if let Some(handle) = self.state.similar_focus.get(index) {
            if !self.state.similar_cards(w).1.contains(&index) {
                self.state.similar_start = index;
            }
            handle.focus(w);
        } else {
            self.focus.focus(w);
        }
    }

    pub(super) fn similar_section(&self, w: &Window, cx: &mut Context<Self>) -> Option<Div> {
        let result = self.state.similar.as_ref()?;
        let mut section = div()
            .px(px(detail::detail_inset(w)))
            .py_3()
            .flex()
            .flex_col()
            .gap_2();
        let mut header = div()
            .flex()
            .items_center()
            .justify_between()
            .child(title("More like this", 21.));
        if result.is_err() {
            return Some(
                section.child(header).child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(label("Couldn’t load recommendations.", 14., MUTED))
                        .child(
                            pill("retry-similar", "Retry", Some(REFRESH), false).on_click(
                                cx.listener(|s, _, _, cx| {
                                    s.state.similar = None;
                                    cx.notify();
                                }),
                            ),
                        ),
                ),
            );
        }
        let (layout, range, items) = self.state.similar_cards(w);
        if range.is_empty() {
            return None;
        }
        let mut buttons = div().flex().gap_1();
        for (enabled, to, name, glyph) in [
            (
                range.start > 0,
                range.start.saturating_sub(layout.columns),
                "Previous recommendations",
                PREV,
            ),
            (
                range.end < items.len(),
                range.end.min(items.len().saturating_sub(layout.columns)),
                "Next recommendations",
                NEXT,
            ),
        ] {
            if enabled {
                buttons = buttons.child(glyph_button(name, glyph).on_click(cx.listener(
                    move |s, _, _, cx| {
                        s.state.similar_start = to;
                        cx.notify();
                    },
                )));
            }
        }
        header = header.child(buttons);
        let mut row = div().flex().gap(px(CATALOG_GAP));
        let last = items.len() - 1;
        for i in range {
            row = row.child(
                self.card(
                    &items[i],
                    layout.card_width,
                    CardLayout::Catalog {
                        poster: layout.poster,
                    },
                    self.state.similar_focus[i].is_focused(w),
                    ("similar-title", i).into(),
                    cx,
                )
                .track_focus(&self.state.similar_focus[i])
                .tab_stop(true)
                .rounded_md()
                .on_key_down(cx.listener(move |s, event: &KeyDownEvent, w, cx| {
                    let index = match event.keystroke.key.as_str() {
                        "left" => i.saturating_sub(1),
                        "right" => (i + 1).min(last),
                        _ => return,
                    };
                    s.focus_similar(index, w);
                    cx.stop_propagation();
                    cx.notify();
                })),
            );
        }
        section = section.child(header).child(row);
        Some(section)
    }
}
