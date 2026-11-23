use super::*;
use crate::jellyfin::{LIBRARY_PAGE_SIZE, catalog_initial};
use std::{collections::BTreeMap, ops::Range};

pub(super) struct CatalogWall {
    items: BTreeMap<usize, Item>,
    pub total: Option<usize>,
    pub scroll: UniformListScrollHandle,
    pub rail_scroll: ScrollHandle,
    pub visible: Range<usize>,
    pub needs_prefetch: bool,
    pub columns: usize,
    pub card_width: f32,
    pub letters: BTreeMap<char, usize>,
    scanned: usize,
    page_job: Option<Task<()>>,
    index_job: Option<Task<()>>,
    pub page_error: Option<String>,
    pub index_error: Option<String>,
    pub rail_focus: Option<char>,
}

impl Default for CatalogWall {
    fn default() -> Self {
        Self {
            items: BTreeMap::new(),
            total: None,
            scroll: UniformListScrollHandle::new(),
            rail_scroll: ScrollHandle::new(),
            visible: 0..0,
            needs_prefetch: true,
            columns: 1,
            card_width: 0.,
            letters: BTreeMap::new(),
            scanned: 0,
            page_job: None,
            index_job: None,
            page_error: None,
            index_error: None,
            rail_focus: None,
        }
    }
}

impl CatalogWall {
    fn reveal_letter(&self, c: char, descending: bool) {
        let index = if c == '#' {
            0
        } else {
            c as usize - 'A' as usize + 1
        };
        self.rail_scroll
            .scroll_to_item(if descending { 26 - index } else { index });
    }

    pub fn suspend(&mut self) {
        self.needs_prefetch = true;
        self.page_job = None;
        self.index_job = None;
        self.rail_focus = None;
    }

    // Preserve the viewport while refreshing membership or metadata.
    pub fn invalidate(&mut self) {
        self.suspend();
        self.items.clear();
        self.letters.clear();
        self.scanned = 0;
        self.page_error = None;
        self.index_error = None;
        if self.total == Some(0) {
            self.total = None;
        }
    }

    pub fn item(&self, index: usize) -> Option<&Item> {
        self.items.get(&index)
    }

    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.items
            .iter()
            .find_map(|(&i, item)| (item.id == id).then_some(i))
    }

    pub fn near_items(&self) -> impl Iterator<Item = &Item> {
        self.items.range(self.near_range()).map(|(_, item)| item)
    }

    fn near_range(&self) -> Range<usize> {
        let total = self.total.unwrap_or(0);
        self.visible
            .start
            .saturating_sub(self.columns * 2)
            .min(total)..(self.visible.end + self.columns * 2).min(total)
    }

    fn needed_offset(&self) -> Option<usize> {
        let Some(total) = self.total else {
            return Some(0);
        };
        if self.items.len() == total {
            return None;
        }
        // Prioritize what is on screen after a jump, then prefetch adjacent rows.
        let first_missing = self
            .visible
            .clone()
            .chain(self.near_range())
            .find(|i| *i < total && !self.items.contains_key(i))?;
        let start = first_missing / LIBRARY_PAGE_SIZE * LIBRARY_PAGE_SIZE;
        (start..=first_missing).find(|i| !self.items.contains_key(i))
    }

    fn index_letters(&mut self, letters: impl IntoIterator<Item = char>) {
        for letter in letters {
            self.letters.entry(letter).or_insert(self.scanned);
            self.scanned += 1;
        }
    }

    fn receive(&mut self, offset: usize, page: Page, alphabetical: bool) {
        self.needs_prefetch = true;
        let total = page.total_record_count;
        if self.total.is_some_and(|old| old != total) {
            self.invalidate();
        }
        self.total = Some(total);
        if page.items.is_empty() && offset < total {
            self.page_error =
                Some("Jellyfin returned an empty page before the end of this library.".into());
            return;
        }
        if alphabetical && offset == self.scanned {
            self.index_letters(
                page.items
                    .iter()
                    .map(|item| catalog_initial(item.sort_name.as_deref(), &item.name)),
            );
        }
        self.items.extend((offset..total).zip(page.items));
        if self.items.len() > LIBRARY_PAGE_SIZE * 6 {
            let near = self.near_range();
            let keep =
                near.start.saturating_sub(2 * LIBRARY_PAGE_SIZE)..near.end + 2 * LIBRARY_PAGE_SIZE;
            self.items.retain(|index, _| keep.contains(index));
        }
    }

    pub fn update_items(&mut self, q: &CatalogQuery, update: &impl Fn(&mut Item)) {
        self.needs_prefetch = true;
        let mut changed_membership = false;
        for item in self.items.values_mut() {
            let included = |i: &Item| {
                let data = i.user_data.as_ref();
                (!q.favorites || data.and_then(|d| d.is_favorite) == Some(true))
                    && (!q.unwatched || data.and_then(|d| d.played) != Some(true))
            };
            let before = included(item);
            update(item);
            changed_membership |= before != included(item);
        }
        if changed_membership {
            self.invalidate();
        }
    }
}

impl Screen {
    pub(super) fn alphabetical(&self) -> bool {
        matches!(self, Self::Library(tab, q) if *tab != Tab::Recommended && q.sort == "SortName" && q.initial.is_none() && (*tab != Tab::Browse || q.kind != "Folder"))
    }
}

impl Library {
    pub(super) fn load_wall(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Screen::Library(tab, mut q) = self.state.screen.clone() else {
            return;
        };
        let Some(server) = self.server.clone() else {
            return;
        };
        match tab {
            Tab::Recommended => {
                q.sort = "CommunityRating";
                q.descending = true;
            }
            Tab::Collections => q.kind = "BoxSet",
            Tab::Playlists => q.kind = "Playlist",
            Tab::Browse => {}
        }
        let alphabetical = self.state.screen.alphabetical();
        let wall = &mut self.state.wall;
        if wall.page_job.is_none()
            && wall.page_error.is_none()
            && let Some(offset) = wall.needed_offset()
        {
            let query = q.clone();
            let source = server.clone();
            let worker = cx
                .background_executor()
                .spawn(async move { source.catalog(&query, None, offset, LIBRARY_PAGE_SIZE) });
            wall.page_job = Some(cx.spawn_in(w, async move |view, cx| {
                let result = worker.await;
                let _ = view.update(cx, |s, cx| {
                    let wall = &mut s.state.wall;
                    wall.page_job = None;
                    match result {
                        Ok(page) => {
                            wall.receive(offset, page, alphabetical);
                            s.state.selection = s
                                .state
                                .selection
                                .min(wall.total.unwrap_or(0).saturating_sub(1));
                        }
                        Err(e) => wall.page_error = Some(format!("Could not load titles. {e:#}")),
                    }
                    cx.notify();
                });
            }));
        }
        if alphabetical
            && wall.index_job.is_none()
            && wall.index_error.is_none()
            && wall.scanned < wall.total.unwrap_or(0)
        {
            let offset = wall.scanned;
            let worker = cx
                .background_executor()
                .spawn(async move { server.catalog_letters(&q, offset) });
            wall.index_job = Some(cx.spawn_in(w, async move |view, cx| {
                let result = worker.await;
                let _ = view.update(cx, |s, cx| {
                    let wall = &mut s.state.wall;
                    wall.index_job = None;
                    if wall.scanned == offset {
                        match result {
                            Ok((_, total)) if Some(total) != wall.total => {
                                wall.invalidate();
                                wall.total = Some(total);
                            }
                            Ok((letters, _)) if !letters.is_empty() => wall.index_letters(letters),
                            Ok(_) => {
                                wall.index_error =
                                    Some("Jellyfin returned an incomplete title index.".into())
                            }
                            Err(e) => {
                                wall.index_error =
                                    Some(format!("Could not load the alphabet index. {e:#}"))
                            }
                        }
                    }
                    cx.notify();
                });
            }));
        }
    }

    pub(super) fn jump_to_letter(&mut self, letter: char, w: &mut Window, cx: &mut Context<Self>) {
        let wall = &mut self.state.wall;
        if let Some(&index) = wall.letters.get(&letter) {
            self.state.selection = index;
            wall.scroll
                .scroll_to_item_strict(index / wall.columns, ScrollStrategy::Top);
            wall.rail_focus = None;
            self.keyboard = true;
            self.focus.focus(w);
            cx.notify();
        }
    }

    pub(super) fn catalog_key(
        &mut self,
        key: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let alphabetical = self.state.screen.alphabetical();
        let wall = &mut self.state.wall;
        let descending = matches!(&self.state.screen, Screen::Library(_,q) if q.descending);
        if let Some(letter) = wall.rail_focus {
            match key {
                "enter" => self.jump_to_letter(letter, w, cx),
                "left" | "escape" | "backspace" => wall.rail_focus = None,
                "up" | "down" => {
                    let next = if (key == "down") != descending {
                        wall.letters.keys().copied().find(|c| *c > letter)
                    } else {
                        wall.letters.keys().rev().copied().find(|c| *c < letter)
                    };
                    wall.rail_focus = next.or(Some(letter));
                    if let Some(c) = wall.rail_focus {
                        wall.reveal_letter(c, descending);
                    }
                }
                _ => return false,
            }
            return true;
        }
        if alphabetical
            && key.len() == 1
            && let Some(c) = key.chars().next()
            && (c.is_ascii_alphabetic() || c == '#')
        {
            self.jump_to_letter(c.to_ascii_uppercase(), w, cx);
            return true;
        }
        let count = wall.total.unwrap_or(0);
        if count == 0 {
            return false;
        }
        let columns = wall.columns;
        // A wheel scroll can leave the last focused poster well outside the viewport.
        let selected = if !wall.visible.is_empty()
            && !wall.visible.contains(&self.state.selection)
            && wall.scroll.0.borrow().deferred_scroll_to_item.is_none()
        {
            (wall.visible.start + self.state.selection % columns).min(count - 1)
        } else {
            self.state.selection
        };
        let stride = (wall.visible.len() / columns).max(1) * columns;
        self.state.selection = match key {
            "right"
                if alphabetical
                    && !wall.letters.is_empty()
                    && ((selected + 1).is_multiple_of(columns) || selected + 1 == count) =>
            {
                wall.rail_focus = wall
                    .item(selected)
                    .map(|i| catalog_initial(i.sort_name.as_deref(), &i.name))
                    .filter(|l| wall.letters.contains_key(l))
                    .or_else(|| wall.letters.keys().next().copied());
                if let Some(c) = wall.rail_focus {
                    wall.reveal_letter(c, descending);
                }
                return true;
            }
            "right" => selected + 1,
            "left" => selected.saturating_sub(1),
            "down" => selected + columns,
            "up" => selected.saturating_sub(columns),
            "pagedown" => selected + stride,
            "pageup" => selected.saturating_sub(stride),
            "home" => 0,
            "end" => count - 1,
            "enter" => {
                if let Some(item) = wall.item(selected).cloned() {
                    self.open_item(item, w, cx);
                }
                return true;
            }
            _ => return false,
        }
        .min(count - 1);
        wall.scroll.scroll_to_item(
            self.state.selection / columns,
            if self.state.selection < selected {
                ScrollStrategy::Top
            } else {
                ScrollStrategy::Bottom
            },
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{CatalogWall, Page, catalog_initial};

    fn page(start: usize, end: usize, total: usize) -> Page {
        let items = (start..end)
            .map(|i| {
                serde_json::from_value(
                    serde_json::json!({"Id": i.to_string(), "Name": format!("Movie {i}")}),
                )
                .unwrap()
            })
            .collect();
        Page {
            items,
            total_record_count: total,
        }
    }

    #[test]
    fn a_fast_jump_fetches_the_destination_and_accepts_short_server_pages() {
        let mut wall = CatalogWall::default();
        assert_eq!(wall.needed_offset(), Some(0));
        wall.receive(0, page(0, 90, 4000), false);
        wall.columns = 15;
        wall.visible = 3195..3270;
        assert_eq!(wall.needed_offset(), Some(3150));
        wall.receive(3150, page(3150, 3190, 4000), false);
        assert_eq!(wall.needed_offset(), Some(3190));
        wall.receive(3190, page(3190, 3280, 4000), false);
        assert_eq!(wall.needed_offset(), Some(3280));
        assert_eq!(wall.index_of("3210"), Some(3210));
    }

    #[test]
    fn shrinking_a_filtered_library_discards_stale_titles_and_clamps_visible_artwork() {
        let mut wall = CatalogWall::default();
        wall.receive(0, page(0, 90, 91), true);
        wall.columns = 4;
        wall.visible = 88..91;
        wall.receive(90, page(0, 0, 2), true);
        assert!(wall.item(87).is_none());
        assert_eq!(wall.near_items().count(), 0);
        wall.visible = 0..2;
        assert_eq!(wall.needed_offset(), Some(0));
        assert!(wall.letters.is_empty());
    }

    #[test]
    fn letters_use_server_order_sort_titles_and_the_first_matching_position() {
        let mut wall = CatalogWall::default();
        wall.index_letters(['Z', 'Z', 'T']);
        wall.index_letters([
            catalog_initial(Some(" John Wick "), "The John Wick Collection"),
            catalog_initial(None, "John Wick"),
            catalog_initial(Some(""), "1917"),
        ]);
        assert_eq!(wall.letters[&'Z'], 0);
        assert_eq!(wall.letters[&'J'], 3);
        assert_eq!(wall.letters[&'#'], 5);
    }
}
