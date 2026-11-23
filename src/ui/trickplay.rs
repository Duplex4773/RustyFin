use super::*;
use crate::jellyfin::trickplay::{Frame, Sheet};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct Trickplay {
    generation: u64,
    pending: Option<Sheet>,
    sheets: std::collections::VecDeque<(Sheet, image::RgbImage)>,
    failed: Option<(Sheet, Instant)>,
    displayed: Option<(Frame, Option<Arc<RenderImage>>)>,
}

impl Trickplay {
    pub fn active(&self) -> bool {
        self.pending.is_some()
            || self.displayed.is_some()
            || !self.sheets.is_empty()
            || self.failed.is_some()
    }

    pub fn clear(&mut self, w: &mut Window, cx: &mut App) {
        self.replace(None, w, cx);
        *self = Self {
            generation: self.generation.wrapping_add(1),
            ..Default::default()
        };
    }

    fn replace(
        &mut self,
        frame: Option<(Frame, Option<Arc<RenderImage>>)>,
        w: &mut Window,
        cx: &mut App,
    ) {
        if let Some((_, Some(old))) = std::mem::replace(&mut self.displayed, frame) {
            cx.drop_image(old, Some(w));
        }
    }

    pub fn preview(&self) -> Option<Arc<RenderImage>> {
        self.displayed.as_ref()?.1.clone()
    }
}

impl Library {
    pub(super) fn load_trickplay(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let item = self.playing.as_ref();
        let frame = self
            .player
            .as_ref()
            .filter(|p| {
                p.seekable
                    && !p.audio_only
                    && (p.controls_visible || self.controls.remote_scrub)
                    && !p.panel_open
                    && self.downloads.playing.is_none()
            })
            .zip(item)
            .and_then(|(player, item)| {
                let at = f64::from(self.scrub.or(self.controls.hover)?) * player.status.duration;
                item.trickplay_frame(
                    &self.controls.source,
                    at,
                    (super::screens::PREVIEW_WIDTH * w.scale_factor()).ceil() as u32,
                )
            });
        let cache = &mut self.artwork.trickplay;
        if cache.displayed.as_ref().map(|(f, _)| *f) == frame {
            return;
        }
        let Some(frame) = frame else {
            cache.replace(None, w, cx);
            return;
        };
        if let Some((_, image)) = cache.sheets.iter().find(|(sheet, _)| *sheet == frame.sheet) {
            let preview = frame
                .crop(image)
                .ok()
                .map(|img| Arc::new(RenderImage::new(vec![img])));
            cache.replace(Some((frame, preview)), w, cx);
            return;
        }
        cache.replace(None, w, cx);
        if cache.pending.is_some()
            || cache
                .failed
                .is_some_and(|(s, t)| s == frame.sheet && t.elapsed() < Duration::from_secs(30))
        {
            return;
        }
        let generation = cache.generation;
        let Some(item) = item else { return };
        let Some(server) = self.source_server(item.origin.as_deref()) else {
            return;
        };
        self.artwork.trickplay.pending = Some(frame.sheet);
        let id = item.id.clone();
        let source = self.controls.source.clone();
        let sheet = frame.sheet;
        let fetch = cx
            .background_executor()
            .spawn(async move { server.trickplay_sheet(&id, &source, sheet) });
        cx.spawn_in(w, async move |view, cx| {
            let result = fetch.await;
            let _ = view.update(cx, |s, cx| {
                let cache = &mut s.artwork.trickplay;
                if cache.generation != generation {
                    return;
                }
                cache.pending = None;
                match result {
                    Ok(image) => {
                        cache.sheets.push_front((sheet, image));
                        cache.sheets.truncate(2);
                    }
                    Err(_) => cache.failed = Some((sheet, Instant::now())),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
