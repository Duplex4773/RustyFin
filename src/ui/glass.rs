use super::theme::{AMBIENT_BASE, AMBIENT_INDIGO, AMBIENT_MIDNIGHT, BG, SURFACE};
use gpui::{App, Div, Hsla, ImageId, RenderImage, Rgba, Window, hsla, rgb};
use gpui::{Bounds, ObjectFit, canvas, div, img, point, prelude::*, px, relative, size};
use std::{
    sync::{Arc, OnceLock},
    time::Instant,
};

const NEUTRAL: u32 = SURFACE;
const WASH_WIDTH: u32 = 144;
const WASH_HEIGHT: u32 = 96;
// Leave enough range for the blue highlight in the fallback palette.
const DITHER_ALPHA: u8 = 175;
const DITHER_TILE: u32 = 512;

struct Palette {
    tint: Hsla,
    accent: Hsla,
    main_x: f32,
}

// Bake the diffuse light once at palette changes. Every translucent panel sees
// the same window-space lighting, even while the content above it scrolls.
fn ambient_frame(palette: Option<&Palette>) -> image::RgbaImage {
    let palette = palette.filter(|p| p.tint != Hsla::from(rgb(NEUTRAL)));
    let light = |hue: Hsla| Rgba::from(hsla(hue.h, hue.s.min(0.42), 0.44, 1.));
    let primary = palette.map_or(rgb(AMBIENT_INDIGO), |p| light(p.tint));
    let secondary = palette.map_or(rgb(0x273b4b), |p| light(p.accent));
    let origin = palette.map_or(0.78, |p| p.main_x);
    let pool = |x: f32, y: f32| (-(x * x + y * y) * 1.6).exp();
    image::RgbaImage::from_fn(WASH_WIDTH + 2, WASH_HEIGHT + 2, |ix, iy| {
        let x = ix.saturating_sub(1).min(WASH_WIDTH - 1) as f32 / (WASH_WIDTH - 1) as f32;
        let y = iy.saturating_sub(1).min(WASH_HEIGHT - 1) as f32 / (WASH_HEIGHT - 1) as f32;
        let mix = |under: Rgba, over: Rgba, a| under.blend(Rgba { a, ..over });
        let base = mix(
            rgb(AMBIENT_BASE),
            rgb(AMBIENT_MIDNIGHT),
            (1. - y) * 0.65 + x * 0.35,
        );
        let primary_alpha = if palette.is_some() { 0.22 } else { 0.64 };
        let glow = mix(
            base,
            primary,
            primary_alpha * pool((x - origin) / 0.57, (y - 0.15) / 0.54),
        );
        let ink = mix(
            glow,
            secondary,
            0.19 * pool((x + origin - 1.) / 0.45, (y - 0.78) / 0.50),
        );
        // Preserve fractional dark tones in the 8-bit atlas. The final tile
        // composites near-black over this expanded range at display resolution.
        let a = f32::from(DITHER_ALPHA) / 255.;
        let encode = |c: f32| ((255. * c - 2. * a) / (1. - a)).round() as u8;
        image::Rgba([encode(ink.b), encode(ink.g), encode(ink.r), 255])
    })
}

fn dither_frame() -> image::RgbaImage {
    // Each 4 × 4 block has mean grey 2. Scramble the ranks between blocks
    // to avoid visible repeating diagonals and keep the grain still at rest.
    const RANKS: [u8; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
    image::RgbaImage::from_fn(DITHER_TILE, DITHER_TILE, |x, y| {
        let block = (x / 4) + (y / 4) * (DITHER_TILE / 4);
        let seed = block.wrapping_mul(0x9e3779b9);
        let mask = ((seed ^ (seed >> 16)).wrapping_mul(0x85ebca6b) >> 28) as u8;
        let rank = RANKS[((y % 4) * 4 + x % 4) as usize] ^ mask;
        let grey = rank.count_ones() as u8;
        image::Rgba([grey, grey, grey, DITHER_ALPHA])
    })
}

fn ambient_dither() -> impl IntoElement {
    static TILE: OnceLock<Arc<RenderImage>> = OnceLock::new();
    canvas(
        |_, _, _| (),
        |bounds, _, w, _| {
            let tile = TILE.get_or_init(|| {
                Arc::new(RenderImage::new(vec![image::Frame::new(dither_frame())]))
            });
            let side = px(DITHER_TILE as f32 / w.scale_factor());
            let cols = (bounds.size.width / side).ceil() as usize;
            let rows = (bounds.size.height / side).ceil() as usize;
            for y in 0..rows {
                for x in 0..cols {
                    let origin = bounds.origin + point(side * x as f32, side * y as f32);
                    let _ = w.paint_image(
                        Bounds::new(origin, size(side, side)),
                        px(0.).into(),
                        tile.clone(),
                        0,
                        false,
                    );
                }
            }
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

fn ambient_image(image: Arc<RenderImage>) -> gpui::Img {
    let dx = 1. / WASH_WIDTH as f32;
    let dy = 1. / WASH_HEIGHT as f32;
    img(image)
        .absolute()
        .left(relative(-dx))
        .top(relative(-dy))
        .w(relative(1. + 2. * dx))
        .h(relative(1. + 2. * dy))
        .object_fit(ObjectFit::Fill)
}

// Sample the decoded GPUI frame in BGRA order. Give coloured midtones a vote,
// so letterboxing, white lettering and black shadows don't dominate the tint.
fn artwork_palette(pixels: &[u8], width: usize, height: usize) -> Palette {
    let mut hues = [0_f32; 24];
    let mut positions = [0_f32; 24];
    let (pixels, _) = pixels.as_chunks::<4>();
    for y in (0..height).step_by(height.div_ceil(24).max(1)) {
        for x in (0..width).step_by(width.div_ceil(40).max(1)) {
            let [b, g, r, a] = pixels[y * width + x];
            let color = Hsla::from(rgb(u32::from_be_bytes([0, r, g, b])));
            if a < 128 || color.s < 0.13 || !(0.10..0.90).contains(&color.l) {
                continue;
            }
            let bin = (color.h * 24.).round() as usize % 24;
            let vote = color.s * (1. - (2. * color.l - 1.).abs());
            hues[bin] += vote;
            positions[bin] += vote * (x as f32 + 0.5) / width as f32;
        }
    }
    let (bin, weight) = hues
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap();
    if *weight < 1. {
        return Palette {
            tint: rgb(NEUTRAL).into(),
            accent: hsla(0., 0., 0.23, 1.),
            main_x: 0.69,
        };
    }
    let tint = hsla(bin as f32 / 24., 0.36, 0.185, 1.);
    let accent = (0..24)
        .filter(|&i| (3..=21).contains(&bin.abs_diff(i)) && hues[i] > weight * 0.05)
        .max_by(|&a, &b| hues[a].total_cmp(&hues[b]));
    let main_x = positions[bin] / weight;
    let main_on_left = accent.is_some_and(|i| main_x < positions[i] / hues[i]);
    Palette {
        tint,
        accent: hsla(
            accent.unwrap_or(bin) as f32 / 24.,
            if accent.is_some() { 0.42 } else { 0.25 },
            0.32,
            1.,
        ),
        main_x: if main_on_left { 0.23 } else { 0.77 } + (main_x - 0.5) * 0.18,
    }
}

// One continuous light field for the whole page. Each section shows a slice of
// this texture so the colour also runs through the open space between shelves.
fn wash_frame(palette: &Palette) -> image::RgbaImage {
    let mut main = palette.tint;
    if main.s < 0.25 {
        main.s = 0.03;
        main.l = 0.17;
    } else {
        main.s = 0.56;
        main.l = 0.28;
    }
    let pool = |x: f32, y: f32| (-1.65 * (x * x + y * y)).exp();
    let main = Rgba::from(main);
    let accent = Rgba::from(palette.accent);
    // Duplicate the edge texels so linear filtering stays inside this image's
    // atlas tile, even where the wash starts and ends over bright artwork.
    image::RgbaImage::from_fn(WASH_WIDTH + 2, WASH_HEIGHT + 2, |ix, iy| {
        let x = ix.saturating_sub(1).min(WASH_WIDTH - 1) as f32 / (WASH_WIDTH - 1) as f32;
        let y = iy.saturating_sub(1).min(WASH_HEIGHT - 1) as f32 / (WASH_HEIGHT - 1) as f32;
        let primary = pool((x - palette.main_x) / 0.57, (y - 0.46) / 0.60);
        let secondary = pool((x - (1. - palette.main_x)) / 0.47, (y - 0.61) / 0.43);
        let ink = rgb(BG)
            .blend(Rgba {
                a: secondary,
                ..accent
            })
            .blend(Rgba { a: primary, ..main });
        let alpha = ease((y / 0.42).min(1.)) * ease(((1. - y) / 0.42).min(1.));
        image::Rgba([ink.b, ink.g, ink.r, alpha].map(|c| (255. * c).round() as u8))
    })
}

// A low-resolution translucent ellipse behind the hero copy. Position it on
// the copy side and borrow the image hue without tinting the whole backdrop.
fn hero_glow_frame(palette: &Palette) -> image::RgbaImage {
    let light = Rgba::from(hsla(palette.tint.h, palette.tint.s.min(0.45), 0.52, 1.));
    let peak = if palette.tint == Hsla::from(rgb(NEUTRAL)) {
        0.09
    } else {
        0.29
    };
    let center = 0.24 + palette.main_x * 0.12;
    image::RgbaImage::from_fn(128, 64, |ix, iy| {
        let x = ix as f32 / 127.;
        let y = iy as f32 / 63.;
        let dx = (x - center) / 0.43;
        let dy = (y - 0.51) / 0.49;
        let falloff = (1. - dx * dx - dy * dy).clamp(0., 1.);
        let alpha = ease(falloff) * peak;
        image::Rgba([light.b, light.g, light.r, alpha].map(|c| (255. * c).round() as u8))
    })
}

struct DiscoverLayers {
    wash: Arc<RenderImage>,
    frost: Arc<RenderImage>,
}

struct ArtworkLayers {
    ambient: Arc<RenderImage>,
    glow: Arc<RenderImage>,
    discover: Option<DiscoverLayers>,
}

impl ArtworkLayers {
    fn release(self, w: &mut Window, cx: &mut App) {
        cx.drop_image(self.ambient, Some(w));
        cx.drop_image(self.glow, Some(w));
        if let Some(layers) = self.discover {
            cx.drop_image(layers.wash, Some(w));
            cx.drop_image(layers.frost, Some(w));
        }
    }
}

// The panel only needs broad patches of colour. Sample a small fixed grid from
// the same decoded BGRA frame as the marquee, then diffuse it once per image.
fn frosted_frame(bytes: &[u8], width: usize, height: usize) -> image::RgbaImage {
    let (pixels, _) = bytes.as_chunks::<4>();
    let small = image::RgbaImage::from_fn(48, 16, |x, y| {
        let mut sum = [0u32; 4];
        for dy in 0..4 {
            for dx in 0..4 {
                let sx = (x as usize * 8 + dx * 2 + 1) * width / (48 * 8);
                let sy = (y as usize * 8 + dy * 2 + 1) * height / (16 * 8);
                for (total, &value) in sum.iter_mut().zip(&pixels[sy * width + sx]) {
                    *total += u32::from(value);
                }
            }
        }
        image::Rgba(sum.map(|value| (value / 16) as u8))
    });
    image::imageops::blur(&small, 2.5)
}

fn ease(t: f32) -> f32 {
    t * t * (3. - 2. * t)
}

pub(super) struct ArtworkGlass {
    source: Option<(ImageId, bool)>,
    from: Rgba,
    to: Rgba,
    start: Instant,
    duration: f32,
    layers: Option<ArtworkLayers>,
    outgoing: Option<ArtworkLayers>,
}

impl Default for ArtworkGlass {
    fn default() -> Self {
        Self {
            source: None,
            from: rgb(NEUTRAL),
            to: rgb(NEUTRAL),
            start: Instant::now(),
            duration: 1.,
            layers: None,
            outgoing: None,
        }
    }
}

impl ArtworkGlass {
    fn current(&self) -> (Rgba, f32) {
        let t = if self.duration > 0. {
            ease((self.start.elapsed().as_secs_f32() / self.duration).min(1.))
        } else {
            1.
        };
        (self.from.blend(Rgba { a: t, ..self.to }), t)
    }

    pub fn clear(&mut self, w: &mut Window, cx: &mut App) {
        for layers in [self.outgoing.take(), self.layers.take()]
            .into_iter()
            .flatten()
        {
            layers.release(w, cx);
        }
        *self = Self::default();
    }

    pub fn sample(
        &mut self,
        image: Option<&RenderImage>,
        discover: bool,
        animate: bool,
        w: &mut Window,
        cx: &mut App,
    ) -> Rgba {
        let source = image.map(|im| (im.id, discover));
        if self.source != source {
            if let Some(old) = self.outgoing.take() {
                old.release(w, cx);
            }
            self.outgoing = self.layers.take();
            self.from = self.current().0;
            self.source = source;
            self.to = rgb(NEUTRAL);
            if let Some(im) = image
                && let Some(bytes) = im.as_bytes(0)
            {
                let size = im.size(0);
                let (width, height) = (size.width.0 as usize, size.height.0 as usize);
                let palette = artwork_palette(bytes, width, height);
                self.to = palette.tint.into();
                let texture = |pixels| Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]));
                self.layers = Some(ArtworkLayers {
                    ambient: texture(ambient_frame(Some(&palette))),
                    glow: texture(hero_glow_frame(&palette)),
                    discover: discover.then(|| DiscoverLayers {
                        wash: texture(wash_frame(&palette)),
                        frost: texture(frosted_frame(bytes, width, height)),
                    }),
                });
            }
            self.duration = if discover { 1. } else { 0.32 };
            self.start = Instant::now();
        }
        if !animate {
            self.duration = 0.;
        }
        let (color, t) = self.current();
        if t == 1.
            && let Some(old) = self.outgoing.take()
        {
            old.release(w, cx);
        }
        if t < 1. && (self.from != self.to || self.outgoing.is_some() || self.layers.is_some()) {
            w.request_animation_frame();
        }
        color
    }

    fn visible_layers(&self) -> impl Iterator<Item = (&ArtworkLayers, f32)> {
        let (_, t) = self.current();
        [(self.outgoing.as_ref(), 1. - t), (self.layers.as_ref(), t)]
            .into_iter()
            .filter_map(|(layers, weight)| (weight > 0.).then_some((layers?, weight)))
    }

    pub fn ambient(&self) -> Div {
        static FALLBACK: OnceLock<Arc<RenderImage>> = OnceLock::new();
        let (_, t) = self.current();
        let background = div().absolute().inset_0().overflow_hidden();
        if t >= 1.
            && let Some(layers) = &self.layers
        {
            return background
                .child(ambient_image(layers.ambient.clone()))
                .child(ambient_dither());
        }
        let fallback = || {
            FALLBACK
                .get_or_init(|| {
                    Arc::new(RenderImage::new(vec![image::Frame::new(ambient_frame(
                        None,
                    ))]))
                })
                .clone()
        };
        let from = self
            .outgoing
            .as_ref()
            .map_or_else(fallback, |l| l.ambient.clone());
        let background = background.child(ambient_image(from));
        background
            .when(
                t < 1. && (self.layers.is_some() || self.outgoing.is_some()),
                |d| {
                    let to = self
                        .layers
                        .as_ref()
                        .map_or_else(fallback, |l| l.ambient.clone());
                    d.child(ambient_image(to).opacity(t))
                },
            )
            .child(ambient_dither())
    }

    pub fn hero_light(&self) -> Div {
        div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .children(self.visible_layers().map(|(layers, weight)| {
                img(layers.glow.clone())
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Fill)
                    .opacity(weight)
            }))
    }

    fn discover_layers(&self) -> impl Iterator<Item = (&DiscoverLayers, f32)> {
        self.visible_layers()
            .filter_map(|(layers, weight)| Some((layers.discover.as_ref()?, weight)))
    }

    pub fn wash(&self, start: f32, end: f32) -> Div {
        let span = end - start;
        let pad_x = 1. / WASH_WIDTH as f32;
        let pad_y = 1. / WASH_HEIGHT as f32;
        div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .children(self.discover_layers().map(|(layers, weight)| {
                img(layers.wash.clone())
                    .absolute()
                    // Map the visible slice to the unpadded part of the image.
                    .left(relative(-pad_x))
                    .w(relative(1. + 2. * pad_x))
                    .top(relative(-(start + pad_y) / span))
                    .h(relative((1. + 2. * pad_y) / span))
                    .object_fit(ObjectFit::Fill)
                    .opacity(weight)
            }))
    }

    pub fn frost_layer(&self) -> Div {
        div()
            .absolute()
            .inset_0()
            .children(self.discover_layers().map(|(layers, weight)| {
                img(layers.frost.clone())
                    .absolute()
                    .inset_0()
                    .size_full()
                    .rounded(px(16.))
                    .object_fit(ObjectFit::Fill)
                    .opacity(weight * 0.07)
            }))
    }
}

#[derive(Default)]
pub(super) struct ShelfLight {
    pub hovered: Option<usize>,
    travel: Option<(f32, f32, Instant)>,
}

impl ShelfLight {
    pub fn position(&mut self, target: f32, animate: bool, w: &Window) -> f32 {
        if !animate {
            self.travel = None;
            return target;
        }
        let Some((from, to, start)) = self.travel else {
            self.travel = Some((target, target, Instant::now()));
            return target;
        };
        let t = ease((start.elapsed().as_secs_f32() / 0.26).min(1.));
        let current = from + (to - from) * t;
        if (target - to).abs() > 0.5 {
            self.travel = Some((current, target, Instant::now()));
            w.request_animation_frame();
        } else if t < 1. && from != to {
            w.request_animation_frame();
        }
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ambient_dither_preserves_the_mean_and_the_fallback_palette() {
        let tile = dither_frame();
        for y in (0..DITHER_TILE).step_by(4) {
            for x in (0..DITHER_TILE).step_by(4) {
                let mut sum = 0u32;
                for dy in 0..4 {
                    for dx in 0..4 {
                        let [b, g, r, a] = tile[(x + dx, y + dy)].0;
                        assert_eq!([r, b, a], [g, g, DITHER_ALPHA]);
                        sum += u32::from(g);
                    }
                }
                assert_eq!(sum, 32);
            }
        }
        let fallback = ambient_frame(None);
        let [b, g, r, _] = fallback[(112, 15)].0;
        assert!(b > g && g > r);
        let warm = artwork_palette([23, 78, 211, 255].repeat(16 * 16).as_slice(), 16, 16);
        let field = ambient_frame(Some(&warm));
        let [b, _, r, _] = field[(112, 15)].0;
        assert!(r > b);
        for img in [&field, &fallback] {
            assert!(
                img.pixels()
                    .all(|p| p.0[..3].iter().all(|&v| v > 0 && v < 255))
            );
        }
    }

    #[test]
    fn samples_bgra_artwork_and_ignores_letterboxing() {
        let mut warm = vec![[0, 0, 0, 255]; 40 * 24];
        warm[320..640].fill([48, 132, 200, 255]);
        let color: Rgba = artwork_palette(warm.as_flattened(), 40, 24).tint.into();
        assert!(color.r > color.g && color.g > color.b);
        warm[320..640].fill([200, 132, 48, 255]);
        let color: Rgba = artwork_palette(warm.as_flattened(), 40, 24).tint.into();
        assert!(color.b > color.g && color.g > color.r);
        warm[320..640].fill([155, 155, 155, 255]);
        assert_eq!(
            artwork_palette(warm.as_flattened(), 40, 24).tint,
            Hsla::from(rgb(NEUTRAL))
        );
    }

    #[test]
    fn wash_separates_artwork_colours_and_finishes_transparent() {
        let mut frame = vec![[171, 131, 42, 255]; 40 * 24];
        for row in frame.chunks_mut(40) {
            row[..12].fill([58, 42, 159, 255]);
        }
        let wash = wash_frame(&artwork_palette(frame.as_flattened(), 40, 24));
        let [lb, _, lr, la] = wash.get_pixel(28, 50).0;
        let [rb, _, rr, ra] = wash.get_pixel(111, 50).0;
        assert!(
            lr > lb && rb > rr,
            "left R/B {lr}/{lb}, right R/B {rr}/{rb}"
        );
        assert!(lr > rr + 20 && rb > lb + 20);
        assert_eq!((la, ra), (255, 255));
        assert!(wash.get_pixel(111, 74)[3] < 150);
        for x in 0..wash.width() {
            assert_eq!(wash[(x, 0)], wash[(x, 1)]);
            assert_eq!(wash[(x, WASH_HEIGHT)], wash[(x, WASH_HEIGHT + 1)]);
            assert_eq!(wash[(x, 1)][3], 0);
            assert_eq!(wash[(x, WASH_HEIGHT)][3], 0);
        }
        for y in 0..wash.height() {
            assert_eq!(wash[(0, y)], wash[(1, y)]);
            assert_eq!(wash[(WASH_WIDTH, y)], wash[(WASH_WIDTH + 1, y)]);
        }
    }

    #[test]
    fn hero_light_is_local_to_the_copy_and_subdued_on_grayscale_art() {
        let frame = |pixel: [u8; 4]| {
            let pixels = [pixel; 16 * 16];
            hero_glow_frame(&artwork_palette(pixels.as_flattened(), 16, 16))
        };
        let warm = frame([45, 110, 205, 255]);
        let neutral = frame([120, 120, 120, 255]);
        let [b, _, r, a] = warm[(42, 32)].0;
        assert!(r > b && (60..=74).contains(&a));
        assert!(neutral[(42, 32)][3] < a / 2);
        assert!(warm.rows().all(|row| row.last().unwrap()[3] == 0));
        assert!(
            warm.rows()
                .next()
                .unwrap()
                .chain(warm.rows().last().unwrap())
                .all(|p| p[3] == 0)
        );
    }
}
