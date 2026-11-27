use gpui::{App, Div, Hsla, ImageId, RenderImage, Rgba, Window, hsla, rgb};
use gpui::{ObjectFit, div, img, prelude::*, px, relative};
use std::{sync::Arc, time::Instant};

const NEUTRAL: u32 = 0x171a20;
const WASH_WIDTH: u32 = 144;
const WASH_HEIGHT: u32 = 96;

struct Palette {
    tint: Hsla,
    accent: Hsla,
    main_x: f32,
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
        main.l = 0.33;
    } else {
        main.s = 0.48;
        main.l = 0.415;
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
        let ink = rgb(0x171b1c)
            .blend(Rgba {
                a: secondary,
                ..accent
            })
            .blend(Rgba { a: primary, ..main });
        let alpha = ease((y / 0.42).min(1.)) * ease(((1. - y) / 0.42).min(1.));
        image::Rgba([ink.b, ink.g, ink.r, alpha].map(|c| (255. * c).round() as u8))
    })
}

struct ArtworkLayers {
    wash: Arc<RenderImage>,
    frost: Arc<RenderImage>,
}

impl ArtworkLayers {
    fn release(self, w: &mut Window, cx: &mut App) {
        cx.drop_image(self.wash, Some(w));
        cx.drop_image(self.frost, Some(w));
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
                if discover {
                    let texture =
                        |pixels| Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]));
                    self.layers = Some(ArtworkLayers {
                        wash: texture(wash_frame(&palette)),
                        frost: texture(frosted_frame(bytes, width, height)),
                    });
                }
            }
            self.duration = if discover { 1. } else { 0.32 };
            self.start = Instant::now();
        }
        if !animate {
            self.duration = 0.;
        }
        let (color, t) = self.current();
        if (t == 1. || !discover)
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

    pub fn wash(&self, start: f32, end: f32) -> Div {
        let span = end - start;
        let pad_x = 1. / WASH_WIDTH as f32;
        let pad_y = 1. / WASH_HEIGHT as f32;
        div()
            .absolute()
            .inset_0()
            .overflow_hidden()
            .children(self.visible_layers().map(|(layers, weight)| {
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
            .children(self.visible_layers().map(|(layers, weight)| {
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
    pub fn position(&mut self, target: f32, w: &Window) -> f32 {
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
}
