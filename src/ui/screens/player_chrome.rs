use super::*;
use crate::ui::playback::{EpisodeDirection, PlayerMenu};
mod episode_transport;
mod performance_overlay;

pub(super) const PANEL: u32 = 0x15161b;
const EDGE: f32 = 14.;
const CONTROLS_HEIGHT: f32 = 112.;
const SEEK_HIT_HEIGHT: f32 = 22.;
const CLOCK_HEIGHT: f32 = 20.;
const GLASS: u32 = 0x12161aac;
pub(in crate::ui) const PREVIEW_WIDTH: f32 = 196.;
const PREVIEW_HEIGHT: f32 = PREVIEW_WIDTH * 9. / 16.;

fn rail(width: f32) -> (f32, f32) {
    let start = (width * 0.03).clamp(EDGE, 56.);
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
        .text_color(rgb(0xffffff))
        .hover(|d| d.bg(rgba(0xffffff2a)))
}

fn capsule() -> Div {
    div()
        .occlude()
        .flex_none()
        .flex()
        .items_center()
        .px(px(5.))
        .h(px(42.))
        .rounded_full()
        .bg(rgba(GLASS))
        .border_1()
        .border_color(rgba(0xffffff50))
}

// Outline the glyphs so the picture remains visible behind the text.
fn outlined_text(text: String, style: impl Fn(Div) -> Div) -> Div {
    let text: SharedString = text.into();
    let mut view = div().relative().min_w_0().max_w_full();
    for (x, y) in [(-1., 0.), (1., 0.), (0., -1.), (0., 1.)] {
        view = view.child(style(
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w_full()
                .text_color(rgba(0x080a0cc0))
                .child(text.clone()),
        ));
    }
    view.child(style(div().text_color(rgb(0xffffff)).child(text)))
}

fn overlay_text(text: String, size: f32, weight: FontWeight) -> Div {
    outlined_text(text, |d| d.truncate())
        .font_weight(weight)
        .text_size(px(size))
        .line_height(px(size + 5.))
}

impl Library {
    pub(in crate::ui) fn step_player_focus(
        &mut self,
        forward: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(p) = &mut self.player else { return };
        p.use_keyboard_controls();
        if p.panel_open {
            self.move_menu_focus(forward, w, cx);
            return;
        }
        if self.controls.remote_scrub {
            return;
        }
        if self.focus.is_focused(w) || w.focused(cx).is_none() {
            // On an idle player there are no painted tab stops yet. The handles
            // reconnect to the buttons in the first frame after the reveal.
            if forward {
                self.menu_focus.play_pause.focus(w);
            } else {
                self.menu_focus.fullscreen.focus(w);
            }
        } else if forward {
            w.focus_next();
        } else {
            w.focus_prev();
        }
        cx.notify();
    }

    pub(in crate::ui) fn toggle_player_menu(
        &mut self,
        page: PlayerMenu,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(p) = &mut self.player else { return };
        if p.panel_open
            && (self.controls.menu == page
                || page == PlayerMenu::Episodes && self.controls.menu.episode_drawer())
        {
            self.close_player_menu(w, cx);
        } else {
            let already_open = p.panel_open;
            p.panel_open = true;
            p.wake_controls();
            self.end_preview();
            self.menu_focus.origin = page;
            self.menu_focus.settings_row = PlayerMenu::Speed;
            if self.controls.menu.episode_drawer() {
                self.controls.episodes = Default::default();
            }
            self.enter_player_menu(page, cx);
            if !already_open {
                self.controls.menu_page_motion = None;
            }
            if page == PlayerMenu::Episodes {
                self.load_episode_drawer(w, cx);
            }
        }
    }

    pub(in crate::ui) fn enter_player_menu(&mut self, page: PlayerMenu, cx: &mut Context<Self>) {
        if self.controls.menu != page {
            self.menu_focus.scroll.set_offset(Point::default());
            let back = page == PlayerMenu::Settings
                || (page == PlayerMenu::Episodes
                    && self.controls.menu == PlayerMenu::EpisodeSeasons);
            self.controls.menu_page_motion = scrolling::animations_enabled()
                .then(|| (Instant::now(), if back { -1. } else { 1. }));
        }
        self.controls.menu = page;
        self.menu_focus.entering.set(true);
        cx.notify();
    }

    fn close_player_menu(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let keyboard = self.player.as_mut().is_some_and(|p| {
            p.panel_open = false;
            p.keyboard_controls()
        });
        self.controls.menu_page_motion = None;
        if self.controls.menu.episode_drawer() {
            self.controls.episodes = Default::default();
        }
        self.menu_focus.entering.set(false);
        if keyboard {
            self.menu_focus.launcher(self.menu_focus.origin).focus(w);
        } else {
            self.focus.focus(w);
        }
        cx.notify();
    }

    pub(in crate::ui) fn back_player_menu(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.controls.menu == PlayerMenu::EpisodeSeasons {
            self.enter_player_menu(PlayerMenu::Episodes, cx);
        } else if self.controls.menu == PlayerMenu::Settings
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
        self.menu_focus.reveal(
            w,
            cx,
            !self.player_menu_notice(self.controls.menu).is_empty(),
            false,
        );
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
        self.controls.episode_hover = None;
    }

    fn finish_scrub(&mut self, event: &MouseUpEvent, w: &mut Window, cx: &mut Context<Self>) {
        if !self.controls.remote_scrub
            && self.scrub.take().is_some()
            && let Some(p) = &self.player
        {
            let at = f64::from(fraction_at(event.position.x, w)) * p.status.duration;
            self.seek_to(at, cx);
        }
    }

    pub(in crate::ui) fn seek_preview_fraction(&self, p: &Player, w: &Window) -> Option<f32> {
        if !p.seekable || p.status.duration <= 0. || p.panel_open {
            return None;
        }
        self.scrub.or(self.controls.hover).or_else(|| {
            (p.keyboard_controls() && self.menu_focus.timeline.is_focused(w))
                .then(|| (p.status.position / p.status.duration).clamp(0., 1.) as f32)
        })
    }

    fn timeline(&self, p: &Player, w: &Window, cx: &mut Context<Self>) -> Div {
        let time = p.status.position;
        let duration = p.status.duration;
        let progress = if duration > 0. {
            (time / duration).clamp(0., 1.) as f32
        } else {
            0.
        };
        let preview = self.seek_preview_fraction(p, w);
        let thickness = if preview.is_some() { 5. } else { 3. };
        let chapters = self
            .playing
            .iter()
            .flat_map(|item| item.chapters.iter().flatten());
        let mut remaining = overlay_text(
            format!("−{}", clock(duration - time)),
            14.,
            FontWeight::MEDIUM,
        )
        .id("player-time-remaining");
        if !p.status.paused && self.controls.speed > 0. {
            let seconds = ((duration - time).max(0.) / self.controls.speed).ceil() as i64;
            if let Some(end) = chrono::TimeDelta::try_seconds(seconds)
                .and_then(|d| chrono::Local::now().checked_add_signed(d))
            {
                let message = format!("Ends at {}", end.format("%H:%M"));
                remaining =
                    remaining.tooltip(move |w, cx| Tooltip::new(message.clone()).build(w, cx));
            }
        }
        let track = div()
            .id("timeline")
            .occlude()
            .track_focus(&self.menu_focus.timeline)
            .tab_stop(true)
            .w_full()
            .h(px(SEEK_HIT_HEIGHT))
            .flex()
            .items_center()
            .cursor_pointer()
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(thickness))
                    .rounded_full()
                    .bg(rgba(0xffffff70))
                    .shadow(vec![BoxShadow {
                        color: rgba(0x05080ab0).into(),
                        offset: Point::default(),
                        blur_radius: px(2.),
                        spread_radius: px(1.),
                    }])
                    .child(
                        div()
                            .h_full()
                            .w(relative(progress))
                            .rounded_full()
                            .bg(rgb(LIME)),
                    )
                    .children(chapters.filter_map(|chapter| {
                        let at = chapter.start_position_ticks as f64 / 10_000_000.;
                        (at > 0. && at < duration).then(|| {
                            div()
                                .absolute()
                                .left(relative((at / duration) as f32))
                                .ml(px(-2.))
                                .top(px((thickness - 11.) / 2.))
                                .w(px(4.))
                                .h(px(11.))
                                .rounded_full()
                                .bg(rgba(0xffffffe0))
                                .border_1()
                                .border_color(rgba(0x0a0e12cf))
                        })
                    }))
                    .child(
                        div()
                            .absolute()
                            .left(relative(progress))
                            .ml(px(-4.5))
                            .top(px((thickness - 9.) / 2.))
                            .size(px(9.))
                            .rounded_full()
                            .bg(rgb(0xffffff))
                            .border_1()
                            .border_color(rgba(0x0a0e122e)),
                    )
                    .when_some(preview, |d, fraction| {
                        d.child(
                            div()
                                .absolute()
                                .left(relative(fraction))
                                .top(px((thickness - 13.) / 2.))
                                .w(px(2.))
                                .h(px(13.))
                                .rounded_full()
                                .bg(rgb(0xffffff)),
                        )
                    }),
            )
            .on_click(cx.listener(|s, e: &ClickEvent, w, cx| {
                if e.mouse_position().is_none() {
                    s.start_preview(w, cx);
                }
            }))
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
            .flex_none()
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .h(px(CLOCK_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(overlay_text(clock(time), 14., FontWeight::MEDIUM))
                    .child(remaining),
            )
            .child(accessibility::timeline(
                track,
                duration,
                time,
                &self.menu_focus.timeline,
            ))
    }

    fn volume_control(&self, w: &Window, p: &Player, cx: &mut Context<Self>) -> Stateful<Div> {
        let value = self.volume;
        let bounds = std::rc::Rc::new(std::cell::Cell::new(Bounds::<Pixels>::default()));
        let measured = bounds.clone();
        let slider = div()
            .id("volume-slider")
            .relative()
            .w(px(if w.viewport_size().width < px(820.) {
                56.
            } else {
                82.
            }))
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
            .id("player-volume")
            .flex()
            .items_center()
            .when(
                accessibility::active()
                    || self.controls.volume_hovered
                    || (p.keyboard_controls() && self.menu_focus.volume.is_focused(w)),
                |d| {
                    d.pl_2().gap_1().child(accessibility::volume(
                        slider,
                        value,
                        &self.menu_focus.volume,
                    ))
                },
            )
            .child(
                accessibility::focus(
                    transport_button(
                        if value == 0. { "Unmute" } else { "Mute" },
                        if value == 0. { "\u{e74f}" } else { "\u{e767}" },
                    ),
                    &self.menu_focus.volume,
                )
                .on_click(cx.listener(|s, _, _, cx| s.volume(None, cx))),
            )
            .on_hover(cx.listener(|s, inside, _, cx| {
                if s.controls.volume_hovered != *inside {
                    s.controls.volume_hovered = *inside;
                    cx.notify();
                }
            }))
    }

    fn menu_button(
        &self,
        name: &'static str,
        glyph: &'static str,
        page: PlayerMenu,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        accessibility::focus(
            transport_button(name, glyph),
            self.menu_focus.launcher(page),
        )
        .when(
            self.player.as_ref().is_some_and(|p| p.panel_open) && self.controls.menu == page,
            |d| d.text_color(rgb(LIME)),
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

    fn exit_player(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        transport_button("Back to title", BACK)
            .occlude()
            .on_click(cx.listener(|s, _, _, cx| {
                s.stop();
                cx.notify();
            }))
    }

    fn playback_title(&self, p: &Player, width: f32, cx: &mut Context<Self>) -> Div {
        let edge = rail(width).0;
        let mut view = div()
            .absolute()
            .top(px(EDGE))
            .left(px(edge))
            .max_w(px((width - 2. * edge).min(700.).max(1.)))
            .flex()
            .items_start()
            .gap(px(9.))
            .child(self.exit_player(cx).bg(rgba(0x10141790)));
        if let Some(item) = &self.playing {
            let series = item.series_name.as_ref().filter(|s| !s.is_empty());
            let name = series.unwrap_or(&item.name);
            let detail = match (series, item.parent_index_number, item.index_number) {
                (Some(_), Some(season), Some(episode)) => {
                    format!("S{season} E{episode} · {}", item.name)
                }
                (Some(_), _, _) => item.name.clone(),
                _ => String::new(),
            };
            let mut info = div()
                .min_w_0()
                .flex()
                .flex_col()
                .items_start()
                .child(overlay_text(name.clone(), 21., FontWeight::SEMIBOLD));
            if !detail.is_empty() {
                info = info.child(overlay_text(detail, 14., FontWeight::MEDIUM));
            }
            let status = if self.controls.remote_scrub {
                Some("← → preview   Enter / A seek   Esc / B cancel")
            } else if !p.status.ready {
                Some("Opening…")
            } else if !p.seekable {
                Some("Live")
            } else {
                None
            };
            if let Some(status) = status {
                info = info.child(overlay_text(status.to_owned(), 13., FontWeight::MEDIUM));
            }
            view = view.child(info);
        }
        view
    }

    fn transport(&self, p: &Player, w: &Window, cx: &mut Context<Self>) -> Div {
        let width = f32::from(w.viewport_size().width);
        let mut playback = capsule();
        if p.audio_only {
            playback = playback.child(self.exit_player(cx));
        }
        let episodes = !p.audio_only && self.has_episode_drawer();
        if episodes {
            playback = playback.child(self.episode_button(EpisodeDirection::Previous, cx));
        }
        if p.seekable {
            playback = playback.child(self.skip_button(false, cx));
        }
        playback = playback.child(
            accessibility::focus(
                transport_button(
                    if p.status.paused { "Play" } else { "Pause" },
                    if p.status.paused { "\u{f5b0}" } else { PAUSE },
                ),
                &self.menu_focus.play_pause,
            )
            .on_click(cx.listener(|s, _, _, cx| s.control(None, cx))),
        );
        if p.seekable {
            playback = playback.child(self.skip_button(true, cx));
        }
        if episodes {
            playback = playback.child(self.episode_button(EpisodeDirection::Next, cx));
        }
        if p.audio_only && self.next_title().is_some() {
            playback = playback.child(
                transport_button("Next title", "\u{e893}")
                    .on_click(cx.listener(|s, _, w, cx| s.advance(w, cx))),
            );
        }

        let mut utility = capsule();
        if p.pcm_volume.is_some() {
            utility = utility.child(self.volume_control(w, p, cx));
        }
        if self.downloads.playing.is_none()
            && let Some(source) = self.current_source()
        {
            let streams = source.media_streams.as_deref().unwrap_or_default();
            if streams.iter().any(|s| s.kind == "Subtitle") {
                utility = utility.child(self.menu_button(
                    "Subtitles",
                    "\u{e7f0}",
                    PlayerMenu::Subtitles,
                    cx,
                ));
            }
            if streams.iter().any(|s| s.kind == "Audio" && !s.is_external) {
                utility = utility.child(self.menu_button(
                    "Audio tracks",
                    "\u{e8d6}",
                    PlayerMenu::Audio,
                    cx,
                ));
            }
        }
        if !p.audio_only && self.has_episode_drawer() {
            utility = utility.child(
                self.menu_button("Episodes", "\u{e8fd}", PlayerMenu::Episodes, cx)
                    .w_auto()
                    .px_2()
                    .gap_1()
                    .child(label("Episodes", 13., TEXT)),
            );
        }
        utility = utility
            .child(self.menu_button("Playback settings", "\u{e9e9}", PlayerMenu::Settings, cx))
            .child(
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
                .track_focus(&self.menu_focus.fullscreen)
                .on_click(|_, w, _| w.toggle_fullscreen()),
            );
        let mut row = div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(playback)
            .child(div().flex_1());
        if p.seekable
            && let Some(g) = self.controls.segments.iter().find(|g| {
                !matches!(g.kind.as_str(), "Intro" | "Outro") && g.active(p.status.position)
            })
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
        div()
            .w_full()
            .h(px(CONTROLS_HEIGHT))
            .flex_none()
            .px(px(rail(width).0))
            .pb(px(EDGE))
            .flex()
            .flex_col()
            .justify_end()
            .gap(px(6.))
            .when(p.seekable && p.status.duration > 0., |d| {
                d.child(self.timeline(p, w, cx))
            })
            .child(row.child(utility))
    }

    fn segment_skip_button(&self, p: &Player, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        if !p.status.ready || !p.seekable || p.audio_only || p.panel_open || self.scrub.is_some() {
            return None;
        }
        let segment = self.controls.segments.iter().find(|g| {
            (g.kind == "Intro" || g.kind == "Outro" && !self.controls.watching_credits)
                && g.active(p.status.position)
        })?;
        let end = (segment.end_ticks as f64 / 10_000_000.).min(p.status.duration);
        let start = segment.start_ticks;
        if end <= p.status.position {
            return None;
        }
        let label = if segment.kind == "Intro" {
            "Skip intro"
        } else {
            "Skip credits"
        };
        Some(
            pill("skip-intro-or-credits", label, Some(NEXT), false)
                .occlude()
                .bg(rgba(0x171c22af))
                .border_1()
                .border_color(rgba(0xffffff60))
                .hover(|d| d.bg(rgba(0x262b33dc)))
                .on_click(cx.listener(move |s, _, _, cx| {
                    if let Some(player) = &s.player
                        && end > player.status.position
                    {
                        s.controls.skipped = Some(start);
                        s.seek_to(end, cx);
                    }
                })),
        )
    }

    pub(super) fn playback_screen(
        &self,
        root: Div,
        p: &Player,
        w: &Window,
        chrome_opacity: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let view = w.viewport_size();
        let width = f32::from(view.width);
        let height = f32::from(view.height);
        let show_bar = chrome_opacity > 0.;
        let duration = p.status.duration;
        let time = p.status.position;
        let view = cx.weak_entity();
        let performance_paints = p.performance.enabled.then(|| p.performance.paints.clone());
        let mut root = root.relative().child(
            canvas(
                |_, _, _| (),
                move |_, _, w, _| {
                    if let Some(paints) = &performance_paints {
                        paints.set(paints.get().saturating_add(1));
                    }
                    w.on_mouse_event(move |event: &MouseDownEvent, phase, w, cx| {
                        let size = w.viewport_size();
                        // GPUI also dispatches mouse events from the Windows title bar and resize frame.
                        if phase == DispatchPhase::Capture
                            && event.button == MouseButton::Left
                            && event.position.x >= px(0.)
                            && event.position.y >= px(0.)
                            && event.position.x < size.width
                            && event.position.y < size.height
                        {
                            let _ = view.update(cx, |s, cx| {
                                if let Some(player) = &mut s.player {
                                    player.use_pointer_controls();
                                    if player.panel_open {
                                        s.menu_focus.scope.focus(w);
                                    } else {
                                        s.focus.focus(w);
                                    }
                                    // Let the control click run without assigning mouse focus.
                                    w.prevent_default();
                                    cx.notify();
                                }
                            });
                        }
                    });
                },
            )
            .absolute()
            .size(px(0.)),
        );
        if p.audio_only {
            root = root.bg(rgb(BG)).child(self.now_playing(p, cx));
        } else {
            // Keep the client area opaque until there is a picture beneath GPUI's overlay.
            root = root.when(!p.status.video_presented, |d| d.bg(rgb(0)));
            root = root.child(div().flex_1()).child(
                div()
                    .id("video-surface")
                    .absolute()
                    .inset_0()
                    .when(!show_bar, |d| {
                        d.children(accessibility::show_playback_controls())
                    })
                    .on_click(|event: &ClickEvent, w, _| {
                        if event.click_count() == 2 {
                            w.toggle_fullscreen();
                        }
                    }),
            );
            if show_bar {
                root = root.child(self.playback_title(p, width, cx).opacity(chrome_opacity));
            }
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
                    root = root.child(
                        floating(bounds)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                outlined_text(text, |d| d.text_center().line_clamp(2))
                                    .w_full()
                                    .text_size(px(self.prefs.subtitle_size as f32 * 0.8)),
                            ),
                    );
                }
            }
        }
        if let Some((message, shown)) = self.controls.profile_notice
            && shown.elapsed() < Duration::from_secs(5)
        {
            let (title_text, detail) = message
                .split_once('\n')
                .map_or((message, None), |(name, detail)| (name, Some(detail)));
            let bounds = Bounds::new(
                point(
                    px(rail(width).0),
                    px(if show_bar && !p.audio_only { 85. } else { EDGE }),
                ),
                size(
                    px((width - 2. * EDGE).clamp(1., if detail.is_some() { 360. } else { 290. })),
                    px(if detail.is_some() { 58. } else { 38. }),
                ),
            );
            root = root.child(
                floating(bounds)
                    .rounded(px(8.))
                    .bg(rgba(0x11161bef))
                    .flex()
                    .flex_col()
                    .justify_center()
                    .px_3()
                    .overflow_hidden()
                    .child(title(title_text, 14.).line_clamp(1))
                    .when_some(detail, |d, text| {
                        d.child(label(text, 12., 0xc8cdce).mt(px(2.)).line_clamp(2))
                    }),
            );
        }
        if p.performance.enabled {
            root = root.child(self.performance_overlay(p, width));
        }
        let credits = self.credits_prompt();
        let mut skip = self.segment_skip_button(p, cx);
        if credits.is_none()
            && self.seek_preview_fraction(p, w).is_none()
            && let Some(skip) = skip.take()
        {
            let bottom = CONTROLS_HEIGHT + p.caption_height + EDGE;
            root = root.child(
                div()
                    .absolute()
                    .right(px(rail(width).0))
                    .bottom(px(bottom))
                    .child(skip),
            );
        }
        if p.panel_open {
            root = root.child(div().absolute().inset_0().occlude().on_mouse_down(
                MouseButton::Left,
                cx.listener(|s, _, w, cx| {
                    s.close_player_menu(w, cx);
                }),
            ));
        }
        if show_bar {
            root = root.child(self.transport(p, w, cx).opacity(chrome_opacity));
            if !p.panel_open
                && let Some(preview) = self.episode_preview(width)
            {
                root = root.child(preview.opacity(chrome_opacity));
            }
        }
        if p.panel_open {
            let edge = rail(width).0;
            let episodes = self.controls.menu.episode_drawer();
            let panel_width = match self.controls.menu {
                PlayerMenu::Info => (width * 0.56).clamp(510., 690.),
                _ if episodes => (width * 0.48).clamp(380., 496.),
                _ => (width * 0.35).clamp(380., 460.),
            }
            .min((width - 2. * edge).max(1.));
            let (menu, preferred_height) = self.playback_menu(p, w, cx);
            let panel_height = preferred_height.min((height - CONTROLS_HEIGHT - 24.).max(1.));
            let bounds = Bounds::new(
                point(
                    px(width - edge - panel_width),
                    px(height - CONTROLS_HEIGHT - 8. - panel_height),
                ),
                size(px(panel_width), px(panel_height)),
            );
            root = root.child(floating(bounds).child(reveal_menu(
                div().size_full().occlude().child(menu),
                ("playback-menu-reveal", self.generation as usize),
                if episodes {
                    point(px(22.), px(0.))
                } else {
                    point(px(0.), px(10.))
                },
            )));
        } else if let Some(next) = credits {
            let card_width = (width - 2. * EDGE).clamp(1., 390.);
            let card_height = 140.;
            let bottom = CONTROLS_HEIGHT + p.caption_height + EDGE;
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
                        .children(skip)
                        .child(
                            pill("watch-credits", "Watch credits", None, false)
                                .track_focus(&self.menu_focus.credits)
                                .on_click(cx.listener(|s, _, w, cx| s.watch_credits(w, cx))),
                        ),
                );
            root = root.child(card);
        } else if show_bar && let Some(f) = self.seek_preview_fraction(p, w) {
            let chapter = self.preview_chapter(w);
            let art = self.artwork.trickplay.preview().or_else(|| {
                chapter.and_then(|(i, _, _)| {
                    self.artwork
                        .get(self.playing.as_ref()?.art(ArtKind::Chapter(i)))
                })
            });
            let timestamp = clock(f64::from(f) * p.status.duration);
            let caption = chapter.map_or(timestamp.clone(), |(i, chapter, _)| {
                let name = chapter
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("Chapter {}", i + 1));
                format!("{timestamp} · {name}")
            });
            let has_image = art.is_some();
            let preview_width = if chapter.is_some() {
                PREVIEW_WIDTH
            } else {
                65.
            };
            let (sw, sh) = art.as_ref().map_or((preview_width, 30.), |image| {
                let pixels = image.size(0);
                let aspect = pixels.width.0.max(1) as f32 / pixels.height.0.max(1) as f32;
                let width = PREVIEW_WIDTH.min(PREVIEW_HEIGHT * aspect);
                (width, width / aspect)
            });
            let (left, length) = rail(width);
            let x = (left + f * length - sw * 0.5).clamp(EDGE, (width - EDGE - sw).max(EDGE));
            // Keep an edge-clamped preview clear of the elapsed/remaining clocks.
            let near_clock = x < left + 72. || x + sw > width - left - 72.;
            let bottom =
                EDGE + 42. + 6. + SEEK_HIT_HEIGHT + if near_clock { CLOCK_HEIGHT + 5. } else { 6. };
            let bounds = Bounds::new(point(px(x), px(height - bottom - sh)), size(px(sw), px(sh)));
            root = root.child(
                floating(bounds)
                    .opacity(chrome_opacity)
                    .rounded(px(7.))
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
                        div()
                            .absolute()
                            .inset_0()
                            .rounded(px(7.))
                            .border_1()
                            .border_color(rgba(0xffffff85)),
                    )
                    .child(
                        label(caption, 12., TEXT)
                            .max_w(px((sw - 6.).max(1.)))
                            .truncate()
                            .px_1()
                            .rounded_sm()
                            .bg(rgba(0x08090bdd))
                            .when(has_image, |d| d.absolute().bottom(px(4.))),
                    ),
            );
        }
        root
    }
}
