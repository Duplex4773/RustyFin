mod anime;
mod catalog;
mod discover;
pub(super) use discover::{featured_titles, marquee_art};
mod editor;
mod explore;
mod format_strip;
mod library_media;
mod live;
mod music;
mod navigation;
mod options;
mod settings;
pub(super) use settings::SettingsPage;
mod player_chrome;
pub(super) use player_chrome::PREVIEW_WIDTH;
mod player_menu;
use super::artwork::background;
use super::*;
use crate::{jellyfin::ArtKind, player::CONTROLS_HEIGHT};
use gpui_component::tooltip::Tooltip;

const BG: u32 = 0x0e0f12;
const SURFACE: u32 = 0x15171c;
const ACTIVE: u32 = 0x2d2f35;
const TEXT: u32 = 0xededed;
const MUTED: u32 = 0xa0a1a6;
const RAIL: f32 = 72.;
const WIDE_RAIL: f32 = 220.;
const RAIL_DURATION: Duration = Duration::from_millis(180);

#[derive(Default)]
pub(super) struct RailMotion {
    expanded: bool,
    from: f32,
    changed_at: Option<Instant>,
}

impl RailMotion {
    fn phase(&self, now: Instant) -> f32 {
        let elapsed = self.changed_at.map_or(0., |start| {
            now.duration_since(start).as_secs_f32() / RAIL_DURATION.as_secs_f32()
        });
        (self.from + if self.expanded { elapsed } else { -elapsed }).clamp(0., 1.)
    }

    pub(super) fn set_expanded(&mut self, expanded: bool) {
        if self.expanded != expanded {
            let now = Instant::now();
            self.from = self.phase(now);
            self.changed_at = Some(now);
            self.expanded = expanded;
        }
    }
}
const PAD: f32 = 16.;
const LIME: u32 = 0xdafe67;

const SEARCH: &str = "\u{e721}";
const PLAY: &str = "\u{e768}";
const PAUSE: &str = "\u{e769}";
const BACK: &str = "\u{e72b}";
const NEXT: &str = "\u{e76c}";
const PREV: &str = "\u{e76b}";
const ACCOUNT: &str = "\u{e77b}";
const REFRESH: &str = "\u{e72c}";
const FILTER: &str = "\u{e71c}";
const SORT: &str = "\u{e8cb}";
const VIDEO: &str = "\u{e714}";

fn icon(code: &'static str, size: f32) -> Div {
    div()
        .flex_none()
        .font_family("Segoe MDL2 Assets")
        .text_size(px(size))
        .line_height(px(size + 2.))
        .child(code)
}
fn label(text: impl Into<SharedString>, size: f32, color: u32) -> Div {
    div()
        .text_size(px(size))
        .text_color(rgb(color))
        .child(text.into())
}
fn title(text: impl Into<SharedString>, size: f32) -> Div {
    label(text, size, TEXT).font_weight(FontWeight::SEMIBOLD)
}
fn tip(button: Stateful<Div>, text: impl Into<SharedString>) -> Stateful<Div> {
    let text = text.into();
    button
        .focusable()
        .tab_stop(true)
        .focus(|d| d.border_1().border_color(rgb(TEXT)))
        .tooltip(move |w, cx| Tooltip::new(text.clone()).build(w, cx))
}
fn glyph_button(id: &'static str, code: &'static str) -> Stateful<Div> {
    icon_button(id, icon(code, 17.))
}
fn icon_button(id: &'static str, glyph: Div) -> Stateful<Div> {
    tip(
        div()
            .id(id)
            .size(px(36.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .hover(|d| d.bg(rgb(ACTIVE)))
            .child(glyph),
        id,
    )
}
fn pill(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    code: Option<&'static str>,
    active: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .focusable()
        .tab_stop(true)
        .focus(|d| d.border_2().border_color(rgb(MUTED)))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .h(px(34.))
        .px_4()
        .rounded_full()
        .cursor_pointer()
        .bg(rgb(if active { TEXT } else { SURFACE }))
        .text_color(rgb(if active { BG } else { TEXT }))
        .text_size(px(13.))
        .font_weight(FontWeight::SEMIBOLD)
        .hover(move |d| d.bg(rgb(if active { 0xffffff } else { ACTIVE })))
        .when_some(code, |d, code| d.child(icon(code, 13.)))
        .child(text.into())
}
fn episode_action(item: &Item) -> String {
    let action = if item.position() > 0 {
        "Resume"
    } else if item.user_data.as_ref().and_then(|d| d.played) == Some(true) {
        "Replay"
    } else {
        "Play"
    };
    let label = match (item.parent_index_number, item.index_number) {
        (Some(s), Some(e)) => format!("{action} S{s} E{e}"),
        _ => format!("{action} {}", item.name),
    };
    if item.position() > 0
        && let Some(duration) = item.run_time_ticks.filter(|t| *t > item.position())
    {
        return format!(
            "{label} · {} min left",
            (duration - item.position()).div_ceil(600_000_000)
        );
    }
    label
}

fn clock(t: f64) -> String {
    let s = t.max(0.) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}
fn coat(angle: f32, from: u32, to: u32, start: f32, end: f32) -> Div {
    div().absolute().inset_0().bg(linear_gradient(
        angle,
        linear_color_stop(rgba(from), start),
        linear_color_stop(rgba(to), end),
    ))
}
const CATALOG_GAP: f32 = 6.;

#[derive(Clone, Copy)]
pub(super) struct CatalogGrid {
    pub(super) columns: usize,
    card_width: f32,
    pub(super) poster: ArtKind,
}

impl CatalogGrid {
    fn new(viewport_width: f32, scale: f32) -> Self {
        let available = (viewport_width - RAIL - 2. * PAD).max(1.);
        let columns = ((available + CATALOG_GAP) / (192. + CATALOG_GAP))
            .round()
            .clamp(1., 15.) as usize;
        let card_width =
            ((available - CATALOG_GAP * (columns - 1) as f32) / columns as f32).floor();
        Self {
            columns,
            card_width,
            poster: if card_width * scale > 320. {
                ArtKind::LargePoster
            } else {
                ArtKind::Poster
            },
        }
    }

    pub(super) fn for_window(w: &Window) -> Self {
        Self::new(w.viewport_size().width.into(), w.scale_factor())
    }
}

#[derive(Clone, Copy)]
enum CardLayout {
    Shelf(ShelfKind),
    Catalog { poster: ArtKind },
}

impl Library {
    fn card(
        &self,
        item: &Item,
        width: f32,
        layout: CardLayout,
        selected: bool,
        id: ElementId,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let wide = matches!(
            layout,
            CardLayout::Shelf(ShelfKind::Continue | ShelfKind::RecentTv)
        );
        let continued = matches!(layout, CardLayout::Shelf(ShelfKind::Continue));
        let shelf = matches!(layout, CardLayout::Shelf(_));
        let key = match layout {
            CardLayout::Shelf(kind) => kind.art(item),
            CardLayout::Catalog { poster } => item.art(poster),
        };
        let height = (width
            * if wide {
                9. / 16.
            } else if matches!(item.kind.as_str(), "MusicAlbum" | "MusicArtist" | "Audio") {
                1.
            } else {
                1.5
            })
        .round();
        let mut picture = div()
            .w_full()
            .h(px(height))
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .rounded(px(8.))
            .bg(rgb(0x25272c));
        if let Some(art) = self.artwork.get(key) {
            picture = picture.child(
                img(art)
                    .rounded(px(8.))
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        } else {
            picture = picture.child(icon(VIDEO, 23.).text_color(rgb(0x55575f)));
        }
        if continued {
            let status = if item.position() > 0 {
                item.run_time_ticks
                    .filter(|t| *t > item.position())
                    .map(|t| format!("{} min left", (t - item.position()).div_ceil(600_000_000)))
                    .unwrap_or_else(|| "Resume".into())
            } else {
                "Next episode".into()
            };
            picture = picture
                .child(coat(180., 0x00000000, 0x050907ad, 0.54, 1.))
                .child(
                    div()
                        .absolute()
                        .left(px(9.))
                        .bottom(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .rounded_full()
                        .bg(rgba(0x101513df))
                        .px(px(9.))
                        .py(px(3.))
                        .child(
                            icon(if item.position() > 0 { PLAY } else { NEXT }, 10.)
                                .text_color(rgb(LIME)),
                        )
                        .child(title(status, 12.)),
                );
        }
        if (!shelf || continued)
            && let Some(progress) = item
                .run_time_ticks
                .filter(|v| *v > 0)
                .map(|d| item.position() as f32 / d as f32)
            && progress > 0.
        {
            picture = picture.child(
                div()
                    .absolute()
                    .bottom(px(if continued { 0. } else { 4. }))
                    .left(px(if continued { 0. } else { 6. }))
                    .right(px(if continued { 0. } else { 6. }))
                    .h(px(3.))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(if continued {
                        rgba(0xdde4d45c)
                    } else {
                        rgba(0x000000aa)
                    })
                    .child(
                        div()
                            .h_full()
                            .w(relative(progress.clamp(0., 1.)))
                            .bg(rgb(if continued { LIME } else { TEXT })),
                    ),
            );
        }
        if !shelf && item.user_data.as_ref().and_then(|u| u.played) == Some(true) {
            picture = picture.child(
                div()
                    .absolute()
                    .top(px(5.))
                    .right(px(5.))
                    .rounded_full()
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgb(TEXT))
                    .text_color(rgb(BG))
                    .child(icon("\u{e73e}", 11.)),
            );
        }
        let outline = if shelf { LIME } else { TEXT };
        picture = picture.child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(8.))
                .border_2()
                .border_color(if selected { rgb(outline) } else { rgba(0) })
                .group_hover("media-card", move |s| s.border_color(rgb(outline))),
        );
        if continued {
            let info = item.clone();
            picture = picture.child(
                div()
                    .id("continue-details")
                    .absolute()
                    .right(px(8.))
                    .top(px(8.))
                    .size(px(28.))
                    .rounded_full()
                    .bg(rgba(0x101513df))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child(icon("\u{e946}", 15.))
                    .focusable()
                    .tab_stop(true)
                    .tooltip(|w, cx| Tooltip::new("Title details").build(w, cx))
                    .when(!selected, |d| {
                        d.opacity(0.).group_hover("media-card", |d| d.opacity(1.))
                    })
                    .focus(|d| d.opacity(1.).border_1().border_color(rgb(LIME)))
                    .on_click(cx.listener(move |s, _, w, cx| {
                        cx.stop_propagation();
                        s.open_item(info.clone(), w, cx);
                    })),
            );
        }
        if let Some(strip) = self.card_format_strip(item, &id, width, cx) {
            picture = picture.child(strip);
        }
        let primary = if wide {
            item.series_name.as_ref().unwrap_or(&item.name)
        } else {
            &item.name
        };
        let secondary = if wide && item.series_name.is_some() {
            match (item.parent_index_number, item.index_number) {
                (Some(s), Some(e)) => format!("S{s:02} E{e:02}  ·  {}", item.name),
                _ => item.name.clone(),
            }
        } else if item.kind == "Episode" {
            item.caption()
        } else {
            item.production_year
                .map(|y| y.to_string())
                .unwrap_or_default()
        };
        let target = item.clone();
        let name = title(primary.clone(), if shelf { 16. } else { 13. }).truncate();
        let (name, detail) = match layout {
            CardLayout::Shelf(_) => (
                name.line_height(px(20.)),
                label(secondary, 13., 0xa7afa9).line_height(px(18.)),
            ),
            CardLayout::Catalog { .. } => (
                name.line_height(px(15.)),
                label(secondary, 11., MUTED).line_height(px(12.)),
            ),
        };
        div()
            .id(id)
            .group("media-card")
            .w(px(width))
            .min_w_0()
            .flex_none()
            .cursor_pointer()
            .child(picture)
            .child(name.mt(px(if shelf { 7. } else { 4. })))
            .child(detail.truncate())
            .on_click(cx.listener(move |s, _, w, cx| match layout {
                CardLayout::Shelf(kind) => s.activate_home_item(kind, target.clone(), w, cx),
                CardLayout::Catalog { .. } => s.open_item(target.clone(), w, cx),
            }))
    }

    fn backdrop(&self, item: &Item) -> Div {
        let mut layer = div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .bg(rgb(SURFACE));
        if let Some(image) = self.artwork.get(background(item)) {
            layer = layer.child(
                img(image)
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        }
        layer
            .child(coat(90., 0x08090c80, 0x08090c00, 0., 0.9))
            .child(coat(180., 0x0e0f1200, 0x0e0f12ff, 0.43, 1.))
    }

    fn title_logo(&self, item: &Item, width: f32, height: f32) -> Option<Div> {
        let logo = self.artwork.get(item.art(ArtKind::Logo))?;
        let fitted = ObjectFit::Contain.get_bounds(
            Bounds::new(Point::default(), size(px(width), px(height))),
            logo.size(0),
        );
        Some(
            div().child(
                img(logo)
                    .w(fitted.size.width)
                    .h(fitted.size.height)
                    .object_fit(ObjectFit::Contain),
            ),
        )
    }

    fn wordmark(&self, item: &Item, width: f32, height: f32) -> Div {
        self.title_logo(item, width, height).unwrap_or_else(|| {
            title(
                item.series_name
                    .clone()
                    .unwrap_or_else(|| item.name.clone()),
                (width / 7.).clamp(30., 48.),
            )
            .line_height(px(49.))
            .line_clamp(2)
            .max_w(px(width * 1.65))
        })
    }

    fn play_button(&self, item: &Item, cx: &mut Context<Self>) -> Stateful<Div> {
        let text = if self.busy {
            "Opening…".into()
        } else if item.position() > 0 {
            format!("Resume  ·  {}", clock(item.position() as f64 / 10_000_000.))
        } else {
            "Play".into()
        };
        let target = item.clone();
        pill("play-title", text, Some(PLAY), true)
            .h(px(42.))
            .px_5()
            .on_click(cx.listener(move |s, _, w, cx| s.play_item(target.clone(), w, cx)))
    }

    fn empty(&self, message: &'static str) -> Div {
        label(
            if self.busy {
                "Loading…"
            } else if self.message.is_empty() {
                message
            } else {
                ""
            },
            15.,
            MUTED,
        )
        .py_5()
    }

    fn paging(&self, cx: &mut Context<Self>) -> Div {
        let mut row = div().flex().items_center().justify_center().gap_4().py_3();
        if let Some(page) = &self.state.page
            && page.total_record_count > self.state.screen.page_size()
        {
            if self.state.offset > 0 {
                row = row.child(
                    glyph_button("Previous page", PREV)
                        .on_click(cx.listener(|s, _, w, cx| s.turn_page(false, w, cx))),
                );
            }
            row = row.child(label(
                format!(
                    "{}–{} of {}",
                    self.state.offset + 1,
                    self.state.offset + page.items.len(),
                    page.total_record_count
                ),
                13.,
                MUTED,
            ));
            if self.state.offset + page.items.len() < page.total_record_count {
                row = row.child(
                    glyph_button("Next page", NEXT)
                        .on_click(cx.listener(|s, _, w, cx| s.turn_page(true, w, cx))),
                );
            }
        }
        row
    }

    fn grid(&self, w: &Window, cx: &mut Context<Self>, empty: &'static str) -> Stateful<Div> {
        let layout = CatalogGrid::for_window(w);
        let mut grid = div()
            .id("catalog-grid")
            .flex_1()
            .min_h_0()
            .w_full()
            .px(px(PAD))
            .track_scroll(&self.state.scroll)
            .overflow_y_scroll();
        if let Some(page) = &self.state.page
            && !page.items.is_empty()
        {
            for (row, chunk) in page.items.chunks(layout.columns).enumerate() {
                let mut line = div().flex().gap(px(CATALOG_GAP)).pb(px(2.));
                for (col, item) in chunk.iter().enumerate() {
                    let index = row * layout.columns + col;
                    line = line.child(self.card(
                        item,
                        layout.card_width,
                        CardLayout::Catalog {
                            poster: layout.poster,
                        },
                        self.keyboard && self.state.selection == index,
                        ("poster", index).into(),
                        cx,
                    ));
                }
                grid = grid.child(line);
            }
            grid = grid.child(self.paging(cx));
        } else {
            grid = grid.child(self.empty(empty));
        }
        grid
    }

    fn change_filter(
        &mut self,
        edit: impl FnOnce(&mut CatalogQuery),
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Screen::Library(_, q) = &mut self.state.screen {
            edit(q);
        }
        self.state.selection = 0;
        self.state.wall = Default::default();
        self.state.offset = 0;
        self.state.page = None;
        self.state.scroll = ScrollHandle::new();
        self.load(w, cx);
    }

    fn browse(&self, tab: Tab, q: &CatalogQuery, w: &Window, cx: &mut Context<Self>) -> Div {
        let mut tabs = div().flex().items_center().gap_1().flex_wrap();
        for (t, name) in [
            (Tab::Recommended, "Recommended"),
            (Tab::Browse, "Browse"),
            (Tab::Collections, "Collections"),
            (Tab::Playlists, "Playlists"),
        ] {
            let query = q.clone();
            tabs = tabs.child(pill(name, name, None, t == tab).on_click(cx.listener(
                move |s, _, w, cx| s.navigate(Screen::Library(t, query.clone()), w, cx),
            )));
        }
        let mut tool = div().flex().items_center().gap_2().flex_wrap();
        if matches!(tab, Tab::Browse | Tab::Recommended) {
            for (kind, name) in [
                ("Movie", "Movies"),
                ("Series", "TV Shows"),
                ("Episode", "Episodes"),
                ("MusicAlbum", "Albums"),
                ("MusicArtist", "Artists"),
                ("Audio", "Songs"),
            ] {
                tool = tool.child(
                    pill(name, name, None, q.kind == kind)
                        .h(px(28.))
                        .px_3()
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.change_filter(|q| q.kind = kind, w, cx)
                        })),
                );
            }
        }
        if tab == Tab::Browse {
            let q = q.clone();
            tool = tool.child(
                glyph_button("Shuffle library", "\u{e8b1}").on_click(cx.listener(
                    move |s, _, w, cx| s.play_queue(Some(q.clone()), None, true, false, w, cx),
                )),
            );
        }
        let current_library = self
            .libraries
            .iter()
            .find(|l| Some(&l.id) == q.library.as_ref())
            .map(|l| l.name.as_str());
        let filter_name = match (current_library, q.favorites) {
            (Some(n), true) => format!("{n} · Favorites"),
            (Some(n), false) => n.to_owned(),
            (None, true) => "Favorites".to_owned(),
            (None, false) => "Filters".to_owned(),
        };
        tool = tool.child(
            pill(
                "filters",
                filter_name,
                Some(FILTER),
                self.filters_open || q.favorites || q.library.is_some(),
            )
            .h(px(28.))
            .px_3()
            .on_click(cx.listener(|s, _, _, cx| {
                s.filters_open = !s.filters_open;
                cx.notify();
            })),
        );
        if tab != Tab::Recommended {
            let sort_label = match q.sort {
                "DateCreated" => "Recently added",
                "CommunityRating" => "Rating",
                _ if q.descending => "Title Z–A",
                _ => "Title A–Z",
            };
            tool = tool.child(
                tip(
                    pill("sort", sort_label, Some(SORT), false)
                        .h(px(28.))
                        .px_3(),
                    "Change sort order",
                )
                .on_click(cx.listener(|s, _, w, cx| {
                    s.change_filter(
                        |q| {
                            (q.sort, q.descending) = match (q.sort, q.descending) {
                                ("SortName", false) => ("SortName", true),
                                ("SortName", true) => ("DateCreated", true),
                                ("DateCreated", _) => ("CommunityRating", true),
                                _ => ("SortName", false),
                            };
                        },
                        w,
                        cx,
                    );
                })),
            );
        }
        let mut header = div()
            .px(px(PAD))
            .py_3()
            .flex()
            .gap_2()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(tabs)
                    .child(
                        glyph_button("Refresh", REFRESH)
                            .on_click(cx.listener(|s, _, w, cx| s.refresh(w, cx))),
                    ),
            )
            .child(tool);
        if self.filters_open {
            let mut filters = div().flex().flex_wrap().gap_2().child(
                pill("all-libraries", "All libraries", None, q.library.is_none()).on_click(
                    cx.listener(|s, _, w, cx| s.change_filter(|q| q.library = None, w, cx)),
                ),
            );
            for (i, lib) in self.libraries.iter().enumerate() {
                let id = lib.id.clone();
                let kind = lib.catalog_kind();
                filters = filters.child(
                    pill(
                        ("library-filter", i),
                        lib.name.clone(),
                        None,
                        q.library.as_ref() == Some(&id),
                    )
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.change_filter(
                            |q| {
                                q.library = Some(id.clone());
                                if let Some(k) = kind {
                                    q.kind = k;
                                }
                            },
                            w,
                            cx,
                        )
                    })),
                );
            }
            filters = filters.child(pill("unwatched", "Unwatched", None, q.unwatched).on_click(
                cx.listener(|s, _, w, cx| s.change_filter(|q| q.unwatched = !q.unwatched, w, cx)),
            ));
            let mut letters = div().flex().flex_wrap().gap_1().child(
                pill("all-letters", "All", None, q.initial.is_none())
                    .h(px(24.))
                    .on_click(
                        cx.listener(|s, _, w, cx| s.change_filter(|q| q.initial = None, w, cx)),
                    ),
            );
            for (i, c) in ('A'..='Z').enumerate() {
                letters = letters.child(
                    pill(("letter", i), c.to_string(), None, q.initial == Some(c))
                        .h(px(24.))
                        .px_2()
                        .on_click(cx.listener(move |s, _, w, cx| {
                            s.change_filter(|q| q.initial = Some(c), w, cx)
                        })),
                );
            }
            header = header.child(letters);
            header = header.child(filters.child(
                pill("favorites", "Favorites", Some("\u{eb51}"), q.favorites).on_click(
                    cx.listener(|s, _, w, cx| {
                        s.change_filter(|q| q.favorites = !q.favorites, w, cx)
                    }),
                ),
            ));
        }
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(self.catalog_wall(q, w, cx))
    }

    fn search_view(&self, term: &str, w: &Window, cx: &mut Context<Self>) -> Div {
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(title("Search", 26.))
                    .child(
                        div().flex().gap_2().children(
                            [(false, "Titles"), (true, "People")].into_iter().map(
                                |(people, name)| {
                                    pill(name, name, None, self.people_search == people).on_click(
                                        cx.listener(move |s, _, w, cx| {
                                            s.people_search = people;
                                            s.state.offset = 0;
                                            s.state.page = None;
                                            s.load(w, cx);
                                        }),
                                    )
                                },
                            ),
                        ),
                    )
                    .child(Input::new(&self.search).prefix(icon(SEARCH, 16.))),
            )
            .child(self.grid(
                w,
                cx,
                if term.is_empty() {
                    "Search your Jellyfin library."
                } else {
                    "No matching titles."
                },
            ))
    }

    fn detail(&self, item: &Item, season: Option<&str>, w: &Window, cx: &mut Context<Self>) -> Div {
        let height = (f32::from(w.viewport_size().height) * 0.62).clamp(265., 560.);
        let mut details = div()
            .absolute()
            .left(px(20.))
            .right(px(20.))
            .bottom(px(16.))
            .flex()
            .flex_col()
            .items_start()
            .gap_3()
            .child(self.wordmark(item, 360., 94.));
        if item.series_name.is_some() {
            details = details
                .child(title(format!("{}  ·  {}", item.caption(), item.name), 18.).line_clamp(2));
        }
        let mut tags = div().flex().flex_wrap().items_center().gap_2();
        for value in [
            item.production_year.map(|y| y.to_string()),
            item.official_rating.clone(),
            item.run_time_ticks
                .filter(|t| *t >= 600_000_000)
                .map(|t| format!("{} min", t / 600_000_000)),
            item.community_rating.map(|r| format!("★ {r:.1}")),
        ]
        .into_iter()
        .flatten()
        {
            tags = tags.child(
                label(value, 13., TEXT)
                    .bg(rgba(0x101115ac))
                    .rounded_full()
                    .px_3()
                    .py_1(),
            );
        }
        for badge in item.badges() {
            tags = tags.child(label(badge, 13., TEXT).px_2().bg(rgb(SURFACE)).rounded_md());
        }
        details = details.child(tags);
        let mut actions = div().flex().flex_wrap().items_center().gap_2();
        if item.playable() {
            actions = actions.child(self.play_button(item, cx));
        } else if let Some(episode) = if item.kind == "Series" {
            self.state.series_target.as_ref()
        } else {
            self.state.page.as_ref().and_then(|p| {
                p.items
                    .iter()
                    .filter(|i| i.playable())
                    .find(|i| i.user_data.as_ref().and_then(|d| d.played) != Some(true))
                    .or_else(|| p.items.iter().find(|i| i.playable()))
            })
        } {
            let episode = episode.clone();
            let text = episode_action(&episode);
            actions = actions.child(
                pill("play-child", text, Some(PLAY), true)
                    .h(px(40.))
                    .on_click(cx.listener(move |s, _, w, cx| s.play_item(episode.clone(), w, cx))),
            );
        }
        if item.playable() {
            let watched = item.user_data.as_ref().and_then(|u| u.played) == Some(true);
            actions = actions.child(
                glyph_button(
                    if watched {
                        "Mark unwatched"
                    } else {
                        "Mark watched"
                    },
                    "\u{e73e}",
                )
                .bg(if watched { rgb(TEXT) } else { rgba(0x15171ccb) })
                .text_color(rgb(if watched { BG } else { TEXT }))
                .on_click(cx.listener(|s, _, w, cx| s.toggle_flag(false, w, cx))),
            );
        }
        let favorite = item.user_data.as_ref().and_then(|u| u.is_favorite) == Some(true);
        actions = actions.child(
            glyph_button(
                if favorite {
                    "Remove from favorites"
                } else {
                    "Add to favorites"
                },
                "\u{eb51}",
            )
            .bg(if favorite {
                rgb(TEXT)
            } else {
                rgba(0x15171ccb)
            })
            .text_color(rgb(if favorite { BG } else { TEXT }))
            .on_click(cx.listener(|s, _, w, cx| s.toggle_flag(true, w, cx))),
        );
        if item.playable() {
            actions = actions.child(
                glyph_button("Tracks and file info", "\u{e946}")
                    .bg(rgb(SURFACE))
                    .on_click(cx.listener(|s, _, _, cx| {
                        s.state.info_open = !s.state.info_open;
                        cx.notify();
                    })),
            );
        } else if matches!(
            item.kind.as_str(),
            "Series"
                | "Folder"
                | "CollectionFolder"
                | "Playlist"
                | "BoxSet"
                | "MusicArtist"
                | "MusicAlbum"
        ) {
            for (shuffle, name) in [(false, "Play all"), (true, "Shuffle")] {
                let parent = item.clone();
                actions = actions.child(pill(name, name, None, false).on_click(cx.listener(
                    move |s, _, w, cx| {
                        s.play_queue(None, Some(parent.clone()), shuffle, false, w, cx)
                    },
                )));
            }
        }
        if matches!(item.kind.as_str(), "MusicAlbum" | "MusicArtist" | "Audio") {
            let parent = item.clone();
            actions = actions.child(pill("instant-mix", "Instant Mix", None, false).on_click(
                cx.listener(move |s, _, w, cx| {
                    s.play_queue(None, Some(parent.clone()), false, true, w, cx)
                }),
            ));
        }
        if item.playable() && item.can_download != Some(false) {
            let target = item.clone();
            actions = actions.child(glyph_button("Download original", "\u{e896}").on_click(
                cx.listener(move |s, _, w, cx| s.enqueue_download(target.clone(), w, cx)),
            ));
        }
        if self.player.as_ref().is_some_and(|p| p.audio_only) && item.playable() {
            for (next, name) in [(true, "Play next"), (false, "Add to queue")] {
                let target = item.clone();
                actions = actions.child(pill(name, name, None, false).on_click(cx.listener(
                    move |s, _, _, cx| {
                        let q = s.controls.queue.get_or_insert_with(Default::default);
                        if next {
                            q.push_front(target.clone());
                        } else {
                            q.push_back(target.clone());
                        }
                        cx.notify();
                    },
                )));
            }
        }
        if matches!(item.kind.as_str(), "Series" | "MusicAlbum" | "Playlist") {
            let target = item.clone();
            actions = actions.child(pill("download-all", "Download all", None, false).on_click(
                cx.listener(move |s, _, w, cx| s.queue_download_group(target.clone(), w, cx)),
            ));
        }
        if matches!(
            item.kind.as_str(),
            "Movie" | "Episode" | "Series" | "MusicAlbum" | "Audio"
        ) {
            let edit = item.clone();
            actions = actions.child(
                glyph_button("Edit metadata and artwork", "\u{e70f}")
                    .on_click(cx.listener(move |s, _, w, cx| s.open_editor(edit.clone(), w, cx))),
            );
        }
        details = details.child(actions);
        let mut head = div().child(
            div()
                .relative()
                .w_full()
                .h(px(height))
                .overflow_hidden()
                .child(self.backdrop(item))
                .child(details)
                .child(
                    div().absolute().left(px(16.)).top(px(12.)).child(
                        glyph_button("Back", BACK)
                            .bg(rgba(0x11121699))
                            .on_click(cx.listener(|s, _, w, cx| s.back(w, cx))),
                    ),
                ),
        );
        if self.state.editor.is_some() {
            head = head.child(self.edit_form(cx));
        }
        if self.state.info_open {
            head = head.child(self.file_info(item, cx));
        }
        if let Some(text) = item.overview.as_ref().filter(|t| !t.is_empty()) {
            head = head.child(
                div()
                    .px_5()
                    .pt_2()
                    .pb_4()
                    .child(title("Overview", 21.))
                    .child(label(text.clone(), 15., TEXT).mt_2().line_height(px(23.))),
            );
        }
        if let Some(genres) = item.genres.as_ref().filter(|g| !g.is_empty()) {
            head = head.child(label(genres.join("  ·  "), 13., MUTED).px_5().pb_4());
        }
        let mut supplements = div();
        if item.kind == "Series" {
            let rule = self.downloads.rules.iter().find(|r| r.series == item.id);
            let mut row = div()
                .px_5()
                .py_2()
                .flex()
                .items_center()
                .flex_wrap()
                .gap_2()
                .child(label("Automatic downloads", 14., TEXT));
            for (limit, name) in [(0, "Off"), (3, "Next 3 unwatched"), (5, "Next 5 unwatched")] {
                let series = item.clone();
                let specials = rule.is_some_and(|r| r.specials);
                row = row.child(
                    pill(name, name, None, rule.map_or(0, |r| r.limit) == limit).on_click(
                        cx.listener(move |s, _, _, cx| s.rule(&series, limit, specials, cx)),
                    ),
                );
            }
            if let Some(rule) = rule {
                let series = item.clone();
                let (limit, specials) = (rule.limit, rule.specials);
                row = row.child(
                    pill("sync-specials", "Include specials", None, specials).on_click(
                        cx.listener(move |s, _, _, cx| s.rule(&series, limit, !specials, cx)),
                    ),
                );
            }
            supplements = supplements.child(row);
        }
        if let Some(result) = &self.state.ratings {
            match result {
                Ok(scores) if !scores.is_empty() => {
                    let mut group = div()
                        .px_5()
                        .py_2()
                        .flex()
                        .items_center()
                        .flex_wrap()
                        .gap_3()
                        .child(title("Ratings", 17.));
                    for score in scores {
                        let value = score
                            .score
                            .map(|n| format!("{n:.0}%"))
                            .or_else(|| score.value.map(|n| n.to_string()))
                            .unwrap_or_default();
                        group = group.child(label(format!("{}  {value}", score.source), 14., TEXT));
                    }
                    head = head.child(group);
                }
                Err(e) => head = head.child(label(format!("MDBList · {e}"), 13., MUTED).px_5()),
                _ => {}
            }
        }
        if item.kind == "Series" {
            supplements = supplements.child(self.credits_extras(item, cx));
        } else {
            head = head.child(self.credits_extras(item, cx));
        }
        if !item.playable() {
            let mut section = div().px_5().py_2().child(title(
                if item.kind == "Series" {
                    "Episodes"
                } else {
                    "Titles"
                },
                21.,
            ));
            if !self.state.seasons.is_empty() {
                let mut seasons = div()
                    .id("seasons")
                    .w_full()
                    .overflow_x_scroll()
                    .flex()
                    .gap_2()
                    .py_2();
                for (i, part) in self.state.seasons.iter().enumerate() {
                    let id = part.id.clone();
                    seasons = seasons.child(
                        pill(("season", i), part.name.clone(), None, season == Some(&id)).on_click(
                            cx.listener(move |s, _, w, cx| {
                                if let Screen::Detail(_, season) = &mut s.state.screen {
                                    *season = Some(id.clone());
                                }
                                s.state.offset = 0;
                                s.state.selection = 0;
                                s.state.page = None;
                                s.load(w, cx);
                            }),
                        ),
                    );
                }
                section = section.child(seasons);
            }
            head = head.child(section);
        }
        let mut body = div()
            .id("title-details")
            .track_scroll(&self.state.scroll)
            .size_full()
            .overflow_y_scroll()
            .child(head);
        if let Some(page) = &self.state.page
            && !page.items.is_empty()
        {
            for (i, child) in page.items.iter().enumerate() {
                let target = child.clone();
                let mut thumbnail = div()
                    .relative()
                    .w(px(128.))
                    .h(px(72.))
                    .flex_none()
                    .overflow_hidden()
                    .rounded_md()
                    .bg(rgb(ACTIVE));
                if let Some(img_data) = self.artwork.get(child.landscape()) {
                    thumbnail =
                        thumbnail.child(img(img_data).size_full().object_fit(ObjectFit::Cover));
                }
                if let Some(strip) = self.card_format_strip(child, &("episode", i).into(), 128., cx)
                {
                    thumbnail = thumbnail.child(strip);
                }
                body = body.child(
                    div()
                        .id(("episode", i))
                        .mx_4()
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_4()
                        .when(self.keyboard && i == self.state.selection, |d| {
                            d.bg(rgb(ACTIVE))
                        })
                        .hover(|d| d.bg(rgb(SURFACE)))
                        .child(thumbnail)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(label(child.caption(), 12., MUTED).truncate())
                                .child(title(child.name.clone(), 16.).truncate())
                                .when_some(child.overview.clone(), |d, t| {
                                    d.child(label(t, 13., MUTED).line_clamp(2))
                                }),
                        )
                        .when(child.kind == "Episode", |row| {
                            let active = self
                                .state
                                .series_target
                                .as_ref()
                                .is_some_and(|ep| ep.id == child.id);
                            let played =
                                child.user_data.as_ref().and_then(|u| u.played) == Some(true);
                            let play = child.clone();
                            row.when(active && !played && child.position() == 0, |r| {
                                r.child(label("Up next", 13., LIME))
                            })
                            .when(played && child.position() == 0, |r| {
                                r.child(label("Watched", 13., MUTED))
                            })
                            .child(
                                pill(
                                    ("play-episode", i),
                                    episode_action(child),
                                    Some(PLAY),
                                    active,
                                )
                                .on_click(cx.listener(
                                    move |s, _, w, cx| {
                                        cx.stop_propagation();
                                        s.play_item(play.clone(), w, cx);
                                    },
                                )),
                            )
                        })
                        .child(icon(NEXT, 14.))
                        .on_click(
                            cx.listener(move |s, _, w, cx| s.open_item(target.clone(), w, cx)),
                        ),
                );
            }
            body = body.child(self.paging(cx));
        } else if !item.playable() {
            body = body.child(
                div()
                    .px_5()
                    .child(self.empty("No titles in this selection.")),
            );
        }
        if item.kind == "Series" {
            body = body.child(supplements);
        }
        div().size_full().min_h_0().child(body)
    }

    fn connection(&self, cx: &mut Context<Self>) -> Div {
        let mut panel = div()
            .w(px(440.))
            .max_w_full()
            .flex()
            .flex_col()
            .gap_4()
            .p_8()
            .rounded_xl()
            .bg(rgb(SURFACE))
            .child(label("RustyFin", 15., MUTED).font_weight(FontWeight::SEMIBOLD));
        if let Some(code) = &self.code {
            panel = panel
                .child(title("Pair this device", 28.))
                .child(label(
                    "Open Quick Connect in a signed-in Jellyfin client and enter this code.",
                    15.,
                    MUTED,
                ))
                .child(title(code.clone(), 46.))
                .child(
                    pill("cancel", "Cancel", None, false)
                        .bg(rgb(ACTIVE))
                        .on_click(cx.listener(|s, _, w, cx| s.cancel_pairing(w, cx))),
                );
        } else {
            panel = panel
                .child(title("Connect to Jellyfin", 28.))
                .child(label("Server address", 14., MUTED))
                .child(Input::new(&self.address))
                .child(
                    pill(
                        "connect",
                        if self.busy {
                            "Connecting…"
                        } else {
                            "Get pairing code"
                        },
                        None,
                        true,
                    )
                    .on_click(cx.listener(|s, _, w, cx| s.connect(w, cx))),
                );
        }
        if self.code.is_none() {
            panel = panel.child(pill("detect", "Find local servers", None, false).bg(rgb(ACTIVE))
                .on_click(cx.listener(|s, _, w, cx| {
                    if !s.busy {
                        s.work(w, cx, crate::jellyfin::discovery::nearby, |s, devices, _, _| {
                            s.nearby = devices;
                            if s.nearby.is_empty() { s.message = "No servers replied on this network. Enter the server address to connect.".into(); }
                            Ok(())
                        });
                    }
                })));
            for (i, device) in self.nearby.iter().enumerate() {
                let address = device.address.clone();
                panel = panel.child(
                    pill(
                        ("local-server", i),
                        format!("{} · {}", device.name, address),
                        None,
                        false,
                    )
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.address
                            .update(cx, |input, cx| input.set_value(address.clone(), w, cx));
                        cx.notify();
                    })),
                );
            }
            for (i, a) in self.accounts.iter().enumerate() {
                let account = a.clone();
                panel = panel.child(
                    pill(
                        ("saved-profile", i),
                        format!(
                            "{} · {}",
                            a.username.as_deref().unwrap_or("Jellyfin"),
                            a.server
                        ),
                        None,
                        false,
                    )
                    .on_click(
                        cx.listener(move |s, _, w, cx| s.choose_account(account.clone(), w, cx)),
                    ),
                );
            }
        }
        div().size_full().child(
            div()
                .id("pair-scroll")
                .size_full()
                .overflow_y_scroll()
                .child(
                    div()
                        .min_h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .p_5()
                        .child(panel),
                ),
        )
    }
}

impl Render for Library {
    fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family("Segoe UI")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key));
        if self.player.is_some() {
            self.load_trickplay(w, cx);
            self.load_artwork(w, cx);
        } else if self.artwork.trickplay.active() {
            self.artwork.trickplay.clear(w, cx);
        }
        if let Some(p) = &self.player
            && !self.controls.mini
        {
            self.rail = Default::default();
            return self.playback_screen(root, p, w, cx);
        }
        if self.state.reload_series_target && !self.busy && self.message.is_empty() {
            self.state.reload_series_target = false;
            self.load(w, cx);
        }
        if self.home_dirty
            && !self.busy
            && self.message.is_empty()
            && matches!(self.state.screen, Screen::Discover)
        {
            self.load(w, cx);
        }
        self.load_artwork(w, cx);
        self.load_card_formats(w, cx);
        self.load_wall(w, cx);
        self.fetch_ratings(w, cx);
        let content = if self.server.is_none() {
            self.rail = Default::default();
            self.connection(cx)
        } else if let Screen::Detail(item, season) = &self.state.screen {
            self.rail = Default::default();
            self.detail(item, season.as_deref(), w, cx)
        } else {
            let body = match &self.state.screen {
                Screen::Discover => self.discover(w, cx),
                Screen::Library(tab, q) => self.browse(*tab, q, w, cx),
                Screen::Search(term) => self.search_view(term, w, cx),
                Screen::Live(_) | Screen::Guide(_) | Screen::Timers(_) => self.live_view(cx),
                Screen::Settings(page) => self.settings(*page, cx),
                Screen::Explore => self.explore_view(cx),
                Screen::Anime => self.anime_view(cx),
                Screen::Copies { item, play } => self.copy_picker(item, *play, cx),
                Screen::Downloads => self.download_view(cx),
                Screen::Detail(..) => unreachable!(),
            };
            div()
                .relative()
                .size_full()
                .flex()
                .pl(px(RAIL))
                .child(body)
                .child(self.rail(w, cx))
        };
        root.child(div().flex_1().min_h_0().flex().child(content))
            .when_some(self.player.as_ref().filter(|p| p.audio_only), |r, p| {
                r.child(self.mini_player(p, cx))
            })
            .when(!self.message.is_empty(), |d| {
                d.child(
                    div()
                        .bg(rgb(0x30292c))
                        .px_4()
                        .py_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(label(self.message.clone(), 14., 0xffdfdb).flex_1())
                        .when(self.failure.as_ref().is_some_and(|f| f.dolby), |d| {
                            d.child(
                                pill(
                                    "hdr-settings-error",
                                    "Windows display settings",
                                    None,
                                    false,
                                )
                                .on_click(|_, _, cx| cx.open_url("ms-settings:display")),
                            )
                        })
                        .when(self.server.is_some(), |d| {
                            d.child(
                                glyph_button("Retry", REFRESH)
                                    .on_click(cx.listener(|s, _, w, cx| s.retry_playback(w, cx))),
                            )
                        })
                        .child(glyph_button("Dismiss", "\u{e711}").on_click(cx.listener(
                            |s, _, _, cx| {
                                s.message.clear();
                                s.failure = None;
                                cx.notify();
                            },
                        ))),
                )
            })
    }
}

#[cfg(test)]
mod catalog_grid_tests {
    use super::{ArtKind, CATALOG_GAP, CatalogGrid, PAD, RAIL};

    #[test]
    fn posters_fill_each_row_at_windowed_and_4k_sizes() {
        for (width, scale, columns, art) in [
            (650., 1., 3, ArtKind::Poster),
            (1080., 1., 5, ArtKind::Poster),
            (3840. / 1.25, 1.25, 15, ArtKind::Poster),
            (3840., 1., 15, ArtKind::Poster),
            (1920., 2., 9, ArtKind::LargePoster),
        ] {
            let grid = CatalogGrid::new(width, scale);
            assert_eq!(grid.columns, columns);
            assert_eq!(grid.poster, art);
            let used = grid.card_width * columns as f32 + CATALOG_GAP * (columns - 1) as f32;
            let remaining = width - RAIL - 2. * PAD - used;
            assert!((0. ..columns as f32).contains(&remaining));
        }
    }
}
