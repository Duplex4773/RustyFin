use gpui::{Hsla, ImageId, RenderImage, Rgba, Window, hsla, rgb};
use std::time::Instant;

const NEUTRAL: u32 = 0x171a20;

// Sample the decoded GPUI frame in BGRA order. Give coloured midtones a vote,
// so letterboxing, white lettering and black shadows don't dominate the tint.
fn artwork_tint(pixels: &[u8], width: usize, height: usize) -> Rgba {
    let mut hues = [0_f32; 24];
    let (pixels, _) = pixels.as_chunks::<4>();
    for y in (0..height).step_by(height.div_ceil(24).max(1)) {
        for x in (0..width).step_by(width.div_ceil(40).max(1)) {
            let [b, g, r, a] = pixels[y * width + x];
            let color = Hsla::from(rgb(u32::from_be_bytes([0, r, g, b])));
            if a < 128 || color.s < 0.13 || !(0.10..0.90).contains(&color.l) {
                continue;
            }
            let bin = (color.h * 24.).round() as usize % 24;
            hues[bin] += color.s * (1. - (2. * color.l - 1.).abs());
        }
    }
    let (bin, weight) = hues
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap();
    if *weight < 1. {
        rgb(NEUTRAL)
    } else {
        hsla(bin as f32 / 24., 0.36, 0.185, 1.).into()
    }
}

pub(super) struct NavigationTint {
    image: Option<ImageId>,
    from: Rgba,
    to: Rgba,
    start: Instant,
}

impl Default for NavigationTint {
    fn default() -> Self {
        Self {
            image: None,
            from: rgb(NEUTRAL),
            to: rgb(NEUTRAL),
            start: Instant::now(),
        }
    }
}

impl NavigationTint {
    fn current(&self) -> (Rgba, f32) {
        let t = (self.start.elapsed().as_secs_f32() / 0.32).min(1.);
        (
            self.from.blend(Rgba {
                a: t * t * (3. - 2. * t),
                ..self.to
            }),
            t,
        )
    }

    pub fn sample(&mut self, image: Option<&RenderImage>, w: &Window) -> Rgba {
        let id = image.map(|im| im.id);
        if self.image != id {
            self.image = id;
            self.from = self.current().0;
            self.to = image
                .and_then(|im| {
                    let size = im.size(0);
                    Some(artwork_tint(
                        im.as_bytes(0)?,
                        size.width.0 as usize,
                        size.height.0 as usize,
                    ))
                })
                .unwrap_or_else(|| rgb(NEUTRAL));
            self.start = Instant::now();
        }
        let (color, t) = self.current();
        if t < 1. && self.from != self.to {
            w.request_animation_frame();
        }
        color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_bgra_artwork_and_ignores_letterboxing() {
        let mut warm = vec![[0, 0, 0, 255]; 40 * 24];
        warm[320..640].fill([48, 132, 200, 255]);
        let color = artwork_tint(warm.as_flattened(), 40, 24);
        assert!(color.r > color.g && color.g > color.b);
        warm[320..640].fill([200, 132, 48, 255]);
        let color = artwork_tint(warm.as_flattened(), 40, 24);
        assert!(color.b > color.g && color.g > color.r);
        warm[320..640].fill([155, 155, 155, 255]);
        assert_eq!(artwork_tint(warm.as_flattened(), 40, 24), rgb(NEUTRAL));
    }
}
