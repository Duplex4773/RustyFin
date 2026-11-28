use super::*;
use crate::jellyfin::formats::Formats;
use std::collections::HashMap;

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key(Vec<(Option<String>, String, Option<String>)>);

impl Key {
    fn for_item(item: &Item) -> Self {
        Self(
            std::iter::once(item)
                .chain(&item.copies)
                .map(|i| {
                    (
                        i.origin.clone(),
                        i.id.clone(),
                        i.date_last_media_added.clone(),
                    )
                })
                .collect(),
        )
    }
}

#[derive(Default)]
pub(super) struct CardFormats {
    // None is an in-flight request. Failed and empty results retry on refresh.
    entries: HashMap<Key, Option<Formats>>,
    pending: usize,
    generation: u64,
    pub(super) expanded: Option<(ElementId, String)>,
}

fn needs_summary(item: &Item) -> bool {
    item.format_group()
        || !item.copies.is_empty()
        || item.media_sources.as_ref().is_some_and(|v| v.len() > 1)
}

impl CardFormats {
    pub fn loading(&self) -> bool {
        self.pending > 0
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.pending = 0;
        self.expanded = None;
        self.generation += 1;
    }

    pub fn for_item(&self, item: &Item) -> Option<Formats> {
        if needs_summary(item) {
            self.entries.get(&Key::for_item(item))?.clone()
        } else {
            Some(item.file_formats(None))
        }
    }
}

impl Library {
    pub(super) fn load_card_formats(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.formats.pending >= 2 || self.server.is_none() {
            return;
        }
        let items: Box<dyn Iterator<Item = &Item> + '_> =
            match &self.state.screen {
                Screen::Discover => Box::new(self.shelves.iter().zip(&self.shelf_scroll).flat_map(
                    |(shelf, scroll)| {
                        shelf.items.iter().enumerate().filter_map(move |(i, item)| {
                            let near = scroll.bounds_for_item(i).map_or(i < 6, |card| {
                                let left = card.left() + scroll.offset().x;
                                let viewport = scroll.bounds();
                                left < viewport.right() + card.size.width
                                    && left + card.size.width * 2. > viewport.left()
                            });
                            near.then_some(item)
                        })
                    },
                )),
                Screen::Library(..) => Box::new(self.state.wall.near_items()),
                Screen::Search(_) => Box::new(self.state.search.groups.iter().flat_map(|g| {
                    &g.page.items[g.visible(screens::search_grid(g.kind, w).columns)]
                })),
                Screen::Detail(item, _) => {
                    let (_, range, similar) = self.state.similar_cards(w);
                    let (_, extras) = screens::credit_ranges(&self.state, item, w);
                    Box::new(
                        self.state
                            .series_target
                            .iter()
                            .chain(self.state.page.iter().flat_map(|p| &p.items))
                            .chain(&similar[range])
                            .chain(
                                self.state
                                    .extra_items()
                                    .skip(extras.start)
                                    .take(extras.len()),
                            ),
                    )
                }
                _ => return,
            };
        let mut requests = Vec::new();
        for item in items.filter(|i| needs_summary(i)) {
            let key = Key::for_item(item);
            if self.formats.entries.contains_key(&key) {
                continue;
            }
            let mut copies = Vec::new();
            for copy in std::iter::once(item).chain(&item.copies) {
                if let Some(server) = self.source_server(copy.origin.as_deref()) {
                    copies.push((server, copy.clone()));
                }
            }
            self.formats.entries.insert(key.clone(), None);
            requests.push((key, copies));
            self.formats.pending += 1;
            if self.formats.pending == 2 {
                break;
            }
        }
        let generation = self.formats.generation;
        for (key, copies) in requests {
            let work = cx.background_executor().spawn(async move {
                let mut summary = Formats::default();
                for (server, item) in copies {
                    summary.merge(server.card_formats(&item)?);
                }
                anyhow::Ok(summary)
            });
            cx.spawn_in(w, async move |view, cx| {
                let summary = work.await.unwrap_or_default();
                let _ = view.update(cx, |s, cx| {
                    if s.formats.generation == generation {
                        s.formats.pending -= 1;
                        s.formats.entries.insert(key, Some(summary));
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }
}
