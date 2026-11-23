use super::*;
use crate::ui::editor::SubtitleSearch;

impl Library {
    pub(super) fn edit_form(&self, cx: &mut Context<Self>) -> Div {
        let editor = self.state.editor.as_ref().unwrap();
        let mut buttons = div()
            .flex()
            .flex_wrap()
            .gap_2()
            .child(
                pill("save-edit", "Save title and overview", None, true)
                    .on_click(cx.listener(|s, _, w, cx| s.save_editor(w, cx))),
            )
            .child(
                pill("close-edit", "Close editor", None, false).on_click(cx.listener(
                    |s, _, _, cx| {
                        s.state.editor = None;
                        cx.notify();
                    },
                )),
            );
        for (value, text) in [
            ("Primary", "Replace poster…"),
            ("Backdrop", "Replace backdrop…"),
            ("Logo", "Replace logo…"),
        ] {
            buttons = buttons.child(
                pill(text, text, None, false)
                    .on_click(cx.listener(move |s, _, w, cx| s.pick_art(value, w, cx))),
            );
        }
        div()
            .px_5()
            .py_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(title("Edit library metadata", 18.))
            .child(label("Title", 14., MUTED))
            .child(Input::new(&editor.title))
            .child(label("Overview", 14., MUTED))
            .child(Input::new(&editor.overview))
            .child(buttons)
    }
    pub(super) fn subtitle_lookup(&self, item: &Item, cx: &mut Context<Self>) -> Div {
        let mut column = div().flex().flex_col().items_start().gap_2();
        if !matches!(item.kind.as_str(), "Movie" | "Episode" | "Video") {
            return column;
        }
        let Some(search) = &self.state.subtitle_search else {
            return column.child(
                pill("find-subs", "Find subtitles online", None, false).on_click(cx.listener(
                    |s, _, w, cx| {
                        s.state.subtitle_search = Some(SubtitleSearch::new(w, cx));
                        cx.notify();
                    },
                )),
            );
        };
        let item_id = item.id.clone();
        column = column.child(
            div()
                .flex()
                .gap_2()
                .items_center()
                .child(label("Subtitle language", 14., MUTED))
                .child(div().w(px(94.)).child(Input::new(&search.language)))
                .child(
                    pill("search-remote-subs", "Search providers", None, true).on_click(
                        cx.listener(move |s, _, w, cx| s.search_subtitles(item_id.clone(), w, cx)),
                    ),
                ),
        );
        if let Some(rows) = &search.rows {
            if rows.is_empty() {
                column = column.child(label(
                    "No matching subtitles from the server’s providers.",
                    14.,
                    MUTED,
                ));
            }
            for (n, row) in rows.iter().enumerate() {
                let item_id = item.id.clone();
                let subtitle = row.id.clone();
                let name = [
                    row.name.as_deref(),
                    row.provider_name.as_deref(),
                    row.format.as_deref(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                column = column.child(
                    pill(
                        ("download-subtitle", n),
                        format!("Download {name}"),
                        None,
                        false,
                    )
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.install_subtitle(item_id.clone(), subtitle.clone(), w, cx)
                    })),
                );
            }
        }
        column
    }
}
