use super::*;

const ROWS: &[&[&str]] = &[
    &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"],
    &["q", "w", "e", "r", "t", "y", "u", "i", "o", "p"],
    &["a", "s", "d", "f", "g", "h", "j", "k", "l", "'"],
    &["z", "x", "c", "v", "b", "n", "m", "-", ".", "&"],
    &["Space", "Delete", "Search", "Close"],
];

#[derive(Clone, Copy)]
pub(in crate::ui) struct SearchKeyboard {
    row: usize,
    column: usize,
}

impl SearchKeyboard {
    fn new() -> Self {
        Self { row: 1, column: 0 }
    }

    fn move_selection(&mut self, key: &str) {
        let columns = ROWS[self.row].len();
        match key {
            "left" => self.column = self.column.saturating_sub(1),
            "right" => self.column = (self.column + 1).min(columns - 1),
            "up" | "down" => {
                self.row = if key == "up" {
                    self.row.saturating_sub(1)
                } else {
                    (self.row + 1).min(ROWS.len() - 1)
                };
                // Action keys span the row. Keep the highlight under the same column.
                self.column = (2 * self.column + 1) * ROWS[self.row].len() / (2 * columns);
            }
            _ => {}
        }
    }
}

impl Library {
    pub(in crate::ui) fn submit_search(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.search_keyboard = None;
        self.remember_search();
        self.keyboard = true;
        self.focus.focus(w);
        self.search_key("down", w, cx);
        if self.search_delay.take().is_some() {
            self.load(w, cx);
        }
        cx.notify();
    }

    pub(super) fn open_search_keyboard(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.search_keyboard = Some(SearchKeyboard::new());
        self.search.read(cx).focus_handle(cx).focus(w);
        cx.notify();
    }

    fn press_search_key(&mut self, text: &'static str, w: &mut Window, cx: &mut Context<Self>) {
        self.search.read(cx).focus_handle(cx).focus(w);
        match text {
            "Close" => self.back(w, cx),
            "Search" => self.submit_search(w, cx),
            "Delete" => w.dispatch_action(Box::new(gpui_component::input::Backspace), cx),
            _ => self.search.update(cx, |input, cx| {
                input.replace(if text == "Space" { " " } else { text }, w, cx)
            }),
        }
    }

    pub(in crate::ui) fn controller_search_key(
        &mut self,
        key: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.server.is_none() || (self.player.is_some() && !self.controls.mini) {
            return false;
        }
        if key == "enter" && self.search_link_focus.is_focused(w) {
            if !matches!(self.state.screen, Screen::Search(_)) {
                self.navigate(Screen::Search(String::new()), w, cx);
            }
            self.open_search_keyboard(w, cx);
            return true;
        }
        if !matches!(self.state.screen, Screen::Search(_)) {
            return false;
        }
        if self.search_keyboard.is_none() {
            if key == "enter" && self.search.read(cx).focus_handle(cx).is_focused(w) {
                self.open_search_keyboard(w, cx);
                return true;
            }
            return false;
        }
        match key {
            "preview" => self.press_search_key("Delete", w, cx),
            "space" => self.submit_search(w, cx),
            "tab" | "shift-tab" => {
                self.search_keyboard = None;
                cx.notify();
                return false;
            }
            _ => return self.keypad_navigation(key, w, cx),
        }
        true
    }

    pub(super) fn keypad_navigation(
        &mut self,
        key: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(keys) = self.search_keyboard.as_mut() else {
            return false;
        };
        match key {
            "up" | "down" | "left" | "right" => keys.move_selection(key),
            "enter" => {
                let text = ROWS[keys.row][keys.column];
                self.press_search_key(text, w, cx);
            }
            "escape" => self.back(w, cx),
            _ => return false,
        }
        cx.notify();
        true
    }

    pub(super) fn search_keypad(&self, keys: SearchKeyboard, cx: &Context<Self>) -> Div {
        let mut panel = div()
            .w_full()
            .max_w(px(600.))
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .rounded_lg()
            .bg(rgb(SURFACE));
        for (r, letters) in ROWS.iter().enumerate() {
            let mut row = div().flex().gap_1();
            for (c, &text) in letters.iter().enumerate() {
                let selected = (r, c) == (keys.row, keys.column);
                let key = div()
                    .id(("search-key", r * 10 + c))
                    .min_w_0()
                    .flex_1()
                    .h(px(38.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .bg(rgb(if selected { FOCUS } else { ACTIVE }))
                    .text_color(rgb(if selected { BG } else { TEXT }))
                    .font_weight(FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .hover(|d| d.border_1().border_color(rgb(TEXT)))
                    .child(text.to_uppercase())
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.search_keyboard = Some(SearchKeyboard { row: r, column: c });
                        s.press_search_key(text, w, cx);
                    }));
                row = row.child(key);
            }
            panel = panel.child(row);
        }
        div().px(px(CONTENT_GUTTER)).pb_2().flex_none().child(panel)
    }
}

#[cfg(test)]
mod tests {
    use super::{ROWS, SearchKeyboard};

    #[test]
    fn action_keys_match_the_column_above_and_all_keys_are_reachable() {
        let mut keys = SearchKeyboard::new();
        let mut reachable = std::collections::HashSet::new();
        let mut pending = vec![keys];
        while let Some(here) = pending.pop() {
            if reachable.insert((here.row, here.column)) {
                for direction in ["left", "right", "up", "down"] {
                    let mut next = here;
                    next.move_selection(direction);
                    pending.push(next);
                }
            }
        }
        assert_eq!(
            reachable.len(),
            ROWS.iter().map(|row| row.len()).sum::<usize>()
        );
        keys.row = 3;
        keys.column = 9;
        keys.move_selection("down");
        assert_eq!(ROWS[keys.row][keys.column], "Close");
        keys.move_selection("right");
        assert_eq!(ROWS[keys.row][keys.column], "Close");
    }
}
