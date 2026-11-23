use super::{Library, Screen};
use crate::jellyfin::{Art, ArtKind, Item};
use gpui::*;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[derive(Default)]
pub(super) struct Artwork {
    pub(super) trickplay: super::trickplay::Trickplay,
    pub(super) glass: super::glass::ArtworkGlass,
    entries: HashMap<Art, Option<Arc<RenderImage>>>,
    pending: HashSet<Art>,
    session: u64,
}

pub(super) fn background(item: &Item) -> Option<Art> {
    item.art(ArtKind::Backdrop)
        .or_else(|| item.art(ArtKind::Thumb))
}

impl Artwork {
    pub fn get(&self, key: Option<Art>) -> Option<Arc<RenderImage>> {
        self.entries.get(&key?)?.as_ref().cloned()
    }

    pub fn clear(&mut self, w: &mut Window, cx: &mut App) {
        for img in self.entries.drain().filter_map(|(_, img)| img) {
            cx.drop_image(img, Some(w));
        }
        self.trickplay.clear(w, cx);
        self.glass.clear(w, cx);
        self.session += 1;
        self.pending.clear();
    }

    pub fn loading(&self) -> bool {
        !self.pending.is_empty()
    }

    pub fn retry(&mut self) {
        self.entries.retain(|_, v| v.is_some());
    }
}

impl Library {
    pub(super) fn preview_chapter(&self) -> Option<(usize, &crate::jellyfin::Chapter, f64)> {
        let p = self.player.as_ref()?;
        let frac = self.scrub.or(self.controls.hover)?;
        let at = frac as f64 * p.status.duration;
        let chapters = self.playing.as_ref()?.chapters.as_ref()?;
        let index = chapters
            .partition_point(|c| c.start_position_ticks <= (at * 10_000_000.) as u64)
            .checked_sub(1)?;
        Some((index, &chapters[index], at))
    }
    pub(super) fn load_artwork(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(server) = self.server.as_ref() else {
            return;
        };
        if self.downloads.playing.is_some() && !self.controls.mini {
            return;
        }
        let mut plan = Vec::new();
        if self.player.is_some()
            && let Some(next) = self.next_title()
        {
            plan.extend(next.landscape());
        }
        if let Some((i, _, _)) = self.preview_chapter()
            && let Some(item) = &self.playing
        {
            plan.extend(item.art(ArtKind::Chapter(i)));
        }
        if let Some(item) = self.playing.as_ref().filter(|i| i.kind == "Audio") {
            plan.extend(item.art(ArtKind::Poster));
        }
        match &self.state.screen {
            Screen::Discover => {
                if let Some(item) = self.featured.get(self.hero) {
                    plan.extend(super::screens::marquee_art(item, w));
                    plan.extend(item.art(ArtKind::Logo));
                }
                // Load the first visible cards from each shelf before the off-screen cards.
                for start in [0..5, 5..24] {
                    for shelf in &self.shelves {
                        for item in shelf.items.iter().skip(start.start).take(start.len()) {
                            plan.extend(shelf.kind.art(item));
                        }
                    }
                    if start.start == 0 && self.featured.len() > 1 {
                        let next = &self.featured[(self.hero + 1) % self.featured.len()];
                        plan.extend(super::screens::marquee_art(next, w));
                        plan.extend(next.art(ArtKind::Logo));
                    }
                }
            }
            Screen::Detail(item, _) => {
                let (layout, range, items) = self.state.similar_cards(w);
                if matches!(item.kind.as_str(), "Movie" | "Series") {
                    plan.extend(super::screens::marquee_art(item, w));
                } else {
                    plan.extend(background(item));
                }
                plan.extend(item.art(ArtKind::Logo));
                if let Some(ep) = &self.state.series_target {
                    plan.extend(ep.landscape());
                }
                if let Some(page) = &self.state.page {
                    plan.extend(page.items.iter().filter_map(Item::landscape));
                }
                if matches!(item.kind.as_str(), "Movie" | "Series") {
                    let (cast, extras) = super::screens::credit_ranges(&self.state, item, w);
                    plan.extend(
                        item.people.as_deref().unwrap_or_default()[cast]
                            .iter()
                            .filter_map(|p| p.portrait(item.origin.as_deref())),
                    );
                    plan.extend(self.state.extras[extras].iter().filter_map(Item::landscape));
                }
                plan.extend(
                    items[range]
                        .iter()
                        .filter_map(|item| item.art(layout.poster)),
                );
            }
            Screen::Library(..) => {
                let poster =
                    super::screens::CatalogGrid::for_wall(w, self.state.screen.alphabetical())
                        .poster;
                plan.extend(self.state.wall.near_items().filter_map(|i| i.art(poster)));
            }
            _ => {
                if let Some(page) = &self.state.page {
                    let poster = match self.state.screen {
                        Screen::Search(_) => super::screens::CatalogGrid::for_window(w).poster,
                        _ => ArtKind::Poster,
                    };
                    plan.extend(page.items.iter().filter_map(|i| i.art(poster)));
                }
            }
        }
        // Keep adjacent pages warm, but release decoded images and their GPU atlas entries
        // as the user moves through a long library.
        if self.artwork.entries.len() > 65 {
            let keep: HashSet<_> = plan.iter().collect();
            self.artwork.entries.retain(|k, image| {
                if keep.contains(k) {
                    return true;
                }
                if let Some(image) = image.take() {
                    cx.drop_image(image, Some(w));
                }
                false
            });
        }
        for key in plan {
            if self.artwork.pending.len() >= 5 {
                break;
            }
            if self.artwork.entries.contains_key(&key) || !self.artwork.pending.insert(key.clone())
            {
                continue;
            }
            let server = key
                .origin
                .as_ref()
                .and_then(|origin| {
                    self.connected
                        .iter()
                        .find(|s| s.identity().is_some_and(|a| &a.key() == origin))
                })
                .unwrap_or(server)
                .clone();
            let session = self.artwork.session;
            let svg = cx.svg_renderer();
            let fetch = key.clone();
            let work = cx.background_executor().spawn(async move {
                let bytes = server.artwork(&fetch).ok()?;
                let format = if fetch.kind == ArtKind::Logo {
                    ImageFormat::Png
                } else {
                    ImageFormat::Jpeg
                };
                Image::from_bytes(format, bytes).to_image_data(svg).ok()
            });
            cx.spawn_in(w, async move |view, cx| {
                let image = work.await;
                let _ = view.update(cx, |s, cx| {
                    if s.artwork.session == session {
                        s.artwork.pending.remove(&key);
                        s.artwork.entries.insert(key, image);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }
}
