use super::*;
use std::collections::HashSet;

const GUTTER: f32 = 32.;
const GAP: f32 = 12.;

fn marquee_size(w: &Window) -> Size<Pixels> {
    let width = w.viewport_size().width - px(RAIL);
    // Keep Continue Watching in view on short windows, and let the banner grow on a TV.
    let viewport = w.viewport_size().height;
    let extra = ((viewport - px(720.)) * 0.25).clamp(px(0.), viewport * 0.11);
    let height = (viewport * 0.62 + extra).clamp(px(286.), (width * 0.82).max(px(286.)));
    size(width, height)
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
    tip(
        div()
            .id(id)
            .size(px(size))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(rgba(0xb3c0b17a))
            .bg(rgba(0x11161155))
            .cursor_pointer()
            .hover(|d| d.bg(rgb(ACTIVE)))
            .child(icon(code, 13.)),
        name,
    )
}

impl Library {
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

    fn marquee_backdrop(&self, item: &Item, w: &Window, banner: Size<Pixels>) -> Div {
        let base = div().absolute().inset_0().overflow_hidden();
        let Some(image) = self
            .artwork
            .get(marquee_art(item, w))
            .or_else(|| self.artwork.get(background(item)))
        else {
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
            .absolute()
            .bottom(px(2.))
            .left_0()
            .right_0()
            .flex()
            .items_center()
            .justify_center()
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
                                    rgba(0xffffff64)
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
        controls
    }

    fn marquee(&self, item: &Item, w: &Window, cx: &mut Context<Self>) -> Div {
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
            .top(px(12.))
            .left(px(20.))
            .right(px(20.))
            .flex()
            .items_center()
            .gap_2()
            .child(title("Discover", 22.))
            .child(div().flex_1())
            .child(
                glyph_button("Refresh", REFRESH)
                    .on_click(cx.listener(|s, _, w, cx| s.refresh(w, cx))),
            )
            .child(
                glyph_button("Account", ACCOUNT).on_click(cx.listener(|s, _, w, cx| {
                    s.navigate(Screen::Settings(SettingsPage::Overview), w, cx)
                })),
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
            .child(toolbar)
            .child(copy)
            .when(self.featured.len() > 1, |d| {
                d.child(self.marquee_pagination(cx))
            })
    }

    pub(super) fn discover(&self, w: &Window, cx: &mut Context<Self>) -> Div {
        let mut scroll = div()
            .id("discover-scroll")
            .track_scroll(&self.state.scroll)
            .overflow_y_scroll()
            .size_full()
            .pb(px(32.))
            .bg(rgb(0x101413));
        scroll.style().restrict_scroll_to_axis = Some(true);
        if let Some(item) = self.featured.get(self.hero) {
            scroll = scroll.child(self.marquee(item, w, cx));
        } else {
            scroll = scroll.child(
                div()
                    .p_5()
                    .child(title("Discover", 24.))
                    .child(self.empty("Your library has no titles yet."))
                    .child(
                        glyph_button("Refresh", REFRESH)
                            .on_click(cx.listener(|s, _, w, cx| s.refresh(w, cx))),
                    ),
            );
        }
        let available = f32::from(w.viewport_size().width) - RAIL - 2. * GUTTER;
        for (row, shelf) in self.shelves.iter().enumerate() {
            let target = if shelf.kind == ShelfKind::RecentMovies {
                154.
            } else {
                238.
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
            for (i, item) in shelf.items.iter().enumerate() {
                strip = strip.child(self.card(
                    item,
                    width,
                    CardLayout::Shelf(shelf.kind),
                    self.keyboard && self.state.shelf == Some(row) && self.state.selection == i,
                    ("home-card", row * 100 + i).into(),
                    cx,
                ));
            }
            let mut heading = div()
                .flex()
                .items_center()
                .justify_between()
                .mb(px(10.))
                .child(title(shelf.kind.title(), 26.).line_height(px(30.)));
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
            scroll = scroll.child(
                div()
                    .px(px(GUTTER))
                    .pt(px(if row == 0 { 20. } else { 24. }))
                    .child(heading)
                    .child(strip),
            );
        }
        div().flex_1().min_w_0().min_h_0().child(scroll)
    }
}
