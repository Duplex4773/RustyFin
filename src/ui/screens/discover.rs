use super::*;
use std::collections::HashSet;

const GUTTER: f32 = 32.;
const GAP: f32 = 12.;
const CHARCOAL: u32 = 0x101413;
const GLASS_INSET: f32 = 14.;
const PIN_WASH_SPAN: f32 = 0.08;

fn wash_start(shelves: &[Shelf], row: usize) -> f32 {
    let Some(visible) = shelves.get(..=row) else {
        return 1.;
    };
    if visible
        .iter()
        .any(|shelf| shelf.kind == ShelfKind::RecentMovies)
    {
        return 1.;
    }
    match (visible[0].kind, row) {
        (ShelfKind::Continue, 0) => 0.42,
        (ShelfKind::Continue, _) | (_, 0) => 0.74,
        _ => 0.90,
    }
}

fn frosted_panel(texture: Div, tint: Rgba, center: f32, card_width: f32) -> Div {
    let mut light = Hsla::from(tint);
    light.l = 0.63;
    light.a = 0.16;
    div()
        .relative()
        .overflow_hidden()
        .rounded(px(16.))
        .bg(rgba(0x14181d26))
        .px(px(GLASS_INSET))
        .py(px(12.))
        .child(texture)
        .child(coat(112., 0xe5ebe50c, 0x11161916, 0., 1.).rounded(px(16.)))
        .child(
            div()
                .absolute()
                .left(px(center - card_width * 0.55))
                .w(px(card_width * 1.1))
                .top(px(12.))
                .bottom(px(8.))
                .rounded_full()
                .shadow(vec![BoxShadow {
                    color: light,
                    offset: Point::default(),
                    blur_radius: px(52.),
                    spread_radius: px(14.),
                }]),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(16.))
                .border_1()
                .border_color(rgba(0xe1e9e41c)),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left(px(16.))
                .right(px(16.))
                .h(px(1.))
                .bg(linear_gradient(
                    90.,
                    linear_color_stop(rgba(0xe9eee729), 0.),
                    linear_color_stop(rgba(0xe9eee700), 1.),
                )),
        )
}

fn marquee_size(w: &Window) -> Size<Pixels> {
    let width = w.viewport_size().width;
    // Keep Continue Watching in view on short windows, and let the banner grow on a TV.
    let viewport = w.viewport_size().height - px(NAV_HEIGHT);
    let extra = ((viewport - px(720.)) * 0.25).clamp(px(0.), viewport * 0.11);
    let height = (viewport * 0.62 + extra).clamp(px(286.), (width * 0.82).max(px(286.)));
    size(width, height + px(NAV_HEIGHT))
}

pub(in crate::ui) fn marquee_art(item: &Item, w: &Window) -> Option<crate::jellyfin::Art> {
    let mut art = background(item)?;
    let banner = marquee_size(w);
    let physical_width = f32::from(banner.width.min(banner.height * 2.5)) * w.scale_factor();
    art.max_width = if physical_width > 2560. {
        3840
    } else if physical_width > 1440. {
        2560
    } else {
        1440
    };
    Some(art)
}

pub(in crate::ui) fn featured_titles(shelves: &[Shelf]) -> Vec<Item> {
    let rows: Vec<_> = [
        ShelfKind::RecentMovies,
        ShelfKind::RecentTv,
        ShelfKind::Continue,
    ]
    .into_iter()
    .filter_map(|kind| shelves.iter().find(|s| s.kind == kind))
    .collect();
    let mut seen = HashSet::new();
    let mut featured = Vec::new();
    let continuing = shelves.iter().find(|s| s.kind == ShelfKind::Continue);
    for index in 0..rows.iter().map(|row| row.items.len()).max().unwrap_or(0) {
        for row in &rows {
            if let Some(item) = row.items.get(index)
                && seen.insert((&item.origin, item.series_id.as_ref().unwrap_or(&item.id)))
            {
                // The shelf opens the show, while its marquee can resume the viewer's episode.
                let mut feature = item;
                if item.kind == "Series"
                    && let Some(episode) = continuing.and_then(|row| {
                        row.items.iter().find(|ep| {
                            ep.origin == item.origin && ep.series_id.as_ref() == Some(&item.id)
                        })
                    })
                {
                    feature = episode;
                }
                featured.push(feature.clone());
                if featured.len() == 6 {
                    return featured;
                }
            }
        }
    }
    featured
}

fn hero_metadata(item: &Item) -> String {
    let kind = if matches!(item.kind.as_str(), "Episode" | "Series") {
        "TV Show"
    } else {
        &item.kind
    };
    let mut parts = vec![kind.to_owned()];
    if let Some(rating) = item.community_rating.filter(|r| *r > 0.) {
        parts.push(format!("★ {rating:.1}"));
    }
    if let Some(year) = item.production_year {
        parts.push(year.to_string());
    }
    parts.join("  ·  ")
}

fn hero_summary(item: &Item) -> String {
    let overview = item.overview.as_deref().unwrap_or_default().trim();
    if item.kind != "Episode" {
        return overview.to_owned();
    }
    let episode = match (item.parent_index_number, item.index_number) {
        (Some(s), Some(e)) => format!("S{s}, E{e} · {}", item.name),
        _ => item.name.clone(),
    };
    if overview.is_empty() {
        episode
    } else {
        format!("{episode}   —   {overview}")
    }
}

fn circle(
    id: impl Into<ElementId>,
    name: &'static str,
    code: &'static str,
    size: f32,
) -> Stateful<Div> {
    artwork_control(tip(
        div()
            .id(id)
            .size(px(size))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .hover(|d| d.bg(rgb(ACTIVE)))
            .child(icon(code, 13.)),
        name,
    ))
}

impl Library {
    fn customise_home_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        self.page_button(
            pill("customise-home", "Customise Home", Some("\u{e713}"), false).bg(rgba(0x181d1bea)),
            cx,
        )
        .on_click(cx.listener(|s, _, w, cx| {
            s.navigate(Screen::Settings(SettingsPage::Home), w, cx);
        }))
    }

    pub(in crate::ui) fn activate_home_item(
        &mut self,
        kind: ShelfKind,
        item: Item,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if kind == ShelfKind::Continue {
            self.play_item(item, w, cx);
        } else {
            self.open_item(item, w, cx);
        }
    }

    pub(super) fn marquee_image(
        &self,
        item: &Item,
        w: &Window,
    ) -> Option<std::sync::Arc<RenderImage>> {
        self.artwork
            .get(marquee_art(item, w))
            .or_else(|| self.artwork.get(background(item)))
    }

    pub(super) fn marquee_backdrop(&self, item: &Item, w: &Window, banner: Size<Pixels>) -> Div {
        let base = div().absolute().inset_0().overflow_hidden();
        let Some(image) = self.marquee_image(item, w) else {
            return base;
        };
        let source = image.size(0);
        let aspect = source.width.0 as f32 / source.height.0.max(1) as f32;
        let layer = if aspect < 1.35 {
            // A poster can be the only available image. Keep the entire design visible.
            let fit = ObjectFit::Contain.get_bounds(Bounds::new(Point::default(), banner), source);
            div()
                .absolute()
                .top(fit.origin.y)
                .right_0()
                .w(fit.size.width)
                .h(fit.size.height)
                .child(img(image).size_full().object_fit(ObjectFit::Contain))
                .child(coat(90., 0x101413ff, 0x10141300, 0., 0.18))
        } else {
            // Center the scene. Cap vertical cropping on shallow or ultrawide windows.
            let width = banner.width.min(banner.height * (aspect * 1.42));
            let margin = (banner.width - width) / 2.;
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(margin)
                .w(width)
                .overflow_hidden()
                .child(img(image).size_full().object_fit(ObjectFit::Cover))
                .when(margin > px(1.), |d| {
                    let feather = (margin / width).clamp(0.01, 0.15);
                    d.child(coat(90., 0x101413ff, 0x10141300, 0., feather))
                        .child(coat(90., 0x10141300, 0x101413ff, 1. - feather, 1.))
                })
        };
        base.child(layer.with_animation(
            SharedString::from(format!(
                "marquee-{}-{}",
                item.origin.as_deref().unwrap_or(""),
                item.id
            )),
            Animation::new(Duration::from_millis(230)),
            |d, t| d.opacity(0.35 + 0.65 * t),
        ))
    }

    fn marquee_play(&self, item: &Item, cx: &mut Context<Self>) -> Stateful<Div> {
        if !item.playable() {
            let show = item.clone();
            return pill("play-featured", "View show", Some(NEXT), true)
                .h(px(40.))
                .on_click(cx.listener(move |s, _, w, cx| s.open_item(show.clone(), w, cx)));
        }
        let position = item.position();
        let duration = item
            .run_time_ticks
            .filter(|&t| t > position && position > 0);
        let action = if position > 0 { "Resume" } else { "Play" };
        let target = item.clone();
        tip(
            div()
                .id("play-featured")
                .h(px(40.))
                .px(px(19.))
                .flex()
                .items_center()
                .gap_2()
                .rounded_full()
                .bg(rgb(0xf8f8f6))
                .text_color(rgb(0x101413))
                .cursor_pointer()
                .hover(|d| d.bg(rgb(0xe2e5df)))
                .child(icon(PLAY, 14.))
                .when_some(duration, |d, total| {
                    d.child(
                        div()
                            .w(px(36.))
                            .h(px(5.))
                            .rounded_full()
                            .bg(rgb(0xc3c6c3))
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(position as f32 / total as f32))
                                    .rounded_full()
                                    .bg(rgb(0x151916)),
                            ),
                    )
                })
                .child(
                    label(
                        duration.map_or_else(
                            || action.to_owned(),
                            |t| format!("{} min left", (t - position).div_ceil(600_000_000)),
                        ),
                        14.,
                        0x101413,
                    )
                    .font_weight(FontWeight::SEMIBOLD),
                ),
            format!("{action} {}", item.name),
        )
        .on_click(cx.listener(move |s, _, w, cx| s.play_item(target.clone(), w, cx)))
    }

    fn marquee_pagination(&self, cx: &mut Context<Self>) -> Div {
        let mut controls = div()
            .flex()
            .items_center()
            .rounded_full()
            .px(px(4.))
            .bg(rgba(ART_CONTROL_BG))
            .text_color(rgb(0xffffff))
            .child(
                glyph_button(
                    if self.rotate {
                        "Pause slideshow"
                    } else {
                        "Start slideshow"
                    },
                    if self.rotate { PAUSE } else { PLAY },
                )
                .size(px(28.))
                .on_click(cx.listener(|s, _, _, cx| {
                    s.rotate = !s.rotate;
                    s.last_slide = Instant::now();
                    cx.notify();
                })),
            );
        for (i, feature) in self.featured.iter().enumerate() {
            let active = self.hero == i;
            controls = controls.child(
                tip(
                    div()
                        .id(("featured-slide", i))
                        .h(px(28.))
                        .px(px(4.))
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .rounded_full()
                        .hover(|d| d.bg(rgba(0xffffff24)))
                        .child(
                            div()
                                .w(px(if active { 22. } else { 7. }))
                                .h(px(7.))
                                .rounded_full()
                                .bg(if active {
                                    rgba(0xfffffff2)
                                } else {
                                    rgba(0xffffffaa)
                                }),
                        ),
                    format!(
                        "Show {}",
                        feature.series_name.as_ref().unwrap_or(&feature.name)
                    ),
                )
                .on_click(cx.listener(move |s, _, _, cx| s.set_hero(i, cx))),
            );
        }
        div()
            .absolute()
            .bottom(px(2.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(controls)
    }

    fn marquee(&self, item: &Item, wash_end: f32, w: &Window, cx: &mut Context<Self>) -> Div {
        let banner = marquee_size(w);
        let width = f32::from(banner.width);
        let height = f32::from(banner.height);
        let inset = (width * 0.028).clamp(20., 40.);
        let text_width = (width - 2. * inset).min((width * 0.55).max(400.));
        let text = item.series_name.as_ref().unwrap_or(&item.name);
        let font_size = (width * 0.058)
            .clamp(33., 80.)
            .min(text_width / (text.chars().count().max(1) as f32 * 0.52))
            .max(28.);
        let wordmark = self
            .title_logo(
                item,
                (text_width * 0.82).min(400.),
                (height * 0.18).clamp(52., 138.),
            )
            .unwrap_or_else(|| {
                title(text.clone(), font_size)
                    .line_height(px(font_size * 1.06))
                    .line_clamp(2)
            });
        let toolbar = div()
            .absolute()
            .top(px(NAV_CLEARANCE))
            .left(px(20.))
            .right(px(20.))
            .flex()
            .items_center()
            .gap_2()
            .child(page_heading("Discover", 22.))
            .child(div().flex_1())
            .child(self.customise_home_button(cx))
            .child(
                artwork_control(glyph_button("Refresh", REFRESH))
                    .on_click(cx.listener(|s, _, w, cx| s.refresh(w, cx))),
            );
        let target = item.clone();
        let summary = hero_summary(item);
        let copy = div()
            .absolute()
            .left(px(inset))
            .bottom(px((height * 0.065).clamp(31., 82.)))
            .w(px(text_width))
            .flex()
            .flex_col()
            .items_start()
            .child(
                tip(
                    div()
                        .id("hero-detail")
                        .max_w_full()
                        .cursor_pointer()
                        .child(wordmark),
                    format!("View {text}"),
                )
                .on_click(cx.listener(move |s, _, w, cx| s.open_item(target.clone(), w, cx))),
            )
            .child(
                label(hero_metadata(item), 14., 0xf2f2ee)
                    .mt_2()
                    .font_weight(FontWeight::SEMIBOLD)
                    .truncate(),
            )
            .when(!summary.is_empty(), |d| {
                d.child(
                    label(summary, 15., 0xe1e3df)
                        .max_w(px(text_width.min(570.)))
                        .mt_1()
                        .line_clamp(if height < 350. { 1 } else { 2 })
                        .text_ellipsis(),
                )
            })
            .child(
                div()
                    .mt_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(self.marquee_play(item, cx))
                    .child(
                        pill("browse-marquee", "Browse library", None, false)
                            .h(px(40.))
                            .bg(rgba(0x181d1bd9))
                            .on_click(cx.listener(|s, _, w, cx| {
                                s.navigate(
                                    Screen::Library(Tab::Browse, CatalogQuery::default()),
                                    w,
                                    cx,
                                )
                            })),
                    ),
            );
        div()
            .relative()
            .w_full()
            .h(banner.height)
            .flex_none()
            .overflow_hidden()
            .bg(rgb(0x101413))
            .child(self.marquee_backdrop(item, w, banner))
            .child(coat(90., 0x070a0c65, 0x070a0c00, 0., 0.49))
            .child(coat(180., 0x07090cc4, 0x07090c00, 0., 0.16))
            .child(coat(180., 0x10141300, 0x101413ff, 0.44, 1.))
            .child(
                div()
                    .absolute()
                    .top(relative(0.42))
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .child(self.artwork.glass.wash(0., wash_end)),
            )
            .child(toolbar)
            .child(copy)
            .when(self.featured.len() > 1, |d| {
                d.child(self.marquee_pagination(cx))
            })
    }

    pub(super) fn discover(&mut self, tint: Rgba, w: &Window, cx: &mut Context<Self>) -> Div {
        self.state.settings_nav.begin_layout();
        let mut scroll = div()
            .id("discover-scroll")
            .track_scroll(&self.state.scroll)
            .overflow_y_scroll()
            .size_full()
            .pb(px(32.))
            .bg(rgb(CHARCOAL));
        scroll.style().restrict_scroll_to_axis = Some(true);
        let mut header = div().w_full();
        let pins = self.home_shortcuts(cx);
        let unavailable = self.home.unavailable();
        if let Some(item) = self.featured.get(self.hero) {
            let wash = wash_start(&self.shelves, 0);
            let wash_end = if pins.is_some() {
                wash - PIN_WASH_SPAN
            } else {
                wash
            };
            header = header.child(self.marquee(item, wash_end, w, cx));
        } else {
            header = header.child(
                div()
                    .p_5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(page_heading("Discover", 24.))
                            .child(self.customise_home_button(cx)),
                    )
                    .when(self.home.loading() && !unavailable, |d| {
                        d.child(label("Loading Home…", 15., MUTED).pt_3())
                    })
                    .when(
                        !unavailable
                            && !self.home.loading()
                            && self.home.failures().next().is_none(),
                        |d| {
                            d.child(label("No featured movies or shows yet.", 15., MUTED).py_3())
                                .child(
                                    pill("browse-home-libraries", "Browse libraries", None, false)
                                        .on_click(cx.listener(|s, _, w, cx| {
                                            s.navigate(
                                                Screen::Library(Tab::Browse, Default::default()),
                                                w,
                                                cx,
                                            )
                                        })),
                                )
                        },
                    ),
            );
        }
        if unavailable {
            let message = if self.shelves.is_empty() {
                "Couldn't load Home from the connected servers."
            } else {
                "Couldn't refresh Home. Showing previously loaded titles."
            };
            header = header.child(
                div()
                    .px(px(GUTTER))
                    .py_3()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(label(message, 15., TEXT))
                    .when(!self.home.loading(), |d| {
                        d.child(
                            pill("retry-home", "Retry", Some(REFRESH), false)
                                .on_click(cx.listener(|s, _, w, cx| s.refresh(w, cx))),
                        )
                    })
                    .when(self.home.loading(), |d| {
                        d.child(label("Retrying…", 14., MUTED))
                    })
                    .child(
                        pill("home-downloads", "Open Downloads", None, false).on_click(
                            cx.listener(|s, _, w, cx| s.navigate(Screen::Downloads, w, cx)),
                        ),
                    ),
            );
        } else {
            for (origin, host, failures) in self.home.failures() {
                let mut notice = div()
                    .px(px(GUTTER))
                    .py_2()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .child(label(format!("Couldn’t update {host}"), 14., MUTED));
                if failures.len() == crate::jellyfin::federation::HomeFeed::ALL.len() {
                    let origin = origin.to_string();
                    notice = notice.child(
                        pill(
                            SharedString::from(format!("retry-home-{origin}")),
                            "Retry server",
                            Some(REFRESH),
                            false,
                        )
                        .on_click(cx.listener(move |s, _, w, cx| {
                            if let Some(server) = s.source_server(Some(&origin)) {
                                for feed in crate::jellyfin::federation::HomeFeed::ALL {
                                    s.load_home_feed(&server, feed, w, cx);
                                }
                            }
                        })),
                    );
                } else {
                    for (feed, reason) in failures {
                        let origin = origin.to_string();
                        let button = pill(
                            SharedString::from(format!("retry-home-{origin}-{}", feed as usize)),
                            format!("Retry {}", feed.title()),
                            Some(REFRESH),
                            false,
                        );
                        notice = notice.child(tip(button, reason.to_string()).on_click(
                            cx.listener(move |s, _, w, cx| {
                                if let Some(server) = s.source_server(Some(&origin)) {
                                    s.load_home_feed(&server, feed, w, cx);
                                }
                            }),
                        ));
                    }
                }
                header = header.child(notice);
            }
        }
        if let Some(pins) = pins {
            header = header.child(pins);
        }
        scroll = scroll.child(header);
        let available = f32::from(w.viewport_size().width) - 2. * GUTTER;
        for (row, shelf) in self.shelves.iter().enumerate() {
            let frosted = shelf.kind == ShelfKind::Continue;
            let target = match shelf.kind {
                ShelfKind::Continue => 238.,
                ShelfKind::RecentTv | ShelfKind::RecentMovies => 154.,
            };
            let columns = ((available + GAP) / (target + GAP)).round().max(1.);
            let width = ((available + GAP) / columns - GAP).floor();
            let handle = &self.shelf_scroll[row];
            let mut strip = div()
                .id(("home-shelf", row))
                .track_scroll(handle)
                // Page arrows and keyboard navigation move the strip. Wheel input belongs to Discover.
                .overflow_hidden()
                .w_full()
                .flex()
                .gap(px(GAP));
            let focused =
                self.keyboard && self.focus.is_focused(w) && self.state.shelf == Some(row);
            for (i, item) in shelf.items.iter().enumerate() {
                let selected = focused && self.state.selection == i;
                let card = self.card(
                    item,
                    width,
                    CardLayout::Shelf(shelf.kind),
                    selected,
                    ("home-card", row * 100 + i).into(),
                    cx,
                );
                let card = accessibility::poster_button(
                    card,
                    item,
                    accessibility::Poster::Home { row, index: i },
                    selected,
                );
                strip = strip.child(card.when(frosted, |d| {
                    d.on_hover(cx.listener(move |s, inside, _, cx| {
                        if *inside {
                            s.shelf_light.hovered = Some(i);
                        } else if s.shelf_light.hovered == Some(i) {
                            s.shelf_light.hovered = None;
                        } else {
                            return;
                        }
                        cx.notify();
                    }))
                }));
            }
            let mut heading = div()
                .flex()
                .items_center()
                .justify_between()
                .mb(px(10.))
                .child(section_heading(shelf.kind.title(), 26.).line_height(px(30.)));
            if shelf.items.len() > columns as usize {
                let mut arrows = div().flex().gap_2();
                for (i, (name, code, dir)) in
                    [("Scroll left", PREV, 1.), ("Scroll right", NEXT, -1.)]
                        .into_iter()
                        .enumerate()
                {
                    let handle = handle.clone();
                    arrows = arrows.child(
                        circle(("home-page", row * 2 + i), name, code, 29.).on_click(
                            move |_, _, cx| {
                                let x = (handle.offset().x + px((width + GAP) * columns * dir))
                                    .clamp(-handle.max_offset().width, px(0.));
                                handle.set_offset(point(x, px(0.)));
                                cx.refresh_windows();
                            },
                        ),
                    );
                }
                heading = heading.child(arrows);
            }
            let mut section = div().relative().pt(px(if row == 0 { 16. } else { 24. }));
            let start = wash_start(&self.shelves, row);
            if start < 1. {
                let end = wash_start(&self.shelves, row + 1);
                section = section.child(self.artwork.glass.wash(start, end));
            }
            let panel = if frosted {
                let index = self
                    .shelf_light
                    .hovered
                    .or(focused.then_some(self.state.selection));
                let target = index
                    .filter(|&i| i < shelf.items.len())
                    .map_or(available * 0.24, |i| {
                        i as f32 * (width + GAP) + width / 2. + f32::from(handle.offset().x)
                    });
                let center =
                    GLASS_INSET + self.shelf_light.position(target.clamp(0., available), w);
                section = section.px(px(GUTTER - GLASS_INSET));
                frosted_panel(self.artwork.glass.frost_layer(), tint, center, width)
            } else {
                section = section.px(px(GUTTER));
                div().relative()
            };
            scroll = scroll.child(section.child(panel.child(heading).child(strip)));
        }
        let mut body = div().relative().flex_1().min_w_0().min_h_0().child(scroll);
        if let Some(removed) = &self.home_undo {
            body =
                body.child(
                    div()
                        .id("continue-undo-notice")
                        .absolute()
                        .bottom(px(20.))
                        .right(px(20.))
                        .max_w(relative(0.9))
                        .block_mouse_except_scroll()
                        .flex()
                        .items_center()
                        .gap_2()
                        .p_2()
                        .pl_3()
                        .rounded_lg()
                        .bg(rgb(0x1e2624))
                        .border_1()
                        .border_color(rgba(0xffffff2b))
                        .shadow_lg()
                        .child(
                            label(
                                format!("Removed {} from Continue Watching", removed.name),
                                14.,
                                TEXT,
                            )
                            .line_clamp(2),
                        )
                        .child(
                            pill("undo-continue-removal", "Undo", None, true)
                                .track_focus(&self.home_undo_focus)
                                .on_key_down(cx.listener(|s, event: &KeyDownEvent, w, cx| {
                                    // The key that confirms removal can be released after Undo gains focus.
                                    if !event.is_held
                                        && matches!(event.keystroke.key.as_str(), "enter" | "space")
                                        && !event.keystroke.modifiers.modified()
                                    {
                                        cx.stop_propagation();
                                        w.prevent_default();
                                        s.undo_continue(w, cx);
                                    }
                                }))
                                .on_click(cx.listener(|s, event, w, cx| {
                                    if matches!(event, ClickEvent::Mouse(_)) {
                                        s.undo_continue(w, cx);
                                    }
                                })),
                        )
                        .child(glyph_button("Dismiss notification", "\u{e711}").on_click(
                            cx.listener(|s, _, w, cx| {
                                s.focus.focus(w);
                                s.home_undo = None;
                                cx.notify();
                            }),
                        )),
                );
        }
        body
    }

    fn home_shortcuts(&self, cx: &mut Context<Self>) -> Option<Div> {
        let layout = &self.prefs.home.layout;
        let mut shortcuts = Vec::new();
        if layout.pin_favourites {
            shortcuts.push((
                "favourites".to_owned(),
                "Favourites".to_owned(),
                CatalogQuery::favourites(),
            ));
        }
        shortcuts.extend(
            self.home_pin_libraries()
                .filter(|library| layout.pinned_libraries.contains(&library.id))
                .map(|library| {
                    (
                        library.id.clone(),
                        library.name.clone(),
                        CatalogQuery {
                            kind: library.catalog_kind().unwrap_or("Folder"),
                            library: Some(library.id.clone()),
                            ..Default::default()
                        },
                    )
                }),
        );
        if shortcuts.is_empty() {
            return None;
        }
        let wash = wash_start(&self.shelves, 0);
        Some(
            div()
                .relative()
                .child(self.artwork.glass.wash(wash - PIN_WASH_SPAN, wash))
                .child(
                    div()
                        .relative()
                        .px(px(GUTTER))
                        .py_3()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            label("Pinned shortcuts", 14., TEXT).font_weight(FontWeight::SEMIBOLD),
                        )
                        .children(shortcuts.into_iter().map(|(id, name, query)| {
                            self.page_button(
                                pill(
                                    SharedString::from(format!("home-pin-{id}")),
                                    name,
                                    None,
                                    false,
                                ),
                                cx,
                            )
                            .on_click(cx.listener(
                                move |s, _, w, cx| {
                                    s.navigate(Screen::Library(Tab::Browse, query.clone()), w, cx);
                                },
                            ))
                        })),
                ),
        )
    }
}

pub(super) fn continue_menu(
    item: Item,
) -> impl Fn(gpui_component::menu::PopupMenu, WeakEntity<Library>) -> gpui_component::menu::PopupMenu
+ Clone {
    move |menu, owner| {
        let details = item.clone();
        let view = owner.clone();
        let mut menu = menu.item(
            accessibility::menu_item(match item.kind.as_str() {
                "Episode" => "Episode details",
                "Movie" => "Movie details",
                _ => "Title details",
            })
            .on_click(move |_, w, cx| {
                cx.stop_propagation();
                let _ = view.update(cx, |s, cx| s.open_item(details.clone(), w, cx));
            }),
        );
        if item.position() > 0 || item.copies.iter().any(|copy| copy.position() > 0) {
            let target = item.clone();
            let view = owner.clone();
            menu = menu.item(accessibility::menu_item("Play from beginning").on_click(
                move |_, w, cx| {
                    cx.stop_propagation();
                    let _ = view.update(cx, |s, cx| {
                        s.play_item_at(target.clone(), Some(0.), w, cx);
                    });
                },
            ));
        }
        if let Some(id) = &item.series_id {
            let id = id.clone();
            let origin = item.origin.clone();
            let view = owner.clone();
            menu = menu.item(
                accessibility::menu_item("Go to series").on_click(move |_, w, cx| {
                    cx.stop_propagation();
                    let _ = view.update(cx, |s, cx| {
                        let Some(server) = s.source_server(origin.as_deref()) else {
                            return;
                        };
                        let id = id.clone();
                        let origin = origin.clone();
                        s.work(
                            w,
                            cx,
                            move || server.item(&id),
                            move |s, mut series, w, cx| {
                                series.origin = origin;
                                s.open_item(series, w, cx);
                                Ok(())
                            },
                        );
                    });
                }),
            );
        }
        let target = item.clone();
        let dismissed = item.clone();
        let view = owner.clone();
        menu.item(
            accessibility::menu_item("Mark watched").on_click(move |_, w, cx| {
                cx.stop_propagation();
                let _ = view.update(cx, |s, cx| {
                    s.set_item_flag(target.clone(), false, true, w, cx)
                });
            }),
        )
        .item(
            accessibility::menu_item("Remove from Continue Watching").on_click(move |_, w, cx| {
                cx.stop_propagation();
                let _ = owner.update(cx, |s, cx| s.dismiss_continue(&dismissed, w, cx));
            }),
        )
    }
}

impl Library {
    pub(in crate::ui) fn focused_continue_actions(
        &mut self,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((row, shelf)) = self
            .state
            .shelf
            .and_then(|r| Some((r, self.shelves.get(r)?)))
        else {
            return false;
        };
        if !matches!(self.state.screen, Screen::Discover) || shelf.kind != ShelfKind::Continue {
            return false;
        }
        let Some(item) = shelf.items.get(self.state.selection).cloned() else {
            return false;
        };
        self.show_popup(
            ("home-card", row * 100 + self.state.selection).into(),
            continue_menu(item),
            w,
            cx,
        );
        true
    }
}
