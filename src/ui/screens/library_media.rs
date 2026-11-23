use super::*;
use std::sync::atomic::Ordering;
impl Library {
    pub(super) fn download_view(&self, cx: &mut Context<Self>) -> Div {
        let mut body = div()
            .id("download-list")
            .track_scroll(&self.state.scroll)
            .size_full()
            .overflow_y_scroll()
            .p_5()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .when(
                        self.history.last().is_some_and(|entry| {
                            matches!(entry.screen, Screen::Settings(SettingsPage::Overview))
                        }),
                        |d| {
                            d.child(
                                glyph_button("Back to settings", BACK)
                                    .on_click(cx.listener(|s, _, w, cx| s.back(w, cx))),
                            )
                        },
                    )
                    .child(title("Downloads", 26.)),
            );
        if !self.downloads.notice.is_empty() {
            body = body.child(label(self.downloads.notice.clone(), 14., TEXT).py_3());
        }
        if let Some(profile) = &self.downloads.profile {
            let mut any = false;
            for (i, d) in self.downloads.records.iter().enumerate() {
                if d.removed(profile) {
                    continue;
                }
                any = true;
                let active = self
                    .downloads
                    .active
                    .as_ref()
                    .filter(|(id, _)| *id == d.source.id);
                let ready = d.ready(profile);
                let paused = d.paused(profile);
                let status = if ready {
                    format!("Available offline · {}", d.format)
                } else if let Some((_, transfer)) = active {
                    let done = transfer.received.load(Ordering::Relaxed);
                    let total = transfer.total.load(Ordering::Relaxed);
                    if total > 0 {
                        format!(
                            "{:.1}% · {} MiB",
                            100. * done as f64 / total as f64,
                            done / 1_048_576
                        )
                    } else {
                        format!("Downloaded {} MiB", done / 1_048_576)
                    }
                } else if paused {
                    "Paused".into()
                } else {
                    "Queued".into()
                };
                let mut row = div()
                    .mt_2()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(SURFACE))
                    .flex()
                    .items_center()
                    .gap_2()
                    .when_some(
                        Some(d.dir(profile).join("cover.jpg")).filter(|p| p.is_file()),
                        |d, path| {
                            d.child(
                                img(path)
                                    .w(px(54.))
                                    .h(px(68.))
                                    .rounded_md()
                                    .object_fit(ObjectFit::Cover),
                            )
                        },
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(title(d.item.name.clone(), 16.).truncate())
                                    .children(super::format_strip::strip(
                                        &crate::jellyfin::formats::Formats::from_streams(
                                            d.source.media_streams.as_deref().unwrap_or_default(),
                                            d.item.kind == "Audio",
                                        ),
                                        true,
                                    )),
                            )
                            .child(label(status, 13., MUTED)),
                    );
                let target = d.clone();
                if ready {
                    row = row.child(
                        pill(("offline-play", i), "Play", Some(PLAY), true).on_click(
                            cx.listener(move |s, _, w, cx| s.play_download(target.clone(), w, cx)),
                        ),
                    );
                } else {
                    row = row.child(
                        pill(
                            ("download-pause", i),
                            if paused { "Resume" } else { "Pause" },
                            None,
                            false,
                        )
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.toggle_download(&target, !paused, w, cx)
                        })),
                    );
                }
                if ready && (d.item.series_id.is_some() || d.item.album_id.is_some()) {
                    let group = d.clone();
                    row = row.child(
                        pill(
                            ("play-local-group", i),
                            if group.item.album_id.is_some() {
                                "Play album"
                            } else {
                                "Play series"
                            },
                            None,
                            false,
                        )
                        .on_click(
                            cx.listener(move |s, _, w, cx| s.queue_offline_group(&group, w, cx)),
                        ),
                    );
                }
                if active.is_none() {
                    let target = d.clone();
                    row = row.child(
                        pill(("remove-local", i), "Remove download", None, false).on_click(
                            cx.listener(move |s, _, _, cx| {
                                if let Some(p) = &s.downloads.profile
                                    && let Err(e) = target.remove_local(p)
                                {
                                    s.downloads.notice = e.to_string();
                                }
                                cx.notify();
                            }),
                        ),
                    );
                }
                body = body.child(row);
            }
            if !any {
                body = body
                    .child(self.empty("Choose Download original on a title to watch it offline."));
            }
        }
        div().flex_1().min_w_0().min_h_0().child(body)
    }
}
