//! RustyFin's chrome. Artwork, rating providers and playback warnings carry their own colours.
use gpui::{prelude::*, *};
use gpui_component::Theme;
use std::sync::{Arc, OnceLock};

// Opaque colours are RGB. Edges and the artwork control fill use RGBA.
pub(super) const BG: u32 = 0x0b0d12;
pub(super) const AMBIENT_BASE: u32 = 0x07090e;
pub(super) const AMBIENT_MIDNIGHT: u32 = 0x121622;
pub(super) const AMBIENT_INDIGO: u32 = 0x34395d;
pub(super) const SURFACE: u32 = 0x13161f;
pub(super) const PANEL_GLASS: u32 = (SURFACE << 8) | 0xbf;
pub(super) const POPOVER_GLASS: u32 = (SURFACE << 8) | 0xeb;
pub(super) const PRESS_FILL: u32 = 0xffffff17;
pub(super) const CONTENT_GUTTER: f32 = 24.;
pub(super) const ACTIVE: u32 = 0x1d202b;
pub(super) const SELECTED: u32 = 0x303646;
pub(super) const TEXT: u32 = 0xffffff;
pub(super) const BODY: u32 = 0xc4cedc;
pub(super) const META: u32 = 0x94a3b8;
pub(super) const MUTED: u32 = 0x8895aa;
pub(super) const FOCUS: u32 = 0xf2f6ff;
pub(super) const HOVER_FILL: u32 = 0xffffff0a;
pub(super) const GLASS_FILL: u32 = 0xffffff24;
pub(super) const GLASS_EDGE: u32 = 0xffffff38;
pub(super) const REST_EDGE: u32 = 0xffffff1a;
pub(super) const HOVER_EDGE: u32 = 0xffffff40;
pub(super) const ART_CONTROL_BG: u32 = (SURFACE << 8) | 0xdf;

pub(super) const HOME_HERO_FRACTION: f32 = 0.55;
pub(super) const SHELF_GAP: f32 = 14.;
pub(super) const CONTINUE_WIDTH: f32 = 225.;
pub(super) const HOME_POSTER_WIDTH: f32 = 150.;
pub(super) const MICRO_FILL: u32 = 0xffffff14;
pub(super) const GLASS_WHITE_SOFT: u32 = 0xffffff1f;
pub(super) const OSD_GLASS: u32 = (BG << 8) | 0xc7;
pub(super) const OSD_RADIUS: f32 = 14.;
pub(super) const OSD_MARGIN_X: f32 = 24.;
pub(super) const OSD_MARGIN_BOTTOM: f32 = 20.;
pub(super) const OSD_TARGET: f32 = 36.;
pub(super) const OSD_ICON: f32 = 20.;
pub(super) const OSD_BUFFER: u32 = 0xffffff40;
pub(super) const SEEK_TRACK: f32 = 4.;
pub(super) const SEEK_TRACK_ACTIVE: f32 = 6.;
pub(super) const SEEK_THUMB: f32 = 12.;
pub(super) const SERIES_MAX_WIDTH: f32 = 1600.;
pub(super) const SERIES_SPLIT_MIN: f32 = 920.;
pub(super) const SIDEBAR_FRACTION: f32 = 0.32;
pub(super) const SERIES_GAP_FRACTION: f32 = 0.03;
pub(super) const EPISODE_IMAGE_WIDTH: f32 = 220.;
pub(super) const CARD_TITLE_SIZE: f32 = 14.;
pub(super) const EPISODE_TITLE_SIZE: f32 = 15.;

// GPUI supports two stops per linear gradient. Sample both cubic curves into one
// reusable alpha texture so there are no joins between quads on scaled displays.
pub(super) fn hero_scrim() -> Div {
    static SCRIM: OnceLock<Arc<RenderImage>> = OnceLock::new();
    let texture = SCRIM.get_or_init(|| {
        let smooth = |t: f32| {
            let t = t.clamp(0., 1.);
            t * t * (3. - 2. * t)
        };
        let [_, r, g, b] = BG.to_be_bytes();
        let mask = image::RgbaImage::from_fn(258, 146, |ix, iy| {
            let x = ix.saturating_sub(1).min(255) as f32 / 255.;
            let y = iy.saturating_sub(1).min(143) as f32 / 143.;
            let left = 0.92 * (1. - smooth(x / 0.55));
            let bottom = smooth((y - 0.38) / 0.62);
            image::Rgba([
                b,
                g,
                r,
                ((left + (1. - left) * bottom) * 255.).round() as u8,
            ])
        });
        Arc::new(RenderImage::new(vec![image::Frame::new(mask)]))
    });
    div().absolute().inset_0().overflow_hidden().child(
        img(texture.clone())
            .absolute()
            .left(relative(-1. / 256.))
            .top(relative(-1. / 144.))
            .w(relative(258. / 256.))
            .h(relative(146. / 144.))
            .object_fit(ObjectFit::Fill),
    )
}

pub(super) fn poster_scrim() -> Background {
    linear_gradient(
        180.,
        linear_color_stop(rgba(0x00000000), 0.76),
        linear_color_stop(rgba(0x000000e6), 1.),
    )
}

// Apply to the artwork outline, independently of the caption and hit target.
pub(super) fn poster_edge(frame: Div, selected: bool, radius: Pixels) -> Div {
    let frame = frame.rounded(radius);
    if selected {
        frame.border_2().border_color(rgb(FOCUS)).child(
            div()
                .absolute()
                .inset_0()
                .rounded((radius - px(2.)).max(px(0.)))
                .border_1()
                .border_color(rgba(0x090a0fda)),
        )
    } else {
        frame
            .border_1()
            .border_color(rgba(REST_EDGE))
            .group_hover("media-card", |s| s.border_color(rgba(HOVER_EDGE)))
    }
}

pub(super) fn primary_action(button: Stateful<Div>) -> Stateful<Div> {
    button
        .bg(rgb(TEXT))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(BG))
        .border_color(rgba(REST_EDGE))
        .shadow(vec![panel_shadow()])
        .relative()
        .hover(|s| s.bg(rgb(0xe9ecf1)))
        .when(super::scrolling::animations_enabled(), |d| {
            d.hover(|s| s.top(px(-1.)))
        })
}

pub(super) fn panel_shadow() -> BoxShadow {
    BoxShadow {
        color: rgba(0x00000048).into(),
        offset: point(px(0.), px(6.)),
        blur_radius: px(18.),
        spread_radius: px(0.),
    }
}

pub(super) fn glass_panel(panel: Div) -> Div {
    panel
        .rounded(px(16.))
        .border_1()
        .border_color(rgba(GLASS_WHITE_SOFT))
        .bg(rgba(PANEL_GLASS))
        .shadow(vec![panel_shadow()])
}

pub(crate) fn apply(cx: &mut App) {
    let theme = Theme::global_mut(cx);
    // Video is composited below GPUI. Only browsing pages paint an opaque canvas.
    theme.background = transparent_black();
    theme.foreground = rgb(TEXT).into();
    theme.popover = rgba(POPOVER_GLASS).into();
    theme.popover_foreground = rgb(TEXT).into();
    theme.border = rgba(REST_EDGE).into();
    theme.input = rgba(HOVER_EDGE).into();
    theme.ring = rgb(FOCUS).into();
    theme.caret = rgb(TEXT).into();
    // Generic component fills also appear in switches and check marks.
    theme.primary = rgb(BODY).into();
    theme.primary_hover = rgb(TEXT).into();
    theme.primary_active = rgb(META).into();
    theme.primary_foreground = rgb(BG).into();
    theme.muted = rgb(SURFACE).into();
    theme.muted_foreground = rgb(MUTED).into();
    theme.secondary = rgb(SURFACE).into();
    theme.secondary_hover = rgb(ACTIVE).into();
    theme.secondary_active = rgb(SELECTED).into();
    theme.secondary_foreground = rgb(BODY).into();
    // gpui-component uses accent as the menu row selection fill.
    theme.accent = rgb(SELECTED).into();
    theme.accent_foreground = rgb(TEXT).into();
    theme.list = rgb(SURFACE).into();
    theme.list_hover = rgb(ACTIVE).into();
    theme.list_active = rgb(SELECTED).into();
    theme.list_active_border = rgb(FOCUS).into();
    theme.selection = rgba(0x94a3b852).into();
}
