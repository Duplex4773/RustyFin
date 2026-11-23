use super::*;
impl Library {
    pub(super) fn now_playing(&self, _: &Player, cx: &mut Context<Self>) -> Div {
        let mut view = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .p_5()
            .gap_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(title("Now Playing", 23.))
                    .child(
                        pill("minimize-music", "Back to library", Some(BACK), false).on_click(
                            cx.listener(|s, _, _, cx| {
                                s.controls.mini = true;
                                cx.notify();
                            }),
                        ),
                    ),
            );
        if let Some(item) = &self.playing {
            let mut art = div()
                .size(px(180.))
                .flex_none()
                .rounded_lg()
                .bg(rgb(SURFACE))
                .overflow_hidden();
            let local = self
                .downloads
                .playing
                .as_ref()
                .zip(self.downloads.profile.as_ref())
                .map(|(d, p)| d.dir(p).join("cover.jpg"))
                .filter(|p| p.is_file());
            if let Some(path) = local {
                art = art.child(
                    img(path)
                        .rounded_lg()
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                );
            } else if let Some(image) = self.artwork.get(item.art(ArtKind::Poster)) {
                art = art.child(
                    img(image)
                        .rounded_lg()
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                );
            }
            view = view.child(
                div().flex().items_center().gap_5().child(art).child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(title(item.name.clone(), 29.).line_clamp(2))
                        .child(label(item.caption(), 15., MUTED)),
                ),
            );
        }
        if !self.controls.lyrics.is_empty() {
            let mut lyrics = div()
                .id("lyrics")
                .track_scroll(&self.controls.lyric_scroll)
                .flex_1()
                .min_h_0()
                .overflow_y_scroll();
            for (i, line) in self.controls.lyrics.iter().enumerate() {
                let start = line.start;
                lyrics = lyrics.child(
                    div()
                        .id(("lyric", i))
                        .py_1()
                        .when(start.is_some(), |d| d.cursor_pointer())
                        .child(label(
                            line.text.clone(),
                            20.,
                            if self.controls.lyric_line == Some(i) {
                                TEXT
                            } else {
                                MUTED
                            },
                        ))
                        .on_click(cx.listener(move |s, _, _, cx| {
                            if let (Some(at), Some(p)) = (start, &s.player) {
                                s.control(Some(at as f64 / 10_000_000. - p.status.position), cx);
                            }
                        })),
                );
            }
            view = view.child(lyrics);
        }
        view
    }
    pub(super) fn mini_player(&self, p: &Player, cx: &mut Context<Self>) -> Div {
        let item = self.playing.as_ref();
        let mut row = div()
            .h(px(62.))
            .px_4()
            .flex_none()
            .bg(rgb(SURFACE))
            .flex()
            .items_center()
            .gap_3();
        if let Some(art) = item.and_then(|i| self.artwork.get(i.art(ArtKind::Poster))) {
            row = row.child(
                img(art)
                    .rounded_md()
                    .size(px(46.))
                    .object_fit(ObjectFit::Cover),
            );
        }
        row.child(
            div()
                .id("open-now-playing")
                .flex_1()
                .min_w_0()
                .cursor_pointer()
                .child(title(item.map_or("", |i| &i.name).to_owned(), 16.).truncate())
                .child(label(
                    format!(
                        "{} / {}",
                        clock(p.status.position),
                        clock(p.status.duration)
                    ),
                    13.,
                    MUTED,
                ))
                .on_click(cx.listener(|s, _, _, cx| {
                    s.controls.mini = false;
                    cx.notify();
                })),
        )
        .child(
            glyph_button(
                if p.status.paused { "Play" } else { "Pause" },
                if p.status.paused { PLAY } else { PAUSE },
            )
            .on_click(cx.listener(|s, _, _, cx| s.control(None, cx))),
        )
        .when(
            self.controls.queue.as_ref().is_some_and(|q| !q.is_empty()),
            |r| {
                r.child(
                    glyph_button("Next track", NEXT)
                        .on_click(cx.listener(|s, _, w, cx| s.advance(w, cx))),
                )
            },
        )
        .child(
            glyph_button("Stop music", "\u{e71a}").on_click(cx.listener(|s, _, _, cx| {
                s.stop();
                cx.notify();
            })),
        )
    }
}
