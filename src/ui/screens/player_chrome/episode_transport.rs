use super::*;

impl EpisodeDirection {
    fn label(self) -> &'static str {
        match self {
            Self::Previous => "Previous episode",
            Self::Next => "Next episode",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Self::Previous => "\u{e892}",
            Self::Next => "\u{e893}",
        }
    }
}

impl Library {
    pub(super) fn episode_button(
        &self,
        direction: EpisodeDirection,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let enabled = self.episode_target(direction).is_some() && !self.busy;
        let button = div()
            .id(direction.label())
            .size(px(OSD_TARGET))
            .rounded_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(if enabled { TEXT } else { MUTED }))
            .child(icon(direction.glyph(), OSD_ICON));
        if !enabled {
            return button.opacity(0.48);
        }
        accessibility::button(button, direction.label(), None, false)
            .focusable()
            .tab_stop(true)
            .cursor_pointer()
            .border_1()
            .border_color(rgba(0))
            .hover(|d| d.bg(rgba(0xffffff2a)))
            .focus(|d| d.border_color(rgb(TEXT)))
            .on_hover(cx.listener(move |s, inside, _, cx| {
                if *inside {
                    s.controls.episode_hover = Some((direction, Instant::now()));
                } else if s
                    .controls
                    .episode_hover
                    .is_some_and(|(hover, _)| hover == direction)
                {
                    s.controls.episode_hover = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |s, _, w, cx| s.navigate_episode(direction, w, cx)))
    }

    pub(super) fn episode_preview(&self, width: f32) -> Option<Div> {
        let (direction, since) = self.controls.episode_hover?;
        if since.elapsed() < Duration::from_millis(1200) || self.busy {
            return None;
        }
        let item = self.episode_target(direction)?;
        let image = self.artwork.get(item.landscape()).map(img).or_else(|| {
            self.downloads.playing.as_ref()?;
            let profile = self.downloads.profile.as_ref()?;
            let record = self
                .downloads
                .records
                .iter()
                .find(|d| d.item.id == item.id)?;
            let path = record.dir(profile).join("cover.jpg");
            path.is_file().then(|| img(path))
        });
        let code = item
            .parent_index_number
            .zip(item.index_number)
            .map(|(s, e)| format!("S{s} E{e}"));
        let duration = item
            .run_time_ticks
            .filter(|n| *n > 0)
            .map(|n| format!("{} min", n.div_ceil(600_000_000)));
        let info = [code, duration]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
        Some(
            div()
                .absolute()
                .left(px(rail(width).0))
                .bottom(px(CONTROLS_HEIGHT + EDGE))
                .w(px((width - 2. * EDGE).min(385.)))
                .flex()
                .items_center()
                .gap_3()
                .p_2()
                .rounded(px(12.))
                .border_1()
                .border_color(rgba(0xffffff45))
                .bg(rgba(0x11161beb))
                .child(
                    div()
                        .w(px(120.))
                        .h(px(68.))
                        .flex_none()
                        .rounded_md()
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(SURFACE))
                        .map(|d| match image {
                            Some(image) => d.child(image.size_full().object_fit(ObjectFit::Cover)),
                            None => d.child(icon(VIDEO, 20.).text_color(rgb(MUTED))),
                        }),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(label(direction.label(), 12., BODY))
                        .child(title(item.name.clone(), 16.).truncate())
                        .when(!info.is_empty(), |d| d.child(label(info, 12., MUTED)))
                        .when_some(
                            item.overview.as_ref().filter(|s| !s.trim().is_empty()),
                            |d, overview| {
                                d.child(label(overview.clone(), 12., MUTED).line_clamp(2))
                            },
                        ),
                ),
        )
    }
}
