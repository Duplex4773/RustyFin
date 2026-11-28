use super::*;
use crate::jellyfin::Art;
use std::{cell::Cell, rc::Rc};

pub(in crate::ui) fn preview_art(item: &Item, screen: &Screen, w: &Window) -> Option<Art> {
    if item.art(ArtKind::Backdrop).is_some() {
        if screen.spotlight() {
            marquee_art(item, w)
        } else {
            item.art(ArtKind::Backdrop)
        }
    } else if item.kind == "Episode" {
        item.art(ArtKind::Thumb)
    } else {
        None
    }
}

fn metadata(item: &Item) -> String {
    let kind = match item.kind.as_str() {
        "Series" => "TV series",
        "BoxSet" => "Collection",
        "MusicAlbum" => "Album",
        "MusicArtist" => "Artist",
        "MusicVideo" => "Music video",
        "Audio" => "Song",
        kind => kind,
    };
    let mut parts = vec![kind.to_owned()];
    if item.kind == "Episode" {
        parts.push(item.caption());
    } else if let Some(year) = item.production_year {
        parts.push(year.to_string());
    }
    if let Some(genres) = &item.genres {
        parts.extend(genres.iter().take(2).cloned());
    }
    parts.retain(|part| !part.is_empty());
    parts.join("   ·   ")
}

impl Library {
    pub(super) fn catalog_preview_visible(&self) -> bool {
        self.state.screen.catalog_preview()
            && !self.filters_open
            && self.state.wall.total.unwrap_or(0) > 0
    }

    pub(super) fn catalog_preview_panel(&self, w: &Window, cx: &mut Context<Self>) -> Option<Div> {
        if !self.catalog_preview_visible() {
            return None;
        }
        let screen = &self.state.screen;
        let spot = screen.spotlight();
        let width = f32::from(w.viewport_size().width);
        let viewport_height = f32::from(w.viewport_size().height);
        let short = viewport_height < 600.;
        let height = if spot {
            (viewport_height * 0.37).clamp(if short { 184. } else { 246. }, 610.)
        } else {
            (viewport_height * 0.18).clamp(if short { 91. } else { 122. }, 184.)
        };
        let base = if spot { BG } else { SURFACE };
        let inset = if spot { CONTENT_GUTTER } else { 16. };
        let mut panel = div()
            .relative()
            .flex_none()
            .h(px(height))
            .overflow_hidden()
            .bg(rgb(base))
            .when(!spot, |d| {
                d.mx(px(CONTENT_GUTTER))
                    .mt_2()
                    .mb_2()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgba(REST_EDGE))
                    .shadow(vec![panel_shadow()])
            });
        let Some(item) = self.state.wall.item(self.state.selection) else {
            return Some(panel);
        };
        let art = self.artwork.get(preview_art(item, screen, w));
        let portrait = art.is_none();
        let poster = CatalogGrid::for_wall(w, screen, self.prefs.poster_size).poster;
        if let Some(image) = art.or_else(|| self.artwork.get(item.art(poster))) {
            let src = image.size(0);
            let aspect = src.width.0 as f32 / src.height.0.max(1) as f32;
            let image_width = if portrait {
                (height * 0.84 * aspect).min(width * 0.38)
            } else {
                (width * if spot { 0.90 } else { 0.62 })
                    .min(height * aspect * if spot { 1.36 } else { 1.75 })
            };
            let layer = div()
                .absolute()
                .right_0()
                .top_0()
                .bottom_0()
                .w(px(image_width))
                .when(portrait, |d| {
                    d.right(px(width * 0.13))
                        .top(px(height * 0.10))
                        .bottom(px(height * 0.06))
                })
                .child(img(image).size_full().object_fit(if portrait {
                    ObjectFit::Contain
                } else {
                    ObjectFit::Cover
                }))
                .child(coat(
                    90.,
                    (base << 8) | 255,
                    base << 8,
                    0.,
                    if portrait { 0.16 } else { 0.39 },
                ));
            panel = if scrolling::animations_enabled() {
                panel.child(layer.with_animation(
                    SharedString::from(format!(
                        "catalog-art-{}-{}-{portrait}",
                        item.origin.as_deref().unwrap_or(""),
                        item.id
                    )),
                    Animation::new(Duration::from_millis(200)),
                    |d, t| d.opacity(0.25 + t * 0.75),
                ))
            } else {
                panel.child(layer)
            };
        }
        let title_width = (width * 0.43).clamp(235., 520.);
        let name = &item.name;
        let heading = if spot {
            let logo = self.title_logo(item, title_width, (height * 0.235).clamp(36., 118.));
            accessibility::heading(
                logo.unwrap_or_else(|| {
                    title(name.clone(), (height * 0.145).clamp(24., 56.)).truncate()
                }),
                name.clone().into(),
                2,
            )
        } else {
            section_heading(
                name.clone(),
                if short {
                    18.
                } else {
                    (width * 0.019).clamp(23., 30.)
                },
            )
            .truncate()
        };
        let summary = item.overview.as_deref().unwrap_or("").trim();
        let meta = metadata(item);
        let action = named_pill(
            "catalog-preview-details",
            "Details",
            None,
            PillStyle::Primary,
            format!("Details for {name}"),
        )
        .h(px(if short { 27. } else { 32. }))
        .on_click(cx.listener(|s, _, w, cx| {
            if let Some(item) = s.state.wall.item(s.state.selection).cloned() {
                s.open_item(item, w, cx);
            }
        }));
        let action = accessibility::keyboard_focus(action, cx);
        let copy = div()
            .min_w_0()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(if short { 3. } else { 5. }))
            .child(heading)
            .when(!meta.is_empty(), |d| {
                d.child(label(meta, 12., META).truncate())
            })
            .when(!summary.is_empty() && !short, |d| {
                d.child(
                    label(summary.to_owned(), 14., BODY)
                        .line_height(px(19.))
                        .line_clamp(if spot && height >= 290. { 2 } else { 1 }),
                )
            })
            .child(action);
        Some(
            panel
                .when(spot, |d| {
                    d.child(self.artwork.glass.hero_light()).child(hero_scrim())
                })
                .when(!spot, |d| {
                    d.child(coat(90., (base << 8) | 230, base << 8, 0.18, 0.72))
                })
                .child(
                    div()
                        .absolute()
                        .left(px(inset))
                        .bottom(px(if short {
                            8.
                        } else if spot {
                            17.
                        } else {
                            12.
                        }))
                        .w(px((width * 0.56).clamp(300., 780.)))
                        .child(copy),
                ),
        )
    }
}

// Both ends use GPUI's measured bounds in window coordinates, including the
// current scroll offset and poster lift. Keep only geometry in navigation history.
pub(in crate::ui) struct CatalogCover {
    pub(in crate::ui) kind: ArtKind,
    flight: Option<(Instant, Bounds<Pixels>)>,
    destination: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl CatalogCover {
    fn frame(&self) -> Option<(Bounds<Pixels>, f32)> {
        let (start, bounds) = self.flight?;
        let t = start.elapsed().as_secs_f32() / 0.24;
        (t < 1.).then(|| (bounds, 1. - (1. - t).powi(3)))
    }
}

impl Library {
    pub(in crate::ui) fn catalog_cover_for(&self, item: &Item, w: &Window) -> Option<CatalogCover> {
        if !self.catalog_preview_visible() {
            return None;
        }
        let kind = CatalogGrid::for_wall(w, &self.state.screen, self.prefs.poster_size).poster;
        item.art(kind)?;
        let wall = &self.state.wall;
        let flight = wall
            .selected_bounds
            .get()
            .filter(|(i, _)| {
                wall.visible.contains(i)
                    && wall
                        .item(*i)
                        .is_some_and(|s| s.id == item.id && s.origin == item.origin)
            })
            .filter(|_| {
                scrolling::animations_enabled() && self.artwork.get(item.art(kind)).is_some()
            })
            .map(|(_, bounds)| (Instant::now(), bounds));
        Some(CatalogCover {
            kind,
            flight,
            destination: Default::default(),
        })
    }

    pub(super) fn detail_catalog_poster(&self, item: &Item, width: f32) -> Option<Div> {
        let cover = self.state.catalog_cover.as_ref()?;
        let image = self.artwork.get(item.art(cover.kind))?;
        let destination = cover.destination.clone();
        Some(
            div()
                .relative()
                .flex_none()
                .w(px(width))
                .h(catalog_poster::image_height(item, width))
                .when(cover.frame().is_none(), |d| {
                    d.child(
                        img(image)
                            .size_full()
                            .rounded_lg()
                            .object_fit(ObjectFit::Cover),
                    )
                })
                .child(
                    canvas(
                        move |bounds, _, _| destination.set(Some(bounds)),
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0()
                    .size_full(),
                ),
        )
    }

    pub(super) fn catalog_cover_overlay(&self, w: &Window) -> Option<impl IntoElement> {
        let Screen::Detail(item, _) = &self.state.screen else {
            return None;
        };
        let cover = self.state.catalog_cover.as_ref()?;
        let (from, t) = cover.frame()?;
        let dest = cover.destination.clone();
        let image = self.artwork.get(item.art(cover.kind))?;
        w.request_animation_frame();
        Some(
            canvas(
                |_, _, _| (),
                move |_, _, w, _| {
                    if let Some(to) = dest.get() {
                        let bounds = Bounds::new(
                            from.origin + (to.origin - from.origin) * t,
                            size(
                                from.size.width + (to.size.width - from.size.width) * t,
                                from.size.height + (to.size.height - from.size.height) * t,
                            ),
                        );
                        w.with_content_mask(Some(ContentMask { bounds }), |w| {
                            let fit = ObjectFit::Cover.get_bounds(bounds, image.size(0));
                            let _ = w.paint_image(fit, px(8.).into(), image, 0, false);
                        });
                    }
                },
            )
            .absolute()
            .inset_0()
            .size_full(),
        )
    }
}
