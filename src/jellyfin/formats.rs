use super::*;
use std::collections::BTreeSet;

impl Stream {
    pub(super) fn is_dolby_vision(&self) -> bool {
        let range = self
            .video_range_type
            .as_deref()
            .unwrap_or("")
            .to_ascii_uppercase();
        let tag = self.codec_tag.as_deref().unwrap_or("").to_ascii_lowercase();
        range.starts_with("DOVI")
            || self.dv_profile.is_some()
            || self.dv_version_major.is_some()
            || ["dvhe", "dvh1", "dvav", "dva1", "dav1"]
                .iter()
                .any(|prefix| tag.starts_with(prefix))
            || self.rpu_present_flag == Some(1)
            || self.el_present_flag == Some(1)
    }

    fn resolution(&self) -> Option<String> {
        let width = self.width.unwrap_or(0);
        let height = self.height.unwrap_or(0);
        // Width also identifies cinema crops such as 3840 × 1600.
        let scan = if self.is_interlaced { 'i' } else { 'p' };
        if width >= 7600 || height >= 4320 {
            Some("8K".into())
        } else if width >= 3800 || height >= 2160 {
            Some("4K".into())
        } else if height >= 1440 {
            Some(format!("{height}{scan}"))
        } else if width >= 1900 || height == 1080 {
            Some(format!("1080{scan}"))
        } else if (width == 1280 && height <= 720) || height == 720 {
            Some(format!("720{scan}"))
        } else if height > 0 {
            Some(format!("{height}{scan}"))
        } else {
            None
        }
    }

    fn range(&self) -> Option<String> {
        if self.is_dolby_vision() {
            return Some("DV".into());
        }
        let range = [&self.video_range_type, &self.video_range]
            .into_iter()
            .flatten()
            .map(|s| s.trim())
            .find(|s| !s.is_empty() && !s.eq_ignore_ascii_case("unknown"))?;
        Some(match range.to_ascii_uppercase().as_str() {
            "HDR10PLUS" | "HDR10+" => "HDR10+".into(),
            "DOLBYVISION" | "DOLBY VISION" => "DV".into(),
            other => other.to_owned(),
        })
    }

    fn codec_name(&self) -> Option<String> {
        let codec = self.codec.as_ref()?.trim().to_ascii_uppercase();
        match codec.as_str() {
            "" | "UNKNOWN" => None,
            "H264" | "H.264" | "AVC1" => Some("AVC".into()),
            "H265" | "H.265" | "HEV1" | "HVC1" => Some("HEVC".into()),
            "AV01" => Some("AV1".into()),
            _ => Some(codec),
        }
    }
}

#[derive(Clone, Default)]
pub struct Formats {
    fields: [BTreeSet<String>; 3],
    // Only label an entire group with a value when all files report that field.
    partial: [bool; 3],
}

impl Formats {
    pub fn from_streams(streams: &[Stream], audio_only: bool) -> Self {
        let video = streams
            .iter()
            .find(|s| s.kind == "Video" && !audio_only && !s.is_attached_pic);
        let audio = streams.iter().find(|s| s.kind == "Audio");
        let values = [
            video.and_then(Stream::resolution),
            video.and_then(Stream::range),
            video
                .or_else(|| audio.filter(|_| audio_only))
                .and_then(Stream::codec_name),
        ];
        Self {
            partial: values.each_ref().map(Option::is_none),
            fields: values.map(|v| v.into_iter().collect()),
        }
    }

    pub fn merge(&mut self, other: Self) {
        for (i, values) in other.fields.into_iter().enumerate() {
            self.fields[i].extend(values);
            self.partial[i] |= other.partial[i];
        }
    }

    pub fn mixed(&self) -> bool {
        self.fields.iter().any(|v| v.len() > 1)
    }

    pub fn labels(&self) -> impl Iterator<Item = &str> {
        self.fields
            .iter()
            .zip(self.partial)
            .filter(|(_, partial)| !partial)
            .filter_map(|(values, _)| values.first().map(String::as_str))
    }

    pub fn details(&self) -> impl Iterator<Item = (&'static str, String)> + '_ {
        ["Resolution", "Range", "Codec"]
            .into_iter()
            .enumerate()
            .filter_map(|(i, name)| {
                let values = &self.fields[i];
                if values.is_empty() {
                    return None;
                }
                let mut text = values
                    .iter()
                    .map(|s| {
                        if s == "DV" {
                            "Dolby Vision"
                        } else {
                            s.as_str()
                        }
                    })
                    .collect::<Vec<_>>();
                if self.partial[i] {
                    text.push("Unknown");
                }
                Some((name, text.join(" / ")))
            })
    }
}

impl Item {
    pub fn format_group(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "Series" | "Season" | "BoxSet" | "Playlist" | "MusicAlbum"
        )
    }

    pub fn detail_source(&self, selected: Option<&str>) -> Option<&Source> {
        let sources = self.media_sources.as_deref().unwrap_or_default();
        match selected {
            Some(id) => sources.iter().find(|s| s.id == id),
            None => sources
                .iter()
                .find(|s| s.supports_direct_play)
                .or_else(|| sources.first()),
        }
    }

    pub fn file_formats(&self, selected: Option<&str>) -> Formats {
        let sources = self.media_sources.as_deref().unwrap_or_default();
        let source = self.detail_source(selected);
        let streams = source
            .and_then(|s| s.media_streams.as_deref())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let is_default = source
                    .zip(sources.first())
                    .is_some_and(|(s, first)| s.id == first.id);
                if sources.is_empty() && selected.is_none() || is_default {
                    self.media_streams.as_deref()
                } else {
                    None
                }
            });
        Formats::from_streams(streams.unwrap_or_default(), self.kind == "Audio")
    }
}

impl Server {
    fn preferred_file_formats(&self, item: &Item) -> Result<Formats> {
        let selected = if item.media_sources.as_ref().is_some_and(|s| s.len() > 1) {
            crate::preferences::load::<MediaChoice>(&crate::preferences::item_path(
                &self.profile_path()?,
                &item.id,
            ))?
            .source
        } else {
            None
        };
        Ok(item.file_formats(selected.as_deref()))
    }

    pub fn card_formats(&self, item: &Item) -> Result<Formats> {
        if !item.format_group() {
            return self.preferred_file_formats(item);
        }
        let mut formats = Formats::default();
        let mut offset = 0;
        loop {
            let request = if item.kind == "Playlist" {
                self.get(&["Playlists", &item.id, "Items"])
            } else {
                self.get(&["Items"]).query(&[
                    ("parentId", item.id.as_str()),
                    ("recursive", "true"),
                    (
                        "includeItemTypes",
                        "Episode,Movie,Video,MusicVideo,Trailer,Audio",
                    ),
                ])
            };
            let page: Page = read(request.query(&[
                ("userId", self.user_id()?),
                ("fields", "MediaSources"),
                ("enableImages", "false"),
                ("enableUserData", "false"),
                ("limit", "64"),
                ("startIndex", &offset.to_string()),
            ]))?;
            offset += page.items.len();
            for child in &page.items {
                if child.playable() {
                    formats.merge(self.preferred_file_formats(child)?);
                }
            }
            if page.items.is_empty() || offset >= page.total_record_count {
                return Ok(formats);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(mut fields: serde_json::Value) -> Stream {
        fields["Type"] = json!("Video");
        serde_json::from_value(fields).unwrap()
    }

    #[test]
    fn cropped_video_and_sdr_use_source_metadata() {
        let film = video(
            json!({"Width":3840,"Height":1600,"VideoRangeType":"DOVIWithHDR10","Codec":"h265"}),
        );
        assert_eq!(
            Formats::from_streams(&[film], false)
                .labels()
                .collect::<Vec<_>>(),
            ["4K", "DV", "HEVC"]
        );
        let tv = video(
            json!({"Width":1920,"Height":1080,"IsInterlaced":true,"VideoRangeType":"Unknown","VideoRange":"SDR","Codec":"h264"}),
        );
        assert_eq!(
            Formats::from_streams(&[tv], false)
                .labels()
                .collect::<Vec<_>>(),
            ["1080i", "SDR", "AVC"]
        );
    }

    #[test]
    fn mixed_series_collects_real_values_and_leaves_unknown_fields_unlabelled() {
        let file = |range: &str, codec: &str| {
            Formats::from_streams(
                &[video(
                    json!({"Height":2160,"VideoRangeType":range,"Codec":codec}),
                )],
                false,
            )
        };
        let mut show = file("HDR10Plus", "hevc");
        show.merge(file("HDR10+", "h265"));
        assert!(!show.mixed());
        show.merge(file("Unknown", "hevc"));
        assert_eq!(show.labels().collect::<Vec<_>>(), ["4K", "HEVC"]);
        show.merge(file("DOVI", "av01"));
        assert!(show.mixed());
        assert_eq!(
            show.details().collect::<Vec<_>>(),
            [
                ("Resolution", "4K".into()),
                ("Range", "Dolby Vision / HDR10+ / Unknown".into()),
                ("Codec", "AV1 / HEVC".into())
            ]
        );
    }

    #[test]
    fn edition_selection_and_audio_cover_art_use_the_correct_stream() {
        let movie: Item = serde_json::from_value(json!({"Id":"movie","Name":"movie","Type":"Movie",
            "MediaSources":[{"Id":"hd","MediaStreams":[{"Type":"Video","Height":1080,"Codec":"avc"}]},
                {"Id":"uhd","MediaStreams":[{"Type":"Video","Height":2160,"Codec":"av1"}]}]})).unwrap();
        assert_eq!(
            movie.file_formats(Some("uhd")).labels().collect::<Vec<_>>(),
            ["4K", "AV1"]
        );
        assert!(
            movie
                .file_formats(Some("missing"))
                .labels()
                .next()
                .is_none()
        );
        let tracks = [
            video(json!({"IsAttachedPic":true,"Height":720,"Codec":"mjpeg"})),
            serde_json::from_value(json!({"Type":"Audio","Codec":"flac"})).unwrap(),
        ];
        assert_eq!(
            Formats::from_streams(&tracks, true)
                .labels()
                .collect::<Vec<_>>(),
            ["FLAC"]
        );
    }
}
