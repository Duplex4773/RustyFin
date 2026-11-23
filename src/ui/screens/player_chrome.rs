use super::*;
use crate::{
    player::{RAIL_INSET, TIMELINE_HEIGHT},
    ui::playback::PlayerMenu,
};

pub(super) const GOLD: u32 = 0xffc107;
pub(super) const PANEL: u32 = 0x15161b;
const EDGE: f32 = 14.;
const TIME_WIDTH: f32 = RAIL_INSET - EDGE - 8.;
pub(in crate::ui) const PREVIEW_WIDTH: f32 = 148.;
const PREVIEW_HEIGHT: f32 = 83.;

fn rail(width: f32) -> (f32, f32) {
    let start = RAIL_INSET;
    (start, (width - 2. * start).max(1.))
}
fn fraction_at(x: Pixels, w: &Window) -> f32 {
    let (start, length) = rail(w.viewport_size().width.into());
    ((f32::from(x) - start) / length).clamp(0., 1.)
}
fn floating(bounds: Bounds<Pixels>) -> Div {
    div()
        .absolute()
        .left(bounds.left())
        .top(bounds.top())
        .w(bounds.size.width)
        .h(bounds.size.height)
}

fn transport_button(id: &'static str, code: &'static str) -> Stateful<Div> {
    icon_button(id, icon(code, 21.))
}

impl Library {
    pub(in crate::ui) fn toggle_player_menu(
        &mut self,
        page: PlayerMenu,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(p) = &mut self.player else { return };
        if p.panel_open && self.controls.menu == page {
            self.close_player_menu(w, cx);
        } else {
            p.panel_open = true;
            p.controls_visible = true;
            self.end_preview();
            self.menu_focus.origin = page;
            self.menu_focus.settings_row = PlayerMenu::Speed;
            self.enter_player_menu(page, cx);
        }
    }

    pub(in crate::ui) fn enter_player_menu(&mut self, page: PlayerMenu, cx: &mut Context<Self>) {
        self.controls.menu = page;
        self.menu_focus.entering.set(true);
        cx.notify();
    }

    fn close_player_menu(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = &mut self.player {
            p.panel_open = false;
        }
        self.menu_focus.entering.set(false);
        self.menu_focus.launcher(self.menu_focus.origin).focus(w);
        cx.notify();
    }

    pub(in crate::ui) fn back_player_menu(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.controls.menu == PlayerMenu::Settings
            || self.controls.menu == self.menu_focus.origin
        {
            self.close_player_menu(w, cx);
        } else {
            self.menu_focus.settings_row = self.controls.menu;
            self.enter_player_menu(PlayerMenu::Settings, cx);
        }
    }

    pub(in crate::ui) fn move_menu_focus(
        &self,
        forward: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old = w
            .focused(cx)
            .filter(|_| self.menu_focus.scope.contains_focused(w, cx));
        if forward {
            w.focus_next();
        } else {
            w.focus_prev();
        }
        if !self.menu_focus.scope.contains_focused(w, cx) {
            old.unwrap_or_else(|| self.menu_focus.close.clone())
                .focus(w);
        }
        self.menu_focus
            .reveal(w, cx, !self.controls.warning.is_empty());
        cx.notify();
    }

    pub(in crate::ui) fn start_preview(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = &self.player
            && !p.panel_open
            && p.seekable
            && !p.audio_only
            && p.status.duration > 0.
        {
            self.controls.remote_scrub = true;
            self.scrub = Some((p.status.position / p.status.duration).clamp(0., 1.) as f32);
            self.focus.focus(w);
            cx.notify();
        }
    }

    pub(in crate::ui) fn end_preview(&mut self) {
        self.controls.remote_scrub = false;
        self.scrub = None;
        self.controls.hover = None;
    }

    fn finish_scrub(&mut self, event: &MouseUpEvent, w: &mut Window, cx: &mut Context<Self>) {
        if self.scrub.take().is_some()
            && let Some(p) = &self.player
        {
            let at = f64::from(fraction_at(event.position.x, w)) * p.status.duration;
            self.control(Some(at - p.status.position), cx);
        }
    }

    fn timeline(&self, p: &Player, time: f64, progress: f32, cx: &mut Context<Self>) -> Div {
        let track = div()
            .id("timeline")
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .cursor_pointer()
            .child(
                div()
                    .w_full()
                    .h(px(5.))
                    .rounded_full()
                    .bg(rgb(0x53555b))
                    .child(
                        div()
                            .relative()
                            .h_full()
                            .w(relative(progress))
                            .rounded_full()
                            .bg(rgb(0xffffff))
                            .child(
                                div()
                                    .absolute()
                                    .right(px(-2.))
                                    .top(px(-4.))
                                    .w(px(3.))
                                    .h(px(13.))
                                    .rounded_full()
                                    .bg(rgb(0xffffff)),
                            ),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, e: &MouseDownEvent, w, cx| {
                    if s.player
                        .as_ref()
                        .is_some_and(|p| p.seekable && p.status.duration > 0.)
                    {
                        s.controls.remote_scrub = false;
                        s.scrub = Some(fraction_at(e.position.x, w));
                        cx.notify();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|s, e: &MouseMoveEvent, w, cx| {
                let value = fraction_at(e.position.x, w);
                s.controls.hover = Some(value);
                if s.scrub.is_some() && !s.controls.remote_scrub {
                    s.scrub = Some(value);
                }
                cx.notify();
            }))
            .on_hover(cx.listener(|s, inside, _, cx| {
                if !inside {
                    s.controls.hover = None;
                    cx.notify();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::finish_scrub))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::finish_scrub));
        div()
            .w_full()
            .h(px(TIMELINE_HEIGHT))
            .flex_none()
            .px(px(EDGE))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                label(clock(time), 13., TEXT)
                    .w(px(TIME_WIDTH))
                    .flex_none()
                    .text_right(),
            )
            .child(track)
            .child(
                label(format!("−{}", clock(p.status.duration - time)), 13., TEXT)
                    .w(px(TIME_WIDTH))
                    .flex_none(),
            )
    }

    fn volume_control(&self, compact: bool, cx: &mut Context<Self>) -> Div {
        let value = self.volume;
        let bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::<Pixels>::default()));
        let measured = bounds.clone();
        let slider = div()
            .id("volume-slider")
            .relative()
            .w(px(if compact { 48. } else { 82. }))
            .h(px(28.))
            .cursor_pointer()
            .flex()
            .items_center()
            .child(
                div()
                    .w_full()
                    .h(px(5.))
                    .rounded_full()
                    .bg(rgb(0x505157))
                    .child(
                        div()
                            .relative()
                            .h_full()
                            .w(relative(value as f32))
                            .rounded_full()
                            .bg(rgb(TEXT))
                            .child(
                                div()
                                    .absolute()
                                    .right(px(-1.))
                                    .top(px(-3.))
                                    .w(px(3.))
                                    .h(px(11.))
                                    .rounded_full()
                                    .bg(rgb(TEXT)),
                            ),
                    ),
            )
            .child(
                canvas(move |b, _, _| measured.set(b), |_, _, _, _| {})
                    .absolute()
                    .inset_0()
                    .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let bounds = bounds.clone();
                    move |s, e: &MouseDownEvent, _, cx| {
                        let b = bounds.get();
                        if b.size.width > px(0.) {
                            s.volume(
                                Some(f64::from((e.position.x - b.left()) / b.size.width)),
                                cx,
                            );
                        }
                    }
                }),
            )
            .on_mouse_move(cx.listener(move |s, e: &MouseMoveEvent, _, cx| {
                let b = bounds.get();
                if e.pressed_button == Some(MouseButton::Left) && b.size.width > px(0.) {
                    s.volume(
                        Some(f64::from((e.position.x - b.left()) / b.size.width)),
                        cx,
                    );
                }
            }));
        div()
            .flex()
            .items_center()
            .gap_1()
            .mr_2()
            .child(
                transport_button(
                    if value == 0. { "Unmute" } else { "Mute" },
                    if value == 0. { "\u{e74f}" } else { "\u{e767}" },
                )
                .on_click(cx.listener(|s, _, _, cx| s.volume(None, cx))),
            )
            .child(slider)
    }

    fn menu_button(
        &self,
        name: &'static str,
        glyph: &'static str,
        page: PlayerMenu,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        transport_button(name, glyph)
            .track_focus(self.menu_focus.launcher(page))
            .when(
                self.player.as_ref().is_some_and(|p| p.panel_open) && self.controls.menu == page,
                |d| d.text_color(rgb(GOLD)),
            )
            .on_click(cx.listener(move |s, _, w, cx| s.toggle_player_menu(page, w, cx)))
    }

    fn skip_button(&self, forward: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let name = if forward {
            "Forward 10 seconds"
        } else {
            "Back 10 seconds"
        };
        tip(
            div()
                .id(name)
                .size(px(36.))
                .flex_none()
                .relative()
                .rounded_full()
                .cursor_pointer()
                .hover(|d| d.bg(rgb(ACTIVE)))
                .child(
                    div()
                        .absolute()
                        .left(px(7.))
                        .top(px(8.))
                        .size(px(21.))
                        .rounded_full()
                        .border_2()
                        .border_color(rgb(TEXT))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            label("10", 9., TEXT)
                                .line_height(px(11.))
                                .font_weight(FontWeight::BOLD),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(2.))
                        .left(px(if forward { 16. } else { 8. }))
                        .child(icon(if forward { "\u{e76c}" } else { "\u{e76b}" }, 10.)),
                ),
            name,
        )
        .on_click(
            cx.listener(move |s, _, _, cx| s.control(Some(if forward { 10. } else { -10. }), cx)),
        )
    }

    fn transport(
        &self,
        p: &Player,
        w: &Window,
        time: f64,
        progress: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let compact = w.viewport_size().width < px(820.);
        let chapters = self
            .playing
            .as_ref()
            .and_then(|item| item.chapters.as_deref())
            .unwrap_or_default();
        let next = chapters
            .iter()
            .find(|c| c.start_position_ticks as f64 / 10_000_000. > p.status.position + 0.25);
        let previous = chapters
            .iter()
            .rev()
            .find(|c| c.start_position_ticks as f64 / 10_000_000. < p.status.position - 2.);
        let jump = |name, glyph, ticks: u64, cx: &mut Context<Self>| {
            transport_button(name, glyph).on_click(cx.listener(move |s, _, _, cx| {
                if let Some(p) = &s.player {
                    s.control(Some(ticks as f64 / 10_000_000. - p.status.position), cx);
                }
            }))
        };
        let mut row = div()
            .w_full()
            .flex_1()
            .min_h_0()
            .px(px(EDGE))
            .flex()
            .items_center()
            .gap(px(if compact { 0. } else { 3. }))
            .child(
                transport_button("Back to title", BACK).on_click(cx.listener(|s, _, _, cx| {
                    s.stop();
                    cx.notify();
                })),
            );
        if p.seekable {
            row = row
                .child(jump(
                    "Previous chapter or restart",
                    "\u{e892}",
                    previous.map_or(0, |c| c.start_position_ticks),
                    cx,
                ))
                .child(self.skip_button(false, cx));
        }
        row = row.child(
            transport_button(
                if p.status.paused { "Play" } else { "Pause" },
                if p.status.paused { "\u{f5b0}" } else { PAUSE },
            )
            .on_click(cx.listener(|s, _, _, cx| s.control(None, cx))),
        );
        if p.seekable {
            row = row.child(self.skip_button(true, cx));
            if let Some(chapter) = next {
                row = row.child(jump(
                    "Next chapter",
                    "\u{e893}",
                    chapter.start_position_ticks,
                    cx,
                ));
            } else if self.next_title().is_some() {
                row = row.child(
                    transport_button("Next title", "\u{e893}")
                        .on_click(cx.listener(|s, _, w, cx| s.advance(w, cx))),
                );
            }
        }
        if self.controls.remote_scrub {
            row = row.child(
                label(
                    if compact {
                        "A seek · B cancel"
                    } else {
                        "← → preview   Enter / A seek   Esc / B cancel"
                    },
                    13.,
                    TEXT,
                )
                .ml_2(),
            );
        } else if !p.status.ready || !p.seekable {
            row = row
                .child(label(if p.status.ready { "Live" } else { "Opening…" }, 13., MUTED).ml_2());
        } else if !compact && !p.status.paused && self.controls.speed > 0. && p.status.duration > 0.
        {
            let seconds = ((p.status.duration - p.status.position).max(0.) / self.controls.speed)
                .ceil() as i64;
            if let Some(end) = chrono::TimeDelta::try_seconds(seconds)
                .and_then(|d| chrono::Local::now().checked_add_signed(d))
            {
                row =
                    row.child(label(format!("Ends at {}", end.format("%H:%M")), 13., MUTED).ml_2());
            }
        }
        row = row.child(div().flex_1());
        if let Some(g) = self
            .controls
            .segments
            .iter()
            .find(|g| g.active(p.status.position))
        {
            let end = g.end_ticks as f64 / 10_000_000.;
            row = row.child(
                pill("skip-segment", format!("Skip {}", g.kind), None, true).on_click(cx.listener(
                    move |s, _, _, cx| {
                        if let Some(p) = &s.player {
                            s.control(Some(end - p.status.position), cx);
                        }
                    },
                )),
            );
        }
        if p.pcm_volume.is_some() {
            row = row.child(self.volume_control(compact, cx));
        }
        row =
            row.child(self.menu_button("Playback settings", "\u{e9e9}", PlayerMenu::Settings, cx));
        if self.downloads.playing.is_none()
            && let Some(source) = self.current_source()
        {
            let streams = source.media_streams.as_deref().unwrap_or_default();
            if streams.iter().any(|s| s.kind == "Subtitle") {
                row =
                    row.child(self.menu_button("Subtitles", "\u{e7f0}", PlayerMenu::Subtitles, cx));
            }
            if streams.iter().any(|s| s.kind == "Audio" && !s.is_external) {
                row =
                    row.child(self.menu_button("Audio tracks", "\u{e8d6}", PlayerMenu::Audio, cx));
            }
        }
        if !chapters.is_empty() && p.seekable {
            row = row.child(self.menu_button("Chapters", "\u{e8fd}", PlayerMenu::Chapters, cx));
        }
        row = row.child(
            transport_button(
                if w.is_fullscreen() {
                    "Exit fullscreen"
                } else {
                    "Fullscreen"
                },
                if w.is_fullscreen() {
                    "\u{e73f}"
                } else {
                    "\u{e740}"
                },
            )
            .on_click(|_, w, _| w.toggle_fullscreen()),
        );
        div()
            .w_full()
            .h(px(CONTROLS_HEIGHT))
            .occlude()
            .flex_none()
            .bg(rgb(0x08090b))
            .flex()
            .flex_col()
            .when(p.seekable, |d| {
                d.child(self.timeline(p, time, progress, cx))
            })
            .child(row)
    }

    pub(super) fn playback_screen(
        &self,
        root: Div,
        p: &Player,
        w: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let view = w.viewport_size();
        let width = f32::from(view.width);
        let height = f32::from(view.height);
        let show_bar = p.controls_visible
            || p.panel_open
            || p.audio_only
            || self.controls.remote_scrub
            || self.menu_focus.launchers.iter().any(|h| h.is_focused(w));
        let duration = p.status.duration;
        let time = self
            .scrub
            .map_or(p.status.position, |f| f64::from(f) * duration);
        let progress = if duration > 0. {
            (time / duration).clamp(0., 1.) as f32
        } else {
            0.
        };
        let overlay = p.show_timeline(
            w,
            show_bar && p.seekable && !p.audio_only,
            progress,
            &clock(time),
            &format!("−{}", clock(duration - time)),
        );
        let mut root = root.relative().when(!p.audio_only, |r| r.bg(rgb(0)));
        let mut caption_bounds = None;
        if p.audio_only {
            root = root.child(self.now_playing(p, cx));
        } else {
            root = root.child(div().flex_1());
            if p.caption_height > 0. {
                let text = self
                    .controls
                    .captions
                    .active(p.status.position - self.controls.subtitle_delay);
                if !text.is_empty() {
                    let cw = (width * 0.88).min(1140.);
                    let y = height - if show_bar { CONTROLS_HEIGHT } else { 0. } - p.caption_height;
                    let bounds = Bounds::new(
                        point(px((width - cw) * 0.5), px(y)),
                        size(px(cw), px(p.caption_height)),
                    );
                    caption_bounds = Some((bounds, 6.));
                    root = root.child(
                        floating(bounds)
                            .rounded(px(6.))
                            .bg(rgb(0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                label(text, self.prefs.subtitle_size as f32 * 0.8, TEXT)
                                    .text_center()
                                    .line_clamp(2),
                            ),
                    );
                }
            }
        }
        let mut notice_bounds = None;
        if let Some((message, shown)) = self.controls.profile_notice
            && shown.elapsed() < Duration::from_secs(3)
        {
            let bounds = Bounds::new(
                point(px(EDGE), px(EDGE)),
                size(px((width - 2. * EDGE).clamp(1., 290.)), px(38.)),
            );
            notice_bounds = Some((bounds, 8.));
            root = root.child(
                floating(bounds)
                    .rounded(px(8.))
                    .bg(rgb(PANEL))
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .child(label(message, 14., TEXT)),
            );
        }
        if p.panel_open {
            root = root.child(div().absolute().inset_0().on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, _, w, cx| {
                    s.close_player_menu(w, cx);
                }),
            ));
        }
        if show_bar {
            root = root.child(self.transport(p, w, time, progress, cx));
        }
        let mut cutout = None;
        if p.panel_open {
            let panel_width = (width * 0.4)
                .clamp(454., 700.)
                .min((width - 2. * EDGE).max(1.));
            let (menu, preferred_height) = self.playback_menu(p, w, cx);
            let panel_height = preferred_height.min((height - CONTROLS_HEIGHT - 24.).max(1.));
            let bounds = Bounds::new(
                point(
                    px(width - EDGE - panel_width),
                    px(height - CONTROLS_HEIGHT - 8. - panel_height),
                ),
                size(px(panel_width), px(panel_height)),
            );
            cutout = Some((bounds, 14.));
            root = root.child(floating(bounds).occlude().child(menu));
        } else if let Some(next) = self.credits_prompt() {
            let card_width = (width - 2. * EDGE).clamp(1., 390.);
            let card_height = 140.;
            let bottom = if show_bar { CONTROLS_HEIGHT } else { 0. } + p.caption_height + EDGE;
            let bounds = Bounds::new(
                point(
                    px(width - EDGE - card_width),
                    px((height - bottom - card_height).max(0.)),
                ),
                size(px(card_width), px(card_height)),
            );
            let auto = self.prefs.autoplay
                || self.controls.queue.is_some()
                || self.downloads.playing.is_some();
            let left = ((duration - time).max(0.) / self.controls.speed.max(0.25)).ceil() as u64;
            let heading = if !p.status.paused
                && let Some((_, at)) = self.controls.outro_deadline
                && at > time
                && at < duration
            {
                format!(
                    "Skipping credits in {}s",
                    ((at - time) / self.controls.speed.max(0.25)).ceil() as u64
                )
            } else if auto && !p.status.paused && left <= 30 {
                format!("Up next in {left}s")
            } else {
                "Up next".into()
            };
            let mut info = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(label(heading, 13., MUTED))
                .child(title(next.name.clone(), 18.).truncate());
            if let (Some(season), Some(episode)) = (next.parent_index_number, next.index_number) {
                info = info.child(label(format!("S{season} E{episode}"), 13., MUTED));
            }
            let card = floating(bounds)
                .occlude()
                .bg(rgb(PANEL))
                .rounded(px(12.))
                .p_3()
                .flex()
                .flex_col()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .when_some(self.artwork.get(next.landscape()), |d, art| {
                            d.child(
                                img(art)
                                    .w(px(96.))
                                    .h(px(54.))
                                    .rounded_md()
                                    .object_fit(ObjectFit::Cover),
                            )
                        })
                        .child(info),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            pill("credits-next", "Play next", Some(PLAY), true)
                                .track_focus(&self.menu_focus.next)
                                .on_click(cx.listener(|s, _, w, cx| s.play_from_credits(w, cx))),
                        )
                        .child(
                            pill("watch-credits", "Watch credits", None, false)
                                .track_focus(&self.menu_focus.credits)
                                .on_click(cx.listener(|s, _, w, cx| s.watch_credits(w, cx))),
                        ),
                );
            root = root.child(card);
            cutout = Some((bounds, 12.));
        } else if show_bar
            && p.seekable
            && p.status.duration > 0.
            && let Some(f) = self.scrub.or(self.controls.hover)
        {
            let art = self.artwork.trickplay.preview().or_else(|| {
                self.preview_chapter().and_then(|(i, _, _)| {
                    self.artwork
                        .get(self.playing.as_ref()?.art(ArtKind::Chapter(i)))
                })
            });
            let has_image = art.is_some();
            let (sw, sh) = art.as_ref().map_or((65., 30.), |image| {
                let pixels = image.size(0);
                let aspect = pixels.width.0.max(1) as f32 / pixels.height.0.max(1) as f32;
                let width = PREVIEW_WIDTH.min(PREVIEW_HEIGHT * aspect);
                (width, width / aspect)
            });
            let (left, length) = rail(width);
            let x = (left + f * length - sw * 0.5).clamp(EDGE, (width - EDGE - sw).max(EDGE));
            let bounds = Bounds::new(
                point(px(x), px(height - CONTROLS_HEIGHT - 6. - sh)),
                size(px(sw), px(sh)),
            );
            cutout = Some((bounds, 6.));
            root = root.child(
                floating(bounds)
                    .rounded(px(6.))
                    .overflow_hidden()
                    .bg(rgb(PANEL))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when_some(art, |d, image| {
                        d.child(
                            img(image)
                                .absolute()
                                .inset_0()
                                .size_full()
                                .object_fit(ObjectFit::Cover),
                        )
                    })
                    .child(
                        label(clock(f64::from(f) * p.status.duration), 12., TEXT)
                            .px_1()
                            .rounded_sm()
                            .bg(rgba(0x08090bdd))
                            .when(has_image, |d| d.absolute().bottom(px(4.))),
                    ),
            );
        }
        if !p.audio_only {
            let reserved = if overlay {
                CONTROLS_HEIGHT - TIMELINE_HEIGHT
            } else if show_bar {
                CONTROLS_HEIGHT
            } else {
                0.
            };
            p.overlay_cutouts(
                [cutout, caption_bounds, notice_bounds],
                reserved,
                w.scale_factor(),
            );
        }
        root
    }
}
