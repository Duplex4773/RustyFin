use super::*;
use crate::jellyfin::ExtraKind;
use std::ops::Range;

const CAST_WIDTH: f32 = 132.;
const EXTRA_WIDTH: f32 = 200.;
const GAP: f32 = 12.;

pub(in crate::ui) type ExtraFeed = Option<Result<Vec<Item>, String>>;

impl ViewState {
    pub(in crate::ui) fn extra_items(&self) -> impl Iterator<Item = &Item> {
        self.extras
            .iter()
            .flatten()
            .filter_map(|feed| feed.as_ref().ok())
            .flatten()
    }
}

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
        page_range(
            state.extra_start,
            state.extra_items().count(),
            width,
            EXTRA_WIDTH,
        ),
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
    pub(super) fn load_extras(&mut self, w: &Window, cx: &mut Context<Self>) {
        let Screen::Detail(item, _) = &self.state.screen else {
            return;
        };
        if self.busy || ExtraKind::ALL.iter().all(|kind| kind.count(item) == 0) {
            return;
        }
        let Some(server) = self.detail_server() else {
            return;
        };
        for (index, kind) in ExtraKind::ALL.into_iter().enumerate() {
            if kind.count(item) == 0
                || self.state.extras[index].is_some()
                || self.extras_jobs[index].is_some()
            {
                continue;
            }
            let item = item.clone();
            let server = server.clone();
            let worker = cx
                .background_executor()
                .spawn(async move { server.extras(&item, kind) });
            // Navigation cancels the UI callback. Completed feeds are cached with the detail view.
            self.extras_jobs[index] = Some(cx.spawn_in(w, async move |view, cx| {
                let result = worker.await;
                let _ = view.update(cx, |s, cx| {
                    s.extras_jobs[index] = None;
                    s.state.extras[index] = Some(result.map_err(|error| error.to_string()));
                    cx.notify();
                });
            }));
        }
    }

    pub(super) fn extras_notice(&self, cx: &mut Context<Self>) -> Option<Div> {
        let pending = self.extras_jobs.iter().any(Option::is_some);
        let failed: Vec<_> = ExtraKind::ALL
            .into_iter()
            .zip(&self.state.extras)
            .filter_map(|(kind, result)| {
                result
                    .as_ref()
                    .is_some_and(Result::is_err)
                    .then_some(kind.label())
            })
            .collect();
        let text = if !failed.is_empty() {
            format!("Couldn’t load {}.", failed.join(" and "))
        } else if pending {
            "Loading trailers and extras…".into()
        } else {
            return None;
        };
        let status = label(text.clone(), 14., MUTED)
            .relative()
            .children(accessibility::status(&text));
        Some(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(status)
                .when(!failed.is_empty(), |d| {
                    d.child(
                        pill(
                            "retry-extras",
                            "Retry trailers and extras",
                            Some(REFRESH),
                            false,
                        )
                        .on_click(cx.listener(|s, _, _, cx| {
                            for feed in &mut s.state.extras {
                                if feed.as_ref().is_some_and(Result::is_err) {
                                    *feed = None;
                                }
                            }
                            cx.notify();
                        })),
                    )
                }),
        )
    }

    pub(super) fn open_person(&mut self, id: &str, w: &mut Window, cx: &mut Context<Self>) {
        let Some(server) = self.detail_server() else {
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
            .child(section_heading(text, 21.))
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
        let notice = self.extras_notice(cx);
        if !extras.is_empty() || notice.is_some() {
            let mut cards = div().flex().gap(px(GAP));
            for (i, extra) in self
                .state
                .extra_items()
                .enumerate()
                .skip(extras.start)
                .take(extras.len())
            {
                let extra = extra.clone();
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
                .child(self.credit_heading(false, &extras, self.state.extra_items().count(), cx))
                .children(notice)
                .when(!extras.is_empty(), |d| d.child(cards));
        }
        view
    }
}
