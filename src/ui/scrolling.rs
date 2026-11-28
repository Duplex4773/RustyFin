use super::*;
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETCLIENTAREAANIMATION, SPI_GETWHEELSCROLLLINES, SystemParametersInfoW,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Surface {
    Page,
    Shelf(ShelfKind),
    PlayerMenu(PlayerMenu),
    Lyrics,
}

pub(super) struct WheelMotion {
    surface: Surface,
    position: Pixels,
    target: Pixels,
    remaining: Pixels,
    last_frame: Instant,
}

impl WheelMotion {
    fn push(
        previous: Option<Self>,
        surface: Surface,
        position: Pixels,
        delta: Pixels,
        limit: Pixels,
        now: Instant,
    ) -> Option<Self> {
        let previous = previous.filter(|p| p.surface == surface && p.position == position);
        let base = previous.as_ref().map_or(position, |p| {
            // Reversing the wheel should reverse the very next frame.
            if (p.target > position) == (delta > px(0.)) {
                p.target
            } else {
                position
            }
        });
        let target = (base + delta).clamp(-limit, px(0.));
        (position != target).then(|| Self {
            surface,
            position,
            target,
            remaining: target - position,
            last_frame: previous.map_or(now, |p| p.last_frame),
        })
    }

    fn advance(&mut self, now: Instant, limit: Pixels) {
        let target = self.target.clamp(-limit, px(0.));
        self.remaining += target - self.target;
        self.target = target;
        let elapsed = now.duration_since(self.last_frame).as_secs_f32();
        // Decay the remaining distance, independently of frame rate and the
        // rounding of large pixel offsets deep in a library.
        self.remaining *= (-elapsed / 0.043).exp();
        self.position = if self.remaining.abs() < px(0.5) {
            self.target
        } else {
            self.target - self.remaining
        };
        self.last_frame = now;
    }
}

fn wheel_lines() -> Option<u32> {
    let mut lines = 0u32;
    // SAFETY: SPI_GETWHEELSCROLLLINES writes one UINT.
    unsafe {
        SystemParametersInfoW(
            SPI_GETWHEELSCROLLLINES,
            0,
            Some((&raw mut lines).cast()),
            Default::default(),
        )
    }
    .ok()?;
    (lines > 0).then_some(lines)
}

pub(super) fn animations_enabled() -> bool {
    let mut enabled = windows::core::BOOL::default();
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL.
    unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut enabled).cast()),
            Default::default(),
        )
    }
    .is_ok()
        && enabled.as_bool()
}

impl ViewState {
    fn wheel_input(
        &mut self,
        surface: Surface,
        handle: &ScrollHandle,
        delta: ScrollDelta,
        before: Pixels,
    ) {
        let previous = self.wheel_motion.take();
        // GPUI reports Windows precision scrolling as fractions of the system's
        // lines-per-notch setting. A single whole line can still be a partial notch.
        let ScrollDelta::Lines(lines) = delta else {
            return;
        };
        if lines.x != 0. || lines.y == 0. {
            return;
        }
        let Some(per_notch) = wheel_lines() else {
            return;
        };
        let notches = lines.y / per_notch as f32;
        let offset = handle.offset();
        let distance = if per_notch == u32::MAX {
            // Windows uses UINT_MAX for "one screen at a time".
            handle.bounds().size.height * notches
        } else {
            offset.y - before
        };
        let limit = handle.max_offset().height;
        if notches.fract() == 0. && animations_enabled() {
            handle.set_offset(point(offset.x, before));
            self.wheel_motion =
                WheelMotion::push(previous, surface, before, distance, limit, Instant::now());
        } else if per_notch == u32::MAX {
            handle.set_offset(point(offset.x, (before + distance).clamp(-limit, px(0.))));
        }
    }

    pub(super) fn page_scroll(&self) -> ScrollHandle {
        if matches!(self.screen, Screen::Library(..)) {
            self.wall.scroll.0.borrow().base_handle.clone()
        } else {
            self.scroll.clone()
        }
    }
}

impl Library {
    pub(super) fn page_shelf(
        &mut self,
        kind: ShelfKind,
        handle: &ScrollHandle,
        delta: Pixels,
        cx: &mut Context<Self>,
    ) {
        let offset = handle.offset();
        let limit = handle.max_offset().width;
        self.state.shelf_motion = if animations_enabled() {
            WheelMotion::push(
                self.state.shelf_motion.take(),
                Surface::Shelf(kind),
                offset.x,
                delta,
                limit,
                Instant::now(),
            )
        } else {
            handle.set_offset(point((offset.x + delta).clamp(-limit, px(0.)), offset.y));
            None
        };
        cx.notify();
    }

    pub(super) fn advance_shelf(&mut self, animate: bool, w: &Window) {
        let Some(mut motion) = self.state.shelf_motion.take() else {
            return;
        };
        let Some((_, handle)) = self
            .shelves
            .iter()
            .zip(&self.shelf_scroll)
            .find(|(shelf, _)| motion.surface == Surface::Shelf(shelf.kind))
        else {
            return;
        };
        let offset = handle.offset();
        if offset.x != motion.position {
            return;
        }
        motion.advance(Instant::now(), handle.max_offset().width);
        handle.set_offset(point(
            if animate {
                motion.position
            } else {
                motion.target
            },
            offset.y,
        ));
        if animate && motion.position != motion.target {
            self.state.shelf_motion = Some(motion);
            w.request_animation_frame();
        }
    }

    pub(super) fn smooth_scrolling(
        &mut self,
        w: &Window,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let (surface, handle) = match self.player.as_ref().filter(|_| !self.controls.mini) {
            Some(p) if p.panel_open => (
                Surface::PlayerMenu(self.controls.menu),
                self.menu_focus.scroll.clone(),
            ),
            Some(p) if p.audio_only => (Surface::Lyrics, self.controls.lyric_scroll.clone()),
            Some(_) => {
                self.state.wheel_motion = None;
                return None;
            }
            None => (Surface::Page, self.state.page_scroll()),
        };
        if let Some(mut motion) = self.state.wheel_motion.take()
            && motion.surface == surface
            && motion.position == handle.offset().y
        {
            motion.advance(Instant::now(), handle.max_offset().height);
            handle.set_offset(point(handle.offset().x, motion.position));
            if motion.position != motion.target {
                self.state.wheel_motion = Some(motion);
                w.request_animation_frame();
            }
        }

        let view = cx.weak_entity();
        Some(
            canvas(
                |_, _, _| (),
                move |_, _, w, _| {
                    let mut before = px(0.);
                    // The root is painted before the scroll containers. Observe the offset
                    // around GPUI's wheel dispatch so its hit testing selects the recipient,
                    // including when a popup or the alphabet rail owns the gesture.
                    w.on_mouse_event(move |event: &ScrollWheelEvent, phase, _, cx| {
                        let offset = handle.offset();
                        if phase == DispatchPhase::Capture {
                            before = offset.y;
                        } else if offset.y != before {
                            let _ = view.update(cx, |s, _| {
                                s.state.wheel_input(surface, &handle, event.delta, before);
                            });
                        }
                    });
                },
            )
            .absolute()
            .size(px(0.)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Surface, WheelMotion};
    use gpui::px;
    use std::time::{Duration, Instant};

    #[test]
    fn wheel_notches_accumulate_and_reversing_discards_pending_distance() {
        let start = Instant::now();
        let limit = px(400.);
        let mut m =
            WheelMotion::push(None, Surface::Page, px(-80.), px(-50.), limit, start).unwrap();
        let next = start + Duration::from_millis(16);
        m.advance(next, limit);
        let position = m.position;
        assert!(position < px(-80.) && position > px(-130.));
        let m = WheelMotion::push(Some(m), Surface::Page, position, px(-50.), limit, next).unwrap();
        assert_eq!(m.target, px(-180.));
        let mut m =
            WheelMotion::push(Some(m), Surface::Page, position, px(30.), limit, next).unwrap();
        assert_eq!(m.target, position + px(30.));
        m.advance(next + Duration::from_millis(8), limit);
        assert!(m.position > position);
        assert!(WheelMotion::push(None, Surface::Page, -limit, px(-40.), limit, start).is_none());
    }

    #[test]
    fn travel_depends_on_elapsed_time_and_settles_at_the_edge() {
        let start = Instant::now();
        let limit = px(500.);
        let at = |millis| start + Duration::from_millis(millis);
        let motion =
            || WheelMotion::push(None, Surface::Page, px(-350.), px(-300.), limit, start).unwrap();
        let mut a = motion();
        let mut b = motion();
        a.advance(at(48), limit);
        for millis in [8, 16, 24, 32, 40, 48] {
            b.advance(at(millis), limit);
        }
        assert!((a.position - b.position).abs() < px(0.01));
        b.advance(at(350), limit);
        assert_eq!(b.position, -limit);
    }
}
