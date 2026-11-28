use super::*;
use crate::jellyfin::formats::Formats;

pub(super) fn strip(formats: &Formats, compact: bool) -> Option<Div> {
    let mut values = formats.labels().peekable();
    values.peek()?;
    let mut bar = div()
        .flex()
        .items_center()
        .flex_none()
        .rounded(px(4.))
        .bg(rgba(0x0b0e10e8))
        .px(px(2.))
        .py(px(3.));
    for (index, value) in values.enumerate() {
        if index != 0 {
            bar = bar.child(div().w(px(1.)).h(px(9.)).bg(rgba(0xffffff47)));
        }
        bar = bar.child(
            label(value.to_owned(), if compact { 10. } else { 11. }, 0xf6f7f6)
                .line_height(px(13.))
                .font_weight(if index == 0 {
                    FontWeight::BOLD
                } else {
                    FontWeight::SEMIBOLD
                })
                .px(px(if compact { 3. } else { 4. })),
        );
    }
    Some(bar)
}

impl Library {
    pub(super) fn card_format_strip(
        &self,
        item: &Item,
        card: &ElementId,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let formats = self.formats.for_item(item)?;
        let compact = width < 155.;
        let inset = px(if compact { 4. } else { 7. });
        if !formats.mixed() {
            return Some(
                strip(&formats, compact)?
                    .absolute()
                    .left(inset)
                    .top(inset)
                    .into_any_element(),
            );
        }
        let key = (card.clone(), item.id.clone());
        let open = self.formats.expanded.as_ref() == Some(&key);
        let mut panel = div()
            .id("card-formats")
            .absolute()
            .top(inset)
            .left(inset)
            .max_w(px(width) - inset * 2.)
            .overflow_hidden()
            .rounded(px(5.))
            .px(px(6.))
            .py(px(3.))
            .bg(rgba(0x0b0e10f2))
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        label("Mixed formats", 11., TEXT)
                            .font_weight(FontWeight::SEMIBOLD)
                            .line_height(px(14.)),
                    )
                    .child(icon(if open { "\u{e70e}" } else { "\u{e70d}" }, 9.)),
            );
        if open {
            let mut rows = div().overflow_hidden();
            let mut height = px(0.);
            for (name, values) in formats.details() {
                height += px(13.);
                rows = rows.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .child(label(name, 10., MUTED).line_height(px(13.)))
                        .child(label(values, 10., TEXT).line_height(px(13.)).truncate()),
                );
            }
            panel = panel
                .child(if scrolling::animations_enabled() {
                    rows.with_animation(
                        "format-breakdown",
                        Animation::new(Duration::from_millis(145)).with_easing(ease_out_quint()),
                        move |d, t| d.max_h(height * t).opacity(t),
                    )
                    .into_any_element()
                } else {
                    rows.into_any_element()
                })
                .on_mouse_down_out(cx.listener(|s, _, _, cx| {
                    s.formats.expanded = None;
                    cx.notify();
                }))
                .on_key_down(cx.listener(|s, e: &KeyDownEvent, _, cx| {
                    if e.keystroke.key == "escape" {
                        s.formats.expanded = None;
                        cx.stop_propagation();
                        cx.notify();
                    }
                }));
        }
        let details = formats
            .details()
            .map(|(name, values)| format!("{name} · {values}"))
            .collect::<Vec<_>>()
            .join("\n");
        Some(
            accessibility::button(
                panel,
                format!("Mixed formats for {}. {details}", item.name),
                None,
                false,
            )
            .focusable()
            .tab_stop(true)
            .focus(|d| d.border_1().border_color(rgb(TEXT)))
            .when(!open, |d| {
                d.tooltip(move |w, cx| Tooltip::new(details.clone()).build(w, cx))
            })
            .on_click(cx.listener(move |s, _, _, cx| {
                cx.stop_propagation();
                s.formats.expanded = if open { None } else { Some(key.clone()) };
                cx.notify();
            }))
            .into_any_element(),
        )
    }
}
