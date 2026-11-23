use super::*;
use std::ops::Range;

const INDEX_WIDTH: f32 = 24.;

impl CatalogGrid {
    pub(in crate::ui) fn for_wall(w: &Window, alphabetical: bool) -> Self {
        let width =
            f32::from(w.viewport_size().width) - if alphabetical { INDEX_WIDTH } else { 0. };
        Self::new(width, w.scale_factor())
    }
}

// UniformList also asks for a sample row during measurement. Its decoration receives
// only the rows actually presented, including on the first frame after a scroll or Back.
struct VisibleRows {
    view: WeakEntity<Library>,
    layout: CatalogGrid,
}

impl UniformListDecoration for VisibleRows {
    fn compute(
        &self,
        rows: Range<usize>,
        _: Bounds<Pixels>,
        _: Point<Pixels>,
        _: Pixels,
        _: usize,
        _: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let _ = self.view.update(cx, |s, cx| {
            let wall = &mut s.state.wall;
            let columns = self.layout.columns;
            let visible = rows.start * columns..(rows.end * columns).min(wall.total.unwrap_or(0));
            if wall.visible != visible
                || wall.card_width != self.layout.card_width
                || wall.columns != columns
            {
                wall.needs_prefetch = true;
                wall.visible = visible;
                wall.columns = columns;
                wall.card_width = self.layout.card_width;
                cx.notify();
            }
        });
        div().into_any_element()
    }
}

impl Library {
    pub(super) fn catalog_wall(&self, q: &CatalogQuery, w: &Window, cx: &mut Context<Self>) -> Div {
        let alphabet = self.state.screen.alphabetical();
        let layout = CatalogGrid::for_wall(w, alphabet);
        let wall = &self.state.wall;
        let mut body = div().flex_1().min_h_0().flex().flex_col();
        if let Some(error) = wall.page_error.as_ref().or(wall.index_error.as_ref()) {
            body = body.child(
                div()
                    .px(px(PAD))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(label(error.clone(), 13., MUTED))
                    .child(
                        pill("retry-catalog", "Retry", Some(REFRESH), false).on_click(cx.listener(
                            |s, _, _, cx| {
                                s.state.wall.page_error = None;
                                s.state.wall.index_error = None;
                                cx.notify();
                            },
                        )),
                    ),
            );
        }
        let total = wall.total.unwrap_or(0);
        if total == 0 {
            let message = if wall.total.is_some() {
                "No titles match this view."
            } else {
                "Loading…"
            };
            return body.when(wall.page_error.is_none(), |d| {
                d.child(label(message, 15., MUTED).px(px(PAD)).py_5())
            });
        }
        let square = matches!(q.kind, "MusicArtist" | "MusicAlbum" | "Audio");
        let square = square
            && matches!(
                self.state.screen,
                Screen::Library(Tab::Browse | Tab::Recommended, _)
            );
        let image_height = (layout.card_width * if square { 1. } else { 1.5 }).round();
        let row_height = image_height + 33. + POSTER_LIFT;
        if !wall.visible.is_empty()
            && (wall.card_width != layout.card_width || wall.columns != layout.columns)
        {
            let (anchor, strategy) = if wall.visible.contains(&self.state.selection) {
                (self.state.selection, ScrollStrategy::Center)
            } else {
                (wall.visible.start, ScrollStrategy::Top)
            };
            wall.scroll
                .scroll_to_item_strict(anchor / layout.columns, strategy);
        }
        let view = cx.entity();
        let list = uniform_list(
            "catalog-wall",
            total.div_ceil(layout.columns),
            move |rows: Range<usize>, _, _| {
                rows.map(|row| {
                    // The row dimensions are already known. UniformList measures a
                    // sample twice per frame. Only construct posters for painted rows.
                    let view = view.clone();
                    canvas(
                        move |bounds, w, cx| {
                            let start = row * layout.columns;
                            let mut line = view
                                .update(cx, |s, cx| {
                                    s.catalog_row(
                                        start..(start + layout.columns).min(total),
                                        layout,
                                        image_height,
                                        w,
                                        cx,
                                    )
                                })
                                .into_any_element();
                            line.prepaint_as_root(bounds.origin, bounds.size.into(), w, cx);
                            line
                        },
                        |_, mut line, w, cx| line.paint(w, cx),
                    )
                    .w_full()
                    .h(px(row_height))
                })
                .collect()
            },
        )
        .track_scroll(wall.scroll.clone())
        .with_decoration(VisibleRows {
            view: cx.weak_entity(),
            layout,
        })
        .h_full()
        .flex_1()
        .min_w_0();
        body.child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .child(list)
                .when(alphabet, |d| d.child(self.alphabet_rail(q.descending, cx))),
        )
    }

    fn catalog_row(
        &self,
        indices: Range<usize>,
        layout: CatalogGrid,
        image_height: f32,
        w: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let focused = self.keyboard && self.focus.is_focused(w);
        let mut line = div().size_full().flex().px(px(PAD)).gap(px(CATALOG_GAP));
        for i in indices {
            line = if let Some(item) = self.state.wall.item(i) {
                let id = ("poster", i).into();
                let selected = focused && self.state.selection == i;
                let face =
                    self.catalog_face(item, layout.card_width, layout.poster, selected, &id, cx);
                line.child(
                    card_frame(face, layout.card_width, selected, id).on_click(cx.listener(
                        move |s, _, w, cx| {
                            if let Some(item) = s.state.wall.item(i).cloned() {
                                s.open_item(item, w, cx);
                            }
                        },
                    )),
                )
            } else {
                line.child(
                    div()
                        .flex_none()
                        .w(px(layout.card_width))
                        .mt(px(POSTER_LIFT))
                        .h(px(image_height))
                        .rounded(px(8.))
                        .bg(rgb(SURFACE)),
                )
            };
        }
        line
    }

    fn alphabet_rail(&self, descending: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let wall = &self.state.wall;
        let at = if self.keyboard && wall.visible.contains(&self.state.selection) {
            self.state.selection
        } else {
            wall.visible.start
        };
        let active = wall
            .item(at)
            .map(|i| crate::jellyfin::catalog_initial(i.sort_name.as_deref(), &i.name));
        let mut letters: Vec<_> = std::iter::once('#').chain('A'..='Z').collect();
        if descending {
            letters.reverse();
        }
        div()
            .id("catalog-alphabet")
            .w(px(INDEX_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .track_scroll(&wall.rail_scroll)
            .overflow_y_scroll()
            .bg(rgb(SURFACE))
            .rounded_lg()
            .children(letters.into_iter().map(|letter| {
                let available = wall.letters.contains_key(&letter);
                let focused = wall.rail_focus == Some(letter);
                div()
                    .id(("catalog-initial", letter as usize))
                    .flex_1()
                    .min_h(px(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(if focused {
                        BG
                    } else if active == Some(letter) {
                        LIME
                    } else if available {
                        TEXT
                    } else {
                        0x686b71
                    }))
                    .when(focused, |d| d.bg(rgb(LIME)))
                    .child(letter.to_string())
                    .when(available, |d| {
                        d.cursor_pointer()
                            .hover(|d| d.bg(rgb(ACTIVE)).text_color(rgb(TEXT)))
                            .on_click(
                                cx.listener(move |s, _, w, cx| s.jump_to_letter(letter, w, cx)),
                            )
                    })
            }))
    }
}
