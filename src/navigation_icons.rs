use gpui::{AssetSource, Image, ImageFormat, Img, SharedString, Styled, img, px};
use std::borrow::Cow;
use std::sync::{Arc, LazyLock};

pub struct NavigationIcons;

pub fn waterline(width: f32, height: f32) -> Img {
    static LOGO: LazyLock<Arc<Image>> = LazyLock::new(|| {
        Arc::new(Image::from_bytes(
            ImageFormat::Png,
            include_bytes!("../assets/brand/waterline.png").to_vec(),
        ))
    });
    img(Arc::clone(&LOGO))
        .w(px(width))
        .h(px(height))
        .flex_none()
}

// GPUI tints each monochrome silhouette with its row's current text color.
macro_rules! glyph {
    ($drawing:literal) => {
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">"#,
            $drawing,
            "</svg>"
        )
    };
}

const ICONS: &[(&str, &str)] = &[
    (
        "nav/heart",
        glyph!(
            r#"<path d="M12 21 3.2 12.5C-2.3 6.9 5.6-.7 11 5l1 1.1L13 5c5.4-5.7 13.3 1.9 7.8 7.5z"/>"#
        ),
    ),
    (
        "nav/libraries",
        glyph!(
            r#"<path d="M2 6h2.3v14H19v2.2H4a2 2 0 0 1-2-2z"/><path fill-rule="evenodd" d="M8 2h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2zm4 4.6v6.8l5-3.4z"/>"#
        ),
    ),
    (
        "nav/movies",
        glyph!(
            r#"<path d="M2 7.2V5a2 2 0 0 1 2-2h1.5l2 3H10L8 3h3.3l2 3h2.5l-2-3h3.3l2 3h2.4L20 3a2 2 0 0 1 2 2v13a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2z"/>"#
        ),
    ),
    (
        "nav/shows",
        glyph!(
            r#"<path d="M3 3h18a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-5.5l.8 2H7.7l.8-2H3a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"/>"#
        ),
    ),
    (
        "nav/folder",
        glyph!(
            r#"<path d="M2 5a2 2 0 0 1 2-2h5.8l2.4 2.5H20a2 2 0 0 1 2 2V19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2z"/>"#
        ),
    ),
    (
        "nav/music",
        glyph!(
            r#"<path d="M10 5.3 21 3v13.2a3.7 3.7 0 1 1-2.8-3.5V7.3l-5.5 1.1v10A3.7 3.7 0 1 1 10 15z"/>"#
        ),
    ),
    (
        "nav/search",
        glyph!(
            r#"<g fill="none" stroke="black" stroke-width="2.8"><circle cx="10" cy="10" r="6.4"/><path stroke-linecap="round" d="m15 15 6 6"/></g>"#
        ),
    ),
    (
        "nav/downloads",
        glyph!(
            r#"<path fill="none" stroke="black" stroke-width="2.8" stroke-linecap="round" stroke-linejoin="round" d="M12 3v11m-4.2-4 4.2 4.2 4.2-4.2M4 15v4a1 1 0 0 0 1 1h14a1 1 0 0 0 1-1v-4"/>"#
        ),
    ),
    (
        "nav/settings",
        glyph!(
            r#"<path fill-rule="evenodd" d="m9.5 1.5-.7 2.7-1.5.8-2.7-.7-2.4 4.1 2 2-.1 2.2-1.9 2 2.4 4.2 2.7-.7 1.6 1 .6 2.8h5l.7-2.8 1.6-1 2.7.7 2.4-4.2-2-2v-2.2l2-2-2.4-4.1-2.7.7-1.6-.8-.7-2.7zM12 8.3a3.7 3.7 0 1 1 0 7.4 3.7 3.7 0 1 1 0-7.4z"/>"#
        ),
    ),
    (
        "nav/explore",
        glyph!(
            r#"<path fill-rule="evenodd" d="M12 2a10 10 0 1 1 0 20 10 10 0 1 1 0-20zm5.6 4.4-7.9 3.3-3.3 7.9 7.9-3.3z"/><circle cx="12" cy="12" r="1.3"/>"#
        ),
    ),
    (
        "nav/live",
        glyph!(
            r#"<path fill-rule="evenodd" d="m7 1 5 4.2L17 1l1.4 1.6L15 5.5h5a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-11a2 2 0 0 1 2-2h5L5.6 2.6zm3 8.1v7.8l6-3.9z"/>"#
        ),
    ),
];

impl AssetSource for NavigationIcons {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, svg)| Cow::Borrowed(svg.as_bytes())))
    }

    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| (*name).into())
            .collect())
    }
}
