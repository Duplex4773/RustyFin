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
        let items: Box<dyn Iterator<Item = &Item> + '_> = match &self.state.screen {
            Screen::Discover => Box::new(self.shelves.iter().zip(&self.shelf_scroll).flat_map(
                |(s, handle)| {
                    s.items.iter().enumerate().filter_map(move |(i, item)| {
                        let near = handle.bounds_for_item(i).map_or(i < 6, |card| {
                            let left = card.left() + handle.offset().x;
                            left < handle.bounds().right() + card.size.width
                                && left + card.size.width * 2. > handle.bounds().left()
                        });
                        near.then_some(item)
                    })
                },
            )),
            Screen::Library(..) | Screen::Search(_) => {
                let columns = screens::CatalogGrid::for_window(w).columns;
                let first = self.state.scroll.top_item().saturating_sub(1);
                let end = (self.state.scroll.bottom_item() + 2).max(first + 3);
                Box::new(
                    self.state
                        .page
                        .iter()
                        .flat_map(|p| &p.items)
                        .skip(first * columns)
                        .take((end - first) * columns),
                )
            }
            Screen::Detail(..) => Box::new(self.state.page.iter().flat_map(|p| &p.items)),
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
