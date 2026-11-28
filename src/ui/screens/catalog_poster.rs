use super::*;
use crate::jellyfin::formats::Formats;

pub(super) fn image_height(item: &Item, width: f32) -> Pixels {
    let square = matches!(item.kind.as_str(), "MusicAlbum" | "MusicArtist" | "Audio");
    px((width * if square { 1. } else { 1.5 }).round())
}

// The catalogue has fixed card dimensions. Paint static artwork and text in
// one element so scrolling doesn't lay out every glyph label and separator.
fn line(
    text: &str,
    size: f32,
    weight: FontWeight,
    color: u32,
    max_width: Option<Pixels>,
    w: &Window,
    cx: &App,
) -> ShapedLine {
    let text: SharedString = if text.contains(['\r', '\n']) {
        text.lines().collect::<Vec<_>>().join(" ").into()
    } else {
        text.to_owned().into()
    };
    let mut run = w.text_style().to_run(text.len());
    run.font.weight = weight;
    run.color = rgb(color).into();
    let size = px(size);
    let shaped = w
        .text_system()
        .shape_line(text.clone(), size, std::slice::from_ref(&run), None);
    if let Some(width) = max_width.filter(|&width| shaped.width > width) {
        let mut wrapper = cx.text_system().line_wrapper(run.font.clone(), size);
        let mut runs = vec![run];
        let truncated = wrapper.truncate_line(text, width, "…", &mut runs);
        return w.text_system().shape_line(truncated, size, &runs, None);
    }
    shaped
}

#[derive(PartialEq)]
struct PosterTextKey {
    title: String,
    subtitle: String,
    formats: Option<Formats>,
    width: Pixels,
    font: Font,
    scale: f32,
}

struct PosterText {
    key: PosterTextKey,
    title: ShapedLine,
    subtitle: ShapedLine,
    labels: Vec<ShapedLine>,
}

impl PosterText {
    fn new(key: PosterTextKey, w: &Window, cx: &App) -> Self {
        let width = Some(key.width);
        let title = line(&key.title, 13., FontWeight::SEMIBOLD, TEXT, width, w, cx);
        let subtitle = line(&key.subtitle, 11., FontWeight::NORMAL, META, width, w, cx);
        let compact = key.width < px(155.);
        let labels = key
            .formats
            .iter()
            .flat_map(Formats::labels)
            .enumerate()
            .map(|(i, value)| {
                line(
                    value,
                    if compact { 10. } else { 11. },
                    if i == 0 {
                        FontWeight::BOLD
                    } else {
                        FontWeight::SEMIBOLD
                    },
                    0xf6f7f6,
                    None,
                    w,
                    cx,
                )
            })
            .collect();
        Self {
            key,
            title,
            subtitle,
            labels,
        }
    }

    fn paint(&self, picture: Bounds<Pixels>, w: &mut Window, cx: &mut App) {
        if !self.labels.is_empty() && picture.intersects(&w.content_mask().bounds) {
            w.with_content_mask(Some(ContentMask { bounds: picture }), |w| {
                paint_format_strip(&self.labels, picture, w, cx);
            });
        }
        let caption = Bounds::new(
            point(picture.left(), picture.bottom() + px(4.)),
            size(picture.size.width, px(27.)),
        );
        if caption.intersects(&w.content_mask().bounds) {
            w.with_content_mask(Some(ContentMask { bounds: caption }), |w| {
                let _ = self.title.paint(caption.origin, px(15.), w, cx);
                let _ =
                    self.subtitle
                        .paint(caption.origin + point(px(0.), px(15.)), px(12.), w, cx);
            });
        }
    }
}

fn paint_format_strip(
    labels: &[ShapedLine],
    picture: Bounds<Pixels>,
    w: &mut Window,
    cx: &mut App,
) {
    let compact = picture.size.width < px(155.);
    let inset = px(if compact { 4. } else { 7. });
    let pad = px(if compact { 3. } else { 4. });
    let origin = picture.origin + point(inset, inset);
    let width = labels
        .iter()
        .fold(px(4.), |sum, l| sum + l.width + pad * 2.)
        + px(labels.len().saturating_sub(1) as f32);
    w.paint_quad(
        fill(Bounds::new(origin, size(width, px(19.))), rgba(0x0b0e10e8)).corner_radii(px(4.)),
    );
    let mut x = origin.x + px(2.);
    for (i, label) in labels.iter().enumerate() {
        if i != 0 {
            w.paint_quad(fill(
                Bounds::new(point(x, origin.y + px(5.)), size(px(1.), px(9.))),
                rgba(0xffffff47),
            ));
            x += px(1.);
        }
        let _ = label.paint(point(x + pad, origin.y + px(3.)), px(13.), w, cx);
        x += label.width + pad * 2.;
    }
}

fn paint_progress(picture: Bounds<Pixels>, progress: f32, w: &mut Window) {
    let track = Bounds::new(
        point(picture.left() + px(6.), picture.bottom() - px(7.)),
        size(picture.size.width - px(12.), px(3.)),
    );
    w.paint_quad(fill(track, rgba(0x000000aa)).corner_radii(px(1.5)));
    w.paint_quad(
        fill(
            Bounds::new(
                track.origin,
                size(track.size.width * progress, track.size.height),
            ),
            rgb(TEXT),
        )
        .corner_radii(px(1.5)),
    );
}

impl Library {
    pub(super) fn catalog_face(
        &self,
        item: &Item,
        width: f32,
        poster: ArtKind,
        selected: bool,
        id: &ElementId,
        cx: &mut Context<Self>,
    ) -> Div {
        let height = image_height(item, width);
        let art = self.artwork.get(item.art(poster));
        let name = item.name.clone();
        let subtitle = if item.kind == "Episode" {
            item.caption()
        } else {
            item.production_year
                .map(|y| y.to_string())
                .unwrap_or_default()
        };
        let formats = self.formats.for_item(item);
        let mixed = formats.as_ref().is_some_and(|f| f.mixed());
        let watched = item.user_data.as_ref().and_then(|u| u.played) == Some(true);
        let progress = item
            .run_time_ticks
            .filter(|&n| n > 0)
            .map_or(0., |n| (item.position() as f32 / n as f32).clamp(0., 1.));
        let missing = art.is_none();
        let paint = canvas(
            |_, _, _| (),
            move |bounds, _, w, cx| {
                let picture = Bounds::new(bounds.origin, size(bounds.size.width, height));
                if picture.intersects(&w.content_mask().bounds) {
                    w.with_content_mask(Some(ContentMask { bounds: picture }), |w| {
                        if let Some(image) = art {
                            let fit = ObjectFit::Cover.get_bounds(picture, image.size(0));
                            let _ = w.paint_image(fit, px(8.).into(), image, 0, false);
                        } else {
                            w.paint_quad(fill(picture, rgb(SURFACE)).corner_radii(px(8.)));
                        }
                        w.paint_quad(fill(picture, poster_scrim()).corner_radii(px(8.)));
                        if progress > 0. {
                            paint_progress(picture, progress, w);
                        }
                    });
                }
                let key = PosterTextKey {
                    title: name,
                    subtitle,
                    formats: formats.filter(|_| !mixed),
                    width: picture.size.width,
                    font: w.text_style().font(),
                    scale: w.scale_factor(),
                };
                // GPUI retains this state only while the card participates in a frame.
                w.with_global_id("catalog-text".into(), |id, w| {
                    w.with_element_state(id, |previous: Option<PosterText>, w| {
                        let text = previous
                            .filter(|p| p.key == key)
                            .unwrap_or_else(|| PosterText::new(key, w, cx));
                        text.paint(picture, w, cx);
                        ((), text)
                    })
                });
            },
        )
        .absolute()
        .inset_0()
        .size_full();
        let mut overlay = poster_edge(
            div().absolute().top_0().left_0().w_full().h(height),
            selected,
            px(8.),
        );
        if missing {
            overlay = overlay
                .flex()
                .items_center()
                .justify_center()
                .child(icon(VIDEO, 23.).text_color(rgb(MUTED)));
        }
        if watched {
            overlay = overlay.child(
                div()
                    .absolute()
                    .top(px(3.))
                    .right(px(3.))
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
        if let Some(scores) = poster_scores(item, if progress > 0. { 11. } else { 5. }) {
            overlay = overlay.child(scores);
        }
        let mut face = div()
            .relative()
            .w_full()
            .h(height + px(31.))
            .child(paint)
            .child(overlay);
        if mixed {
            face = face.child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(height)
                    .overflow_hidden()
                    .children(self.card_format_strip(item, id, width, cx)),
            );
        }
        face
    }
}
