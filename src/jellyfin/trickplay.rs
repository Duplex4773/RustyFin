use super::{Item, Server};
use anyhow::{Context, Result, ensure};
use image::{ImageFormat, ImageReader, RgbImage};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{Cursor, Read},
};

pub type Manifest = HashMap<String, Option<HashMap<String, Info>>>;

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Info {
    width: i32,
    height: i32,
    tile_width: i32,
    tile_height: i32,
    thumbnail_count: i32,
    interval: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sheet {
    pub width: u32,
    pub index: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Frame {
    pub sheet: Sheet,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl Info {
    fn frame(&self, seconds: f64) -> Option<Frame> {
        if !seconds.is_finite()
            || [
                self.width,
                self.height,
                self.tile_width,
                self.tile_height,
                self.thumbnail_count,
                self.interval,
            ]
            .into_iter()
            .any(|v| v <= 0)
        {
            return None;
        }
        let w = self.width as u32;
        let h = self.height as u32;
        let cols = self.tile_width as u32;
        let per_sheet = cols.checked_mul(self.tile_height as u32)?;
        let index = ((seconds.max(0.) * 1000. / f64::from(self.interval)) as u32)
            .min(self.thumbnail_count as u32 - 1);
        let cell = index % per_sheet;
        Some(Frame {
            sheet: Sheet {
                width: w,
                index: index / per_sheet,
            },
            x: (cell % cols).checked_mul(w)?,
            y: (cell / cols).checked_mul(h)?,
            width: w,
            height: h,
        })
    }
}

impl Item {
    pub fn trickplay_frame(&self, source: &str, seconds: f64, target_width: u32) -> Option<Frame> {
        let manifest = self.trickplay.as_ref()?;
        let versions = manifest
            .get(source)
            .or_else(|| {
                let id = uuid::Uuid::parse_str(source).ok()?;
                manifest
                    .iter()
                    .find_map(|(k, v)| (uuid::Uuid::parse_str(k).ok() == Some(id)).then_some(v))
            })?
            .as_ref()?;
        versions
            .values()
            .filter_map(|info| info.frame(seconds))
            .min_by_key(|f| (f.width < target_width, f.width.abs_diff(target_width)))
    }
}

impl Server {
    pub fn trickplay_sheet(&self, item: &str, source: &str, sheet: Sheet) -> Result<RgbImage> {
        let response = self
            .get(&[
                "Videos",
                item,
                "Trickplay",
                &sheet.width.to_string(),
                &format!("{}.jpg", sheet.index),
            ])
            .query(&[("mediaSourceId", source)])
            .send()?
            .error_for_status()?;
        const MAX_BYTES: u64 = 24 * 1024 * 1024;
        let mut bytes = Vec::new();
        response.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_BYTES,
            "Trickplay sheet exceeds the download limit."
        );
        let reader = || ImageReader::with_format(Cursor::new(&bytes), ImageFormat::Jpeg);
        let (w, h) = reader().into_dimensions()?;
        ensure!(
            u64::from(w) * u64::from(h) <= 24 * 1024 * 1024,
            "Trickplay sheet exceeds the decoded image limit."
        );
        Ok(reader().decode()?.into_rgb8())
    }
}

impl Frame {
    pub fn crop(&self, image: &RgbImage) -> Result<image::Frame> {
        ensure!(
            self.x
                .checked_add(self.width)
                .context("Invalid Trickplay column")?
                <= image.width()
                && self
                    .y
                    .checked_add(self.height)
                    .context("Invalid Trickplay row")?
                    <= image.height(),
            "Missing cell in Trickplay sheet."
        );
        let cropped = image::imageops::crop_imm(image, self.x, self.y, self.width, self.height);
        let bgra = image::RgbaImage::from_fn(self.width, self.height, |x, y| {
            use image::GenericImageView;
            let [r, g, b] = cropped.get_pixel(x, y).0;
            image::Rgba([b, g, r, 255])
        });
        Ok(image::Frame::new(bgra))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_specific_tiles_and_partial_final_sheet() {
        let item: Item = serde_json::from_value(serde_json::json!({
            "Id":"movie","Name":"Movie","Trickplay":{"fc087eef-622e-492f-82ab-1f58aa4996db":{
                "2":{"Width":2,"Height":1,"TileWidth":2,"TileHeight":2,"Interval":1000,"ThumbnailCount":6},
                "4":{"Width":4,"Height":1,"TileWidth":2,"TileHeight":2,"Interval":1000,"ThumbnailCount":6}
            }}
        })).unwrap();
        let frame = |t| {
            item.trickplay_frame("fc087eef622e492f82ab1f58aa4996db", t, 2)
                .unwrap()
        };
        assert!(item.trickplay_frame("other", 0., 2).is_none());
        assert_eq!((frame(2.9).x, frame(2.9).y), (0, 1));
        let last = frame(60.);
        assert_eq!((last.sheet.index, last.x, last.y), (1, 2, 0));
        let sheet = RgbImage::from_fn(4, 1, |x, _| image::Rgb([x as u8, 9, 17]));
        assert_eq!(
            last.crop(&sheet).unwrap().buffer().get_pixel(0, 0).0,
            [17, 9, 2, 255]
        );
        assert!(frame(2.).crop(&sheet).is_err());
        assert_eq!(
            item.trickplay_frame("fc087eef622e492f82ab1f58aa4996db", 0., 3)
                .unwrap()
                .sheet
                .width,
            4
        );
    }
}
