mod anime;
mod browse;
mod catalog;
mod catalog_poster;
mod credits;
pub(super) use credits::{ExtraFeed, credit_ranges};
mod detail;
use detail::detail_tool;
mod discover;
mod home_settings;
pub(super) use discover::{featured_titles, marquee_art};
mod controller_search;
mod editor;
mod explore;
mod format_strip;
mod library_media;
mod live;
mod music;
mod navigation;
mod options;
mod search;
pub(super) use controller_search::SearchKeyboard;
mod settings;
mod settings_navigation;
pub(super) use search::{SearchState, search_grid};
mod similar;
pub(super) use settings::SettingsPage;
pub(super) use settings_navigation::SettingsNavigation;
mod player_chrome;
pub(super) use player_chrome::PREVIEW_WIDTH;
mod player_menu;
use super::artwork::background;
use super::*;
use crate::{jellyfin::ArtKind, preferences::PosterSize};
use gpui_component::tooltip::Tooltip;

const BG: u32 = 0x0e0f12;
const SURFACE: u32 = 0x15171c;
const ACTIVE: u32 = 0x2d2f35;
const TEXT: u32 = 0xededed;
const MUTED: u32 = 0xa0a1a6;
const NAV_HEIGHT: f32 = 48.;
// Space for the floating bar and its top and bottom gutters.
const NAV_CLEARANCE: f32 = NAV_HEIGHT + 18.;
const POSTER_LIFT: f32 = 3.;
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

impl ViewState {
    pub(super) fn reveal_row(&self, index: usize) {
        if matches!(self.screen, Screen::Detail(..) | Screen::Discover)
            && let Some(row) = self.scroll.bounds_for_item(index)
        {
            let top = self.scroll.bounds().top() + px(NAV_HEIGHT);
            let offset = self.scroll.offset();
            if row.top() + offset.y < top {
                self.scroll
                    .set_offset(point(offset.x, (top - row.top()).min(px(0.))));
                return;
            }
        }
        self.scroll.scroll_to_item(index);
    }
}

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
fn page_heading(text: impl Into<SharedString>, size: f32) -> Div {
    let text = text.into();
    accessibility::heading(title(text.clone(), size), text, 1)
}
fn section_heading(text: impl Into<SharedString>, size: f32) -> Div {
    let text = text.into();
    accessibility::heading(title(text.clone(), size), text, 2)
}
fn tip(button: Stateful<Div>, text: impl Into<SharedString>) -> Stateful<Div> {
    let text = text.into();
    accessibility::button(button, text.clone(), None, false)
        .focusable()
        .tab_stop(true)
        .focus(|d| d.border_1().border_color(rgb(TEXT)))
        .tooltip(move |w, cx| Tooltip::new(text.clone()).build(w, cx))
}
fn glyph_button(id: &'static str, code: &'static str) -> Stateful<Div> {
    icon_button(id, icon(code, 17.))
}
const ART_CONTROL_BG: u32 = 0x111418df;

fn artwork_control(button: Stateful<Div>) -> Stateful<Div> {
    button
        .bg(rgba(ART_CONTROL_BG))
        .text_color(rgb(0xffffff))
        .border_1()
        .border_color(rgba(0xffffff40))
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
    let text = text.into();
    let control = div()
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
        .child(text.clone());
    accessibility::button(control, text, None, false)
}
fn episode_play_label(item: &Item) -> String {
    let action = if item.position() > 0 {
        "Resume"
    } else if item.user_data.as_ref().and_then(|d| d.played) == Some(true) {
        "Replay"
    } else {
        "Play"
    };
    match (item.parent_index_number, item.index_number) {
        (Some(s), Some(e)) => format!("{action} S{s} E{e}"),
        _ => format!("{action} {}", item.name),
    }
}
fn episode_action(item: &Item) -> String {
    let label = episode_play_label(item);
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
        Self::fit(viewport_width - 2. * PAD, scale, 192., 15)
    }

    fn fit(available: f32, scale: f32, target_width: f32, max_columns: usize) -> Self {
        let available = available.max(1.);
        let columns = ((available + CATALOG_GAP) / (target_width + CATALOG_GAP))
            .round()
            .clamp(1., max_columns as f32) as usize;
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

fn poster_years(item: &Item) -> String {
    let Some(start) = item.production_year else {
        return String::new();
    };
    if item.kind == "Series" {
        if item.status.as_deref() == Some("Continuing") {
            return format!("{start} – Present");
        }
        let end = item
            .end_date
            .as_deref()
            .and_then(|date| date.get(..4)?.parse::<u32>().ok());
        if let Some(end) = end.filter(|year| *year > start) {
            return format!("{start} – {end}");
        }
    }
    start.to_string()
}

fn score_badge(name: &'static str) -> Stateful<Div> {
    div()
        .id(name)
        .flex()
        .items_center()
        .flex_none()
        .gap_1()
        .h(px(20.))
        .px(px(5.))
        .rounded(px(4.))
        .bg(rgba(0x080a0de6))
        .font_weight(FontWeight::SEMIBOLD)
        .tooltip(move |w, cx| Tooltip::new(name).build(w, cx))
}

fn score_badges(item: &Item, imdb: Option<f64>) -> impl Iterator<Item = Stateful<Div>> {
    // Jellyfin uses these fields for several metadata providers and omits the source.
    let critic = item.critic_rating.filter(|r| (0.0..=100.0).contains(r));
    let community = item.community_rating.filter(|&r| r > 0. && r <= 10.);
    let critic = critic.map(|score| {
        score_badge("Critic rating")
            .child(
                svg()
                    .path("icons/tomato.svg")
                    .size(px(12.))
                    .flex_none()
                    .text_color(rgb(0xff6a57)),
            )
            .child(label(format!("{score:.0}%"), 11., TEXT))
    });
    let audience = if let Some(score) = imdb {
        Some(
            score_badge("IMDb rating")
                .child(label("IMDb", 11., 0xffd35a))
                .child(label(format!("{score:.1}"), 11., TEXT)),
        )
    } else {
        community.map(|score| {
            score_badge("Community rating").child(label(format!("★ {score:.1}"), 11., 0xffd35a))
        })
    };
    [critic, audience].into_iter().flatten()
}

fn poster_scores(item: &Item, bottom: f32) -> Option<Div> {
    let mut badges = score_badges(item, None).peekable();
    badges.peek()?;
    Some(
        div()
            .absolute()
            .bottom(px(bottom))
            .right(px(5.))
            .flex()
            .flex_col()
            .items_end()
            .gap(px(2.))
            .children(badges),
    )
}

fn card_frame(face: Div, width: f32, selected: bool, id: ElementId) -> Stateful<Div> {
    let face = if selected {
        face.with_animation(
            "poster-focus-lift",
            Animation::new(Duration::from_millis(140)).with_easing(ease_out_quint()),
            |d, t| d.top(px(-POSTER_LIFT * t)),
        )
        .into_any_element()
    } else {
        face.into_any_element()
    };
    div()
        .id(id)
        .group("media-card")
        .w(px(width))
        .min_w_0()
        .flex_none()
        .cursor_pointer()
        .pt(px(POSTER_LIFT))
        .child(face)
}

impl Library {
    fn imdb_score(&self) -> Option<f64> {
        self.state
            .ratings
            .as_ref()?
            .as_ref()
            .ok()?
            .iter()
            .find_map(crate::services::Rating::imdb_score)
    }

    fn card(
        &self,
        item: &Item,
        width: f32,
        layout: CardLayout,
        selected: bool,
        id: ElementId,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let face = match layout {
            CardLayout::Shelf(kind) => self.shelf_face(item, width, kind, selected, &id, cx),
            CardLayout::Catalog { poster } => {
                self.catalog_face(item, width, poster, selected, &id, cx)
            }
        };
        let target = item.clone();
        card_frame(face, width, selected, id).on_click(cx.listener(
            move |s, _, w, cx| match layout {
                CardLayout::Shelf(kind) => s.activate_home_item(kind, target.clone(), w, cx),
                CardLayout::Catalog { .. } => s.open_item(target.clone(), w, cx),
            },
        ))
    }

    fn shelf_face(
        &self,
        item: &Item,
        width: f32,
        kind: ShelfKind,
        selected: bool,
        id: &ElementId,
        cx: &mut Context<Self>,
    ) -> Div {
        let continued = kind == ShelfKind::Continue;
        let recent = !continued;
        let key = kind.art(item);
        let height = (width
            * if continued {
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
        if continued
            && let Some(progress) = item
                .run_time_ticks
                .filter(|v| *v > 0)
                .map(|d| item.position() as f32 / d as f32)
            && progress > 0.
        {
            picture = picture.child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(3.))
                    .rounded_full()
                    .overflow_hidden()
                    .bg(rgba(0xdde4d45c))
                    .child(
                        div()
                            .h_full()
                            .w(relative(progress.clamp(0., 1.)))
                            .bg(rgb(LIME)),
                    ),
            );
        }
        let unplayed = item
            .user_data
            .as_ref()
            .and_then(|data| data.unplayed_item_count)
            .filter(|&count| count > 0 && recent && item.kind == "Series");
        if recent && let Some(scores) = poster_scores(item, 5.) {
            picture = picture.child(scores);
        }
        if let Some(count) = unplayed {
            let value = if count > 99 {
                "99+".into()
            } else {
                count.to_string()
            };
            picture = picture.child(
                label(value, 11., 0xffffff)
                    .id("unplayed-episodes")
                    .absolute()
                    .top(px(5.))
                    .right(px(5.))
                    .min_w(px(21.))
                    .h(px(21.))
                    .px(px(3.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_weight(FontWeight::SEMIBOLD)
                    .rounded_full()
                    .bg(rgb(0x23529e))
                    .tooltip(move |w, cx| {
                        let noun = if count == 1 { "episode" } else { "episodes" };
                        Tooltip::new(format!("{count} unwatched {noun}")).build(w, cx)
                    }),
            );
        }
        picture = picture.child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(8.))
                .border_2()
                .border_color(if selected { rgb(LIME) } else { rgba(0) })
                .group_hover("media-card", |s| s.border_color(rgb(LIME))),
        );
        if continued {
            let button = div()
                .id("continue-actions")
                .size(px(28.))
                .rounded_full()
                .bg(rgba(0x101513ed))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .child(icon("\u{e712}", 16.))
                .focusable()
                .tab_stop(true)
                .when(self.popup_menu.is_none(), |d| {
                    d.tooltip(|w, cx| Tooltip::new("More actions").build(w, cx))
                })
                .when(
                    !selected && !self.popup_menu.as_ref().is_some_and(|(key, _)| key == id),
                    |d| d.opacity(0.).group_hover("media-card", |d| d.opacity(1.)),
                )
                .focus(|d| d.opacity(1.).border_1().border_color(rgb(LIME)));
            picture = picture.child(
                self.popup_trigger(
                    id.clone(),
                    accessibility::button(
                        button,
                        format!("More actions for {}", item.name),
                        None,
                        false,
                    ),
                    discover::continue_menu(item.clone()),
                    cx,
                )
                .absolute()
                .right(px(8.))
                .top(px(8.)),
            );
        }
        let format_width = if continued || unplayed.is_some() {
            width - 28.
        } else {
            width
        };
        if let Some(strip) = self.card_format_strip(item, id, format_width, cx) {
            picture = picture.child(strip);
        }
        let primary = if continued {
            item.series_name.as_ref().unwrap_or(&item.name)
        } else {
            &item.name
        };
        let secondary = if continued && item.series_name.is_some() {
            match (item.parent_index_number, item.index_number) {
                (Some(s), Some(e)) => format!("S{s:02} E{e:02}  ·  {}", item.name),
                _ => item.name.clone(),
            }
        } else if item.kind == "Episode" {
            item.caption()
        } else if recent {
            poster_years(item)
        } else {
            item.production_year
                .map(|y| y.to_string())
                .unwrap_or_default()
        };
        let name = title(primary.clone(), 16.);
        let name = if recent {
            name.w(px(width))
                .text_center()
                .line_clamp(1)
                .text_ellipsis()
        } else {
            name.truncate()
        };
        let detail =
            label(secondary, 13., if continued { 0xd9ddda } else { 0xa7afa9 }).line_height(px(18.));
        div()
            .relative()
            .child(picture)
            .child(name.line_height(px(20.)).mt(px(7.)))
            .child(detail.truncate().when(recent, |d| d.text_center()))
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
        let name = item.series_name.as_ref().unwrap_or(&item.name);
        let wordmark = self.title_logo(item, width, height).unwrap_or_else(|| {
            title(name.clone(), (width / 7.).clamp(30., 48.))
                .line_height(px(49.))
                .line_clamp(2)
                .max_w(px(width * 1.65))
        });
        accessibility::heading(wordmark, name.clone().into(), 1)
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
        self.state.wheel_motion = None;
        self.state.offset = 0;
        self.state.page = None;
        self.state.scroll = ScrollHandle::new();
        self.load(w, cx);
    }

    fn basic_detail_header(&self, item: &Item, w: &Window, cx: &mut Context<Self>) -> Div {
        let height = NAV_CLEARANCE + (f32::from(w.viewport_size().height) * 0.62).clamp(265., 560.);
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
        tags = tags.children(score_badges(item, self.imdb_score()));
        for badge in item.badges() {
            tags = tags.child(label(badge, 13., TEXT).px_2().bg(rgb(SURFACE)).rounded_md());
        }
        details = details.child(tags);
        let mut actions = div().flex().flex_wrap().items_center().gap_2();
        if item.playable() {
            actions = actions.child(self.play_button(item, cx));
        } else if let Some(episode) = self.state.page.as_ref().and_then(|p| {
            p.items
                .iter()
                .filter(|i| i.playable())
                .find(|i| i.user_data.as_ref().and_then(|d| d.played) != Some(true))
                .or_else(|| p.items.iter().find(|i| i.playable()))
        }) {
            let episode = episode.clone();
            let text = episode_action(&episode);
            actions = actions.child(
                pill("play-child", text, Some(PLAY), true)
                    .h(px(40.))
                    .on_click(cx.listener(move |s, _, w, cx| s.play_item(episode.clone(), w, cx))),
            );
        }
        details = details.child(self.secondary_actions(item, actions, cx));
        div().child(
            div()
                .relative()
                .w_full()
                .h(px(height))
                .overflow_hidden()
                .child(self.backdrop(item))
                .child(details)
                .child(
                    div().absolute().left(px(16.)).top(px(NAV_CLEARANCE)).child(
                        artwork_control(glyph_button("Back", BACK))
                            .on_click(cx.listener(|s, _, w, cx| s.back(w, cx))),
                    ),
                ),
        )
    }

    fn secondary_actions(&self, item: &Item, mut actions: Div, cx: &mut Context<Self>) -> Div {
        let resume_item = if item.kind == "Series" {
            self.state.series_target.as_ref()
        } else {
            Some(item)
        };
        if let Some(target) = resume_item.filter(|i| i.playable() && i.position() > 0) {
            let target = target.clone();
            actions = actions.child(
                pill("restart-title", "Play from beginning", Some(REFRESH), false).on_click(
                    cx.listener(move |s, _, w, cx| {
                        s.play_item_at(target.clone(), Some(0.), w, cx);
                    }),
                ),
            );
        }
        if item.playable() {
            let watched = item.user_data.as_ref().and_then(|u| u.played) == Some(true);
            actions = actions.child(
                detail_tool(
                    item,
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
            detail_tool(
                item,
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
            .when(self.state.detail_panel == Some(DetailPanel::More), |b| {
                b.track_focus(&self.detail_current_focus)
            })
            .on_click(cx.listener(|s, _, w, cx| s.toggle_flag(true, w, cx))),
        );
        if item.playable() {
            actions = actions.child(
                detail_tool(item, "Tracks and file info", "\u{e946}")
                    .bg(rgb(SURFACE))
                    .on_click(cx.listener(|s, _, w, cx| {
                        s.state.info_open = !s.state.info_open;
                        s.state.detail_panel = None;
                        s.focus.focus(w);
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
            actions = actions.child(detail_tool(item, "Download original", "\u{e896}").on_click(
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
                detail_tool(item, "Edit metadata and artwork", "\u{e70f}").on_click(cx.listener(
                    move |s, _, w, cx| {
                        s.state.detail_panel = None;
                        s.open_editor(edit.clone(), w, cx);
                    },
                )),
            );
        }
        actions
    }

    fn detail(&self, item: &Item, season: Option<&str>, w: &Window, cx: &mut Context<Self>) -> Div {
        let cinematic = matches!(item.kind.as_str(), "Series" | "Movie");
        let mut head = if cinematic {
            self.cinematic_header(item, w, cx)
        } else {
            self.basic_detail_header(item, w, cx)
        };
        if self.state.editor.is_some() {
            head = head.child(self.edit_form(cx));
        }
        if self.state.info_open {
            head = head.child(self.file_info(item, cx));
        }
        if !cinematic && let Some(text) = item.overview.as_ref().filter(|t| !t.is_empty()) {
            head = head.child(
                div()
                    .px_5()
                    .pt_2()
                    .pb_4()
                    .child(section_heading("Overview", 21.))
                    .child(label(text.clone(), 15., TEXT).mt_2().line_height(px(23.))),
            );
        }
        if !cinematic && let Some(genres) = item.genres.as_ref().filter(|g| !g.is_empty()) {
            head = head.child(label(genres.join("  ·  "), 13., MUTED).px_5().pb_4());
        }
        let mut supplements = div();
        if item.kind == "Series" && self.owns_download_scope(item) {
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
                Ok(scores) => {
                    let mut additional = scores
                        .iter()
                        .filter(|score| score.imdb_score().is_none())
                        .peekable();
                    if additional.peek().is_some() {
                        let group = div()
                            .px_5()
                            .py_2()
                            .flex()
                            .items_center()
                            .flex_wrap()
                            .gap_3()
                            .child(section_heading("Ratings", 17.))
                            .children(additional.map(|score| {
                                let value = score
                                    .score
                                    .map(|n| format!("{n:.0}%"))
                                    .or_else(|| score.value.map(|n| n.to_string()))
                                    .unwrap_or_default();
                                label(format!("{}  {value}", score.source), 14., TEXT)
                            }));
                        if item.kind == "Series" {
                            supplements = supplements.child(group);
                        } else {
                            head = head.child(group);
                        }
                    }
                }
                Err(e) => {
                    let message = label(format!("MDBList · {e}"), 13., MUTED).px_5();
                    if item.kind == "Series" {
                        supplements = supplements.child(message);
                    } else {
                        head = head.child(message);
                    }
                }
            }
        }
        if item.kind == "Series" {
            supplements = supplements.child(self.credits_extras(item, w, cx));
        } else {
            head = head.child(self.credits_extras(item, w, cx));
        }
        if !item.playable() {
            let mut section = div()
                .px_5()
                .when(cinematic, |d| d.px(px(detail::detail_inset(w))))
                .py_2()
                .child(section_heading(
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
                    .track_scroll(&self.state.season_scroll)
                    .w_full()
                    .overflow_x_scroll()
                    .flex()
                    .gap_2()
                    .py_2();
                for (i, part) in self.state.seasons.iter().enumerate() {
                    let id = part.id.clone();
                    seasons = seasons.child(
                        pill(("season", i), part.name.clone(), None, season == Some(&id))
                            .when(
                                self.keyboard
                                    && self.state.series_focus == SeriesFocus::Seasons
                                    && season == Some(&id),
                                |d| d.border_2().border_color(rgb(LIME)),
                            )
                            .on_click(cx.listener(move |s, _, w, cx| {
                                s.state.series_focus = SeriesFocus::Hero;
                                s.focus.focus(w);
                                s.select_season(&id, w, cx);
                            })),
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
                        .when(cinematic, |d| d.mx(px(detail::detail_inset(w) - 8.)))
                        .px_2()
                        .py_2()
                        .rounded_md()
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap_4()
                        .when(
                            self.keyboard
                                && i == self.state.selection
                                && (item.kind != "Series"
                                    || self.state.series_focus == SeriesFocus::Episodes),
                            |d| d.bg(rgb(ACTIVE)),
                        )
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
        body = body.when_some(self.similar_section(w, cx), |d, row| d.child(row));
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
            .child(crate::navigation_icons::waterline(164., 40.));
        if let Some(code) = &self.code {
            panel = panel
                .child(page_heading("Pair this device", 28.))
                .child(label(
                    "Open Quick Connect in a signed-in Jellyfin client and enter this code.",
                    15.,
                    MUTED,
                ))
                .child(title(code.clone(), 46.))
                .child(
                    pill("pair-password", "Use username and password", None, false)
                        .on_click(cx.listener(|s, _, w, cx| s.show_password_sign_in(w, cx))),
                )
                .child(
                    pill("cancel", "Cancel", None, false)
                        .bg(rgb(ACTIVE))
                        .on_click(cx.listener(|s, _, w, cx| s.cancel_pairing(w, cx))),
                );
        } else {
            panel = panel
                .child(page_heading("Connect to Jellyfin", 28.))
                .child(label("Server address", 14., MUTED))
                .child(accessibility::field(
                    Input::new(&self.address).disabled(self.busy),
                    &self.address,
                    "Server address",
                    false,
                    cx,
                ));
            if let Some(form) = &self.password_sign_in {
                panel = panel
                    .child(label("Username", 14., MUTED))
                    .child(accessibility::field(
                        Input::new(&form.username).disabled(self.busy),
                        &form.username,
                        "Username",
                        false,
                        cx,
                    ))
                    .child(label("Password", 14., MUTED))
                    .child(accessibility::field(
                        Input::new(&form.password).disabled(self.busy),
                        &form.password,
                        "Password",
                        true,
                        cx,
                    ))
                    .child(
                        pill(
                            "password-submit",
                            if self.busy {
                                "Signing in…"
                            } else {
                                "Sign in"
                            },
                            None,
                            true,
                        )
                        .on_click(cx.listener(|s, _, w, cx| s.sign_in_with_password(w, cx))),
                    )
                    .child(
                        pill("password-back", "Back", None, false).on_click(cx.listener(
                            |s, _, w, cx| {
                                s.cancel_pairing(w, cx);
                                s.message.clear();
                            },
                        )),
                    );
            } else {
                panel = panel
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
                    )
                    .child(
                        pill("use-password", "Use username and password", None, false)
                            .bg(rgb(ACTIVE))
                            .on_click(cx.listener(|s, _, w, cx| s.show_password_sign_in(w, cx))),
                    );
            }
        }
        if self.code.is_none() && self.password_sign_in.is_none() {
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
                .track_scroll(&self.state.scroll)
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
        // Keep the window state through a queued episode or track change.
        if !self.busy
            && self.player.as_ref().is_none_or(|p| p.audio_only)
            && let Some(fullscreen) = self.fullscreen_before_video.take()
            && w.is_fullscreen() != fullscreen
        {
            w.toggle_fullscreen();
        }
        let return_opacity = self.return_opacity(w, cx);
        let root = div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .text_color(rgb(TEXT))
            .font_family("Segoe UI")
            .track_focus(&self.focus)
            .capture_any_mouse_down(cx.listener(|s, _, _, _| s.state.wheel_motion = None))
            .on_key_down(cx.listener(Self::key))
            .children(accessibility::frame())
            .child(Self::mouse_navigation(cx))
            .children(self.smooth_scrolling(w, cx));
        let chrome_opacity = self
            .player
            .as_mut()
            .filter(|_| !self.controls.mini)
            .map(|p| {
                let rail_focused = p.keyboard_controls() && self.menu_focus.timeline.is_focused(w);
                let interacting = accessibility::active()
                    || self.scrub.is_some()
                    || self.controls.hover.is_some()
                    || self.controls.volume_hovered
                    || self.controls.episode_hover.is_some()
                    || rail_focused;
                p.controls_opacity(interacting, self.controls.animate_chrome, w)
            });
        if self.player.is_some() {
            self.load_trickplay(chrome_opacity.is_some_and(|a| a > 0.), w, cx);
            self.load_artwork(w, cx);
        } else if self.artwork.trickplay.active() {
            self.artwork.trickplay.clear(w, cx);
        }
        if let Some(p) = &self.player
            && let Some(opacity) = chrome_opacity
        {
            self.state.search.restore_after_playback =
                matches!(self.state.screen, Screen::Search(_));
            if opacity == 0.
                && !self.focus.is_focused(w)
                && self.credits_prompt().is_none()
                && w.is_window_active()
            {
                self.focus.focus(w);
            }
            let root = root
                .key_context(PLAYER_KEYS)
                .on_action(
                    cx.listener(|s, _: &NextPlayerControl, w, cx| s.step_player_focus(true, w, cx)),
                )
                .on_action(cx.listener(|s, _: &PreviousPlayerControl, w, cx| {
                    s.step_player_focus(false, w, cx)
                }))
                .on_mouse_move(cx.listener(|s, e: &MouseMoveEvent, w, cx| {
                    if s.player
                        .as_mut()
                        .is_some_and(|p| p.pointer_moved(e.position, w))
                    {
                        cx.notify();
                    }
                }));
            return self.playback_screen(root, p, w, opacity, cx);
        }
        let root = root.bg(rgb(BG));
        if std::mem::take(&mut self.state.search.restore_after_playback) {
            self.state.search.restore_focus(w);
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
        // Several wheel frames move through exactly the same rows. Revisit
        // prefetching when the visible range, page data or outstanding work changes.
        let scrolling_wall = matches!(self.state.screen, Screen::Library(..))
            && self.state.wheel_motion.is_some()
            && self.player.is_none();
        if !scrolling_wall || self.state.wall.needs_prefetch {
            self.load_artwork(w, cx);
            self.load_card_formats(w, cx);
            self.state.wall.needs_prefetch = self.artwork.loading() || self.formats.loading();
        }
        self.load_search_target(w, cx);
        self.load_wall(w, cx);
        self.fetch_ratings(w, cx);
        self.load_similar(w, cx);
        self.load_extras(w, cx);
        let discover = matches!(self.state.screen, Screen::Discover);
        let artwork = match &self.state.screen {
            Screen::Discover => self.featured.get(self.hero),
            Screen::Detail(item, _) => Some(item.as_ref()),
            _ => None,
        }
        .and_then(|item| self.marquee_image(item, w));
        let tint = self
            .artwork
            .glass
            .sample(artwork.as_deref(), discover, w, cx);
        let content = if self.server.is_none() {
            self.connection(cx)
        } else {
            match &self.state.screen {
                Screen::Detail(item, season) => self.detail(item, season.as_deref(), w, cx),
                Screen::Discover => self.discover(tint, w, cx),
                Screen::Library(tab, q) => self.browse(*tab, q, w, cx),
                Screen::Search(term) => self.search_view(term, w, cx),
                Screen::Live(_) | Screen::Guide(_) | Screen::Timers(_) => self.live_view(cx),
                Screen::Settings(page) => self.settings(*page, cx),
                Screen::Explore | Screen::ExploreTitle(_) => self.explore_view(cx),
                Screen::Anime | Screen::AnimeTitle(_) => self.anime_view(cx),
                Screen::Copies { item, action } => self.copy_picker(item, *action, cx),
                Screen::Downloads => self.download_view(cx),
            }
        };
        let hero = matches!(self.state.screen, Screen::Detail(..))
            || matches!(self.state.screen, Screen::Discover)
                && self.featured.get(self.hero).is_some();
        root.child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .when(self.server.is_some(), |d| {
                    d.tab_index(1).tab_group().tab_stop(false)
                })
                .when(self.server.is_some() && !hero, |d| d.pt(px(NAV_CLEARANCE)))
                .opacity(return_opacity)
                .child(content),
        )
        .when(self.server.is_some(), |d| {
            d.child(self.top_navigation(tint, w, cx))
        })
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
mod poster_tests {
    use super::{ArtKind, CATALOG_GAP, CatalogGrid, Item, PAD, ShelfKind, poster_years};
    use crate::preferences::PosterSize;

    #[test]
    fn recent_series_use_primary_posters_and_respect_series_status() {
        let mut series: Item = serde_json::from_value(serde_json::json!({
            "Id": "show", "Name": "Show", "Type": "Series",
            "ImageTags": {"Primary": "poster"}, "BackdropImageTags": ["backdrop"],
            "ProductionYear": 2021, "EndDate": "2025-09-01T00:00:00Z", "Status": "Continuing"
        }))
        .unwrap();
        assert_eq!(
            ShelfKind::RecentTv.art(&series),
            series.art(ArtKind::Poster)
        );
        assert_eq!(
            ShelfKind::Continue.art(&series),
            series.art(ArtKind::SmallBackdrop)
        );
        assert_eq!(poster_years(&series), "2021 – Present");
        series.status = Some("Ended".into());
        assert_eq!(poster_years(&series), "2021 – 2025");
        series.production_year = Some(2025);
        assert_eq!(poster_years(&series), "2025");
    }

    #[test]
    fn posters_fill_each_row_at_windowed_and_4k_sizes() {
        for (width, scale, columns, art) in [
            (650., 1., 3, ArtKind::Poster),
            (1080., 1., 5, ArtKind::Poster),
            (3840. / 1.25, 1.25, 15, ArtKind::Poster),
            (3840., 1., 15, ArtKind::Poster),
            (1920., 2., 10, ArtKind::LargePoster),
        ] {
            let grid = CatalogGrid::new(width, scale);
            assert_eq!(grid.columns, columns);
            assert_eq!(grid.poster, art);
            let used = grid.card_width * columns as f32 + CATALOG_GAP * (columns - 1) as f32;
            let remaining = width - 2. * PAD - used;
            assert!((0. ..columns as f32).contains(&remaining));
        }
    }

    #[test]
    fn library_poster_sizes_reflow_at_windowed_and_maximized_4k_widths() {
        for (client, counts) in [(1348., [7, 5, 4]), (3840., [20, 15, 11])] {
            let scale = 1.25;
            let available = client / scale - 2. * PAD - 24.;
            for (size, count) in [PosterSize::Small, PosterSize::Medium, PosterSize::Large]
                .into_iter()
                .zip(counts)
            {
                let grid = CatalogGrid::wall_fit(available, scale, size);
                assert_eq!(grid.columns, count);
                let used = grid.card_width * count as f32 + CATALOG_GAP * (count - 1) as f32;
                assert!((0. ..count as f32).contains(&(available - used)));
                assert_eq!(
                    grid.poster == ArtKind::LargePoster,
                    grid.card_width * scale > 320.
                );
            }
        }
    }
}
