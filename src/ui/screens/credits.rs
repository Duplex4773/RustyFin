use super::*;
use std::ops::Range;

const CAST_WIDTH: f32 = 132.;
const EXTRA_WIDTH: f32 = 200.;
const GAP: f32 = 12.;

fn page_range(start: usize, total: usize, width: f32, card: f32) -> Range<usize> {
    let count = ((width + GAP) / (card + GAP)).floor().max(1.) as usize;
    let first = start.min(total.saturating_sub(count));
    first..(first + count).min(total)
}

pub(in crate::ui) fn credit_ranges(
    state: &ViewState,
    item: &Item,
    w: &Window,
) -> (Range<usize>, Range<usize>) {
    let width = f32::from(w.viewport_size().width) - 2. * detail::detail_inset(w);
    (
        page_range(
            state.cast_start,
            item.people.as_ref().map_or(0, Vec::len),
            width,
            CAST_WIDTH,
        ),
        page_range(state.extra_start, state.extras.len(), width, EXTRA_WIDTH),
    )
}

fn card(id: ElementId, width: f32) -> Stateful<Div> {
    div()
        .id(id)
        .focusable()
        .tab_stop(true)
        .cursor_pointer()
        .w(px(width))
        .flex_none()
        .rounded_md()
        .flex()
        .flex_col()
        .gap_1()
        .p_1()
        .hover(|s| s.bg(rgb(SURFACE)))
        .focus(|s| s.bg(rgb(ACTIVE)).border_1().border_color(rgb(LIME)))
}

impl Library {
    pub(super) fn open_person(&mut self, id: &str, w: &mut Window, cx: &mut Context<Self>) {
        let Some(server) = self.server.clone() else {
            return;
        };
        let id = id.to_owned();
        self.work(
            w,
            cx,
            move || server.item(&id),
            |s, person, w, cx| {
                s.open_item(person, w, cx);
                Ok(())
            },
        );
    }

    fn credit_heading(
        &self,
        cast: bool,
        range: &Range<usize>,
        total: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let (text, prev, next) = if cast {
            ("Cast and crew", "Previous people", "Next people")
        } else {
            ("Trailers and extras", "Previous extras", "Next extras")
        };
        let mut buttons = div().flex().gap_1();
        for (enabled, to, name, glyph) in [
            (
                range.start > 0,
                range.start.saturating_sub(range.len()),
                prev,
                PREV,
            ),
            (
                range.end < total,
                range.end.min(total.saturating_sub(range.len())),
                next,
                NEXT,
            ),
        ] {
            if enabled {
                buttons = buttons.child(glyph_button(name, glyph).on_click(cx.listener(
                    move |s, _, _, cx| {
                        if cast {
                            s.state.cast_start = to;
                        } else {
                            s.state.extra_start = to;
                        }
                        cx.notify();
                    },
                )));
            }
        }
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(title(text, 21.))
            .child(buttons)
    }

    pub(super) fn credit_cards(&self, item: &Item, w: &Window, cx: &mut Context<Self>) -> Div {
        let (cast, extras) = credit_ranges(&self.state, item, w);
        let mut view = div()
            .px(px(detail::detail_inset(w)))
            .pb_4()
            .flex()
            .flex_col()
            .gap_2();
        let people = item.people.as_deref().unwrap_or_default();
        if !cast.is_empty() {
            let mut cards = div().flex().gap(px(GAP));
            for i in cast.clone() {
                let person = &people[i];
                let id = person.id.clone();
                let mut portrait = div()
                    .size(px(84.))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(rgb(ACTIVE))
                    .flex()
                    .items_center()
                    .justify_center();
                if let Some(image) = self.artwork.get(person.portrait(item.origin.as_deref())) {
                    portrait = portrait.child(
                        img(image)
                            .size_full()
                            .rounded_full()
                            .object_fit(ObjectFit::Cover),
                    );
                } else {
                    let initials: String = person
                        .name
                        .split_whitespace()
                        .take(2)
                        .filter_map(|word| word.chars().next())
                        .collect();
                    portrait = portrait.child(label(initials, 22., MUTED));
                }
                let role = person
                    .role
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .or(person.kind.as_deref());
                cards = cards.child(
                    card(("cast-member", i).into(), CAST_WIDTH)
                        .child(portrait)
                        .child(title(person.name.clone(), 14.).truncate())
                        .when_some(role, |d, r| {
                            d.child(
                                label(r.to_owned(), 13., MUTED)
                                    .line_height(px(17.))
                                    .line_clamp(2),
                            )
                        })
                        .on_click(cx.listener(move |s, _, w, cx| s.open_person(&id, w, cx))),
                );
            }
            view = view
                .child(self.credit_heading(true, &cast, people.len(), cx))
                .child(cards);
        }
        if !extras.is_empty() {
            let mut cards = div().flex().gap(px(GAP));
            for i in extras.clone() {
                let extra = self.state.extras[i].clone();
                let mut still = div()
                    .w_full()
                    .h(px(108.))
                    .rounded_md()
                    .overflow_hidden()
                    .bg(rgb(ACTIVE))
                    .flex()
                    .items_center()
                    .justify_center();
                if let Some(image) = self.artwork.get(extra.landscape()) {
                    still = still.child(
                        img(image)
                            .size_full()
                            .rounded_md()
                            .object_fit(ObjectFit::Cover),
                    );
                } else {
                    still = still.child(icon(PLAY, 24.).text_color(rgb(MUTED)));
                }
                cards = cards.child(
                    card(("extra-still", i).into(), EXTRA_WIDTH)
                        .child(still)
                        .child(title(extra.name.clone(), 14.).truncate())
                        .on_click(
                            cx.listener(move |s, _, w, cx| s.open_item(extra.clone(), w, cx)),
                        ),
                );
            }
            view = view
                .child(self.credit_heading(false, &extras, self.state.extras.len(), cx))
                .child(cards);
        }
        view
    }
}
