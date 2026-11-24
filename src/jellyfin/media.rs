use super::*;
use std::path::PathBuf;

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct MediaChoice {
    pub source: Option<String>,
    pub audio: Option<i32>,
    /// A negative index explicitly disables subtitles. An absent value follows Jellyfin.
    pub subtitle: Option<i32>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Chapter {
    pub image_tag: Option<String>,
    pub name: Option<String>,
    pub start_position_ticks: u64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Segment {
    #[serde(rename = "Type")]
    pub kind: String,
    pub start_ticks: u64,
    pub end_ticks: u64,
}
impl Segment {
    pub fn active(&self, seconds: f64) -> bool {
        let at = (seconds * 10_000_000.) as u64;
        self.start_ticks <= at && at < self.end_ticks
    }
}
impl Stream {
    pub fn frame_rate(&self) -> Option<f64> {
        self.average_frame_rate
            .filter(|f| (1.0..=240.0).contains(f))
            .or_else(|| self.real_frame_rate.filter(|f| (1.0..=240.0).contains(f)))
    }

    pub fn bitmap_subtitle(&self) -> bool {
        self.kind == "Subtitle"
            && (self.is_text_subtitle_stream == Some(false)
                || self.codec.as_deref().is_some_and(|codec| {
                    matches!(
                        codec.to_ascii_lowercase().as_str(),
                        "hdmv_pgs_subtitle"
                            | "pgssub"
                            | "pgs"
                            | "dvd_subtitle"
                            | "dvdsub"
                            | "vobsub"
                            | "dvb_subtitle"
                            | "dvbsub"
                            | "xsub"
                    )
                }))
    }

    pub fn subtitle_limitation(&self, native_dolby: bool) -> Option<&'static str> {
        if !self.bitmap_subtitle() {
            return None;
        }
        if native_dolby {
            Some("Bitmap subtitles require a text alternative in Dolby Vision")
        } else if self.is_external {
            Some("External bitmap subtitles are unavailable")
        } else {
            None
        }
    }

    pub fn label(&self) -> String {
        if let Some(name) = &self.display_title {
            return name.clone();
        }
        let parts = [
            self.title.clone(),
            self.language.clone(),
            self.codec.as_ref().map(|s| s.to_uppercase()),
            self.channels.map(|c| format!("{c} ch")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        if parts.is_empty() {
            self.kind.clone()
        } else {
            parts.join(" · ")
        }
    }
}
impl Source {
    pub fn known_native_dolby(&self) -> bool {
        self.media_streams
            .as_deref()
            .and_then(|s| super::route(s).ok())
            .is_some_and(|r| r.native_profile().is_some())
    }
    pub fn frame_rate(&self) -> Option<f64> {
        self.media_streams
            .as_ref()?
            .iter()
            .filter(|s| s.kind == "Video")
            .find_map(Stream::frame_rate)
    }
    pub fn selected_audio(&self, desired: Option<i32>) -> Result<Option<&Stream>> {
        let mut tracks = self
            .media_streams
            .iter()
            .flatten()
            .filter(|s| s.kind == "Audio" && !s.is_external);
        match desired {
            Some(index) => {
                Ok(Some(tracks.find(|s| s.index == Some(index)).context(
                    "This audio track is unavailable in the selected file.",
                )?))
            }
            None => Ok(tracks.min_by_key(|s| {
                let server_default = self.default_audio_stream_index.is_some()
                    && s.index == self.default_audio_stream_index;
                (!server_default, !s.is_default)
            })),
        }
    }
    pub fn name(&self) -> String {
        self.name
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                self.path
                    .as_ref()?
                    .rsplit(['/', '\\'])
                    .next()
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| self.container.clone().unwrap_or_else(|| "Original".into()))
    }
}
impl Item {
    pub fn badges(&self) -> Vec<String> {
        let streams = self
            .media_streams
            .as_deref()
            .or_else(|| {
                self.media_sources
                    .as_ref()?
                    .first()?
                    .media_streams
                    .as_deref()
            })
            .unwrap_or_default();
        let mut labels = Vec::new();
        if let Some(v) = streams.iter().find(|s| s.kind == "Video") {
            if let (Some(width), Some(height)) = (v.width, v.height) {
                labels.push(if width >= 7000 {
                    "8K".into()
                } else if width >= 3800 {
                    "4K".into()
                } else if width >= 1900 {
                    "1080p".into()
                } else {
                    format!("{height}p")
                });
            }
            if v.dv_profile.is_some()
                || v.video_range_type
                    .as_deref()
                    .is_some_and(|v| v.starts_with("DOVI"))
            {
                labels.push("Dolby Vision".into());
            } else if let Some(range) = &v.video_range_type
                && !matches!(range.as_str(), "SDR" | "Unknown")
            {
                labels.push(range.replace("Plus", "+"));
            }
        }
        if let Some(a) = streams.iter().find(|s| s.kind == "Audio")
            && let Some(codec) = &a.codec
        {
            labels.push(if a.label().to_ascii_lowercase().contains("atmos") {
                "Atmos".into()
            } else {
                codec.to_uppercase()
            });
        }
        labels
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Lyric {
    pub start: Option<u64>,
    pub text: String,
}
impl Server {
    pub fn profile_path(&self) -> Result<PathBuf> {
        let a = self.account.as_ref().context("Sign in first.")?;
        crate::preferences::account_path(self.base.as_str(), &a.user)
    }
    pub fn item(&self, id: &str) -> Result<Item> {
        read(
            self.get(&["Items", id])
                .query(&[("userId", self.user_id()?)]),
        )
    }
    pub fn segments(&self, id: &str) -> Result<Vec<Segment>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Segments {
            items: Vec<Segment>,
        }
        let response = self
            .get(&["MediaSegments", id])
            .send()
            .map_err(|e| e.without_url())?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }
        Ok(response
            .error_for_status()?
            .json::<Segments>()?
            .items
            .into_iter()
            .filter(|s| {
                s.end_ticks > s.start_ticks
                    && matches!(
                        s.kind.as_str(),
                        "Intro" | "Outro" | "Recap" | "Preview" | "Commercial"
                    )
            })
            .collect())
    }
    fn subtitle_path<'a>(
        &self,
        item: &'a str,
        source: &'a str,
        index: &'a str,
        file: &'a str,
    ) -> [&'a str; 7] {
        ["Videos", item, source, "Subtitles", index, "0", file]
    }
    pub(super) fn subtitle_url(&self, item: &str, source: &str, stream: &Stream) -> Result<Url> {
        let mut url = self.base.clone();
        let format = match stream.codec.as_deref() {
            Some("ass" | "ssa") => "ass",
            Some("subrip" | "srt") => "srt",
            Some("webvtt" | "vtt") => "vtt",
            _ if stream.is_text_subtitle_stream == Some(true) => "vtt",
            _ => bail!("Unsupported external subtitle format."),
        };
        let index = stream
            .index
            .context("Subtitle stream index is missing.")?
            .to_string();
        url.path_segments_mut()
            .unwrap()
            .pop_if_empty()
            .extend(self.subtitle_path(item, source, &index, &format!("Stream.{format}")));
        url.query_pairs_mut().append_pair(
            "api_key",
            &self.account.as_ref().context("Sign in first.")?.token,
        );
        Ok(url)
    }
    pub fn captions(&self, item: &str, source: &str, index: i32) -> Result<String> {
        use std::io::Read;
        let response = self
            .get(&self.subtitle_path(item, source, &index.to_string(), "Stream.vtt"))
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.without_url())?;
        let mut content = String::new();
        response
            .take(16 * 1024 * 1024 + 1)
            .read_to_string(&mut content)?;
        ensure!(
            content.len() <= 16 * 1024 * 1024,
            "Subtitle file is too large."
        );
        Ok(content)
    }
    pub fn next_episode(&self, item: &Item) -> Result<Option<Item>> {
        let Some(series) = &item.series_id else {
            return Ok(None);
        };
        let p: Page = read(self.get(&["Shows", series, "Episodes"]).query(&[
            ("userId", self.user_id()?),
            ("startItemId", &item.id),
            ("isMissing", "false"),
            ("limit", "2"),
            ("fields", "Overview,Genres"),
        ]))?;
        Ok(p.items
            .windows(2)
            .find(|w| w[0].id == item.id)
            .map(|w| w[1].clone()))
    }

    pub fn episode_neighbors(&self, item: &Item) -> Result<(Option<Item>, Option<Item>)> {
        let Some(series) = &item.series_id else {
            return Ok((None, None));
        };
        let page: Page = read(self.get(&["Shows", series, "Episodes"]).query(&[
            ("userId", self.user_id()?),
            ("adjacentTo", &item.id),
            ("isMissing", "false"),
            ("fields", "Overview,Genres"),
        ]))?;
        let Some(at) = page.items.iter().position(|episode| episode.id == item.id) else {
            return Ok((None, self.next_episode(item)?));
        };
        Ok((
            at.checked_sub(1)
                .and_then(|index| page.items.get(index))
                .cloned(),
            page.items.get(at + 1).cloned(),
        ))
    }
    pub fn lyrics(&self, id: &str) -> Result<Vec<Lyric>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Lyrics {
            lyrics: Vec<Lyric>,
        }
        Ok(read::<Lyrics>(self.get(&["Audio", id, "Lyrics"]))?.lyrics)
    }
    pub fn extras(&self, item: &Item) -> Result<Vec<Item>> {
        let mut all = Vec::new();
        for (count, endpoint) in [
            (item.local_trailer_count, "LocalTrailers"),
            (item.special_feature_count, "SpecialFeatures"),
        ] {
            if count.unwrap_or(0) > 0 {
                all.extend(read::<Vec<Item>>(
                    self.get(&["Items", &item.id, endpoint])
                        .query(&[("userId", self.user_id()?)]),
                )?);
            }
        }
        Ok(all)
    }
}

#[cfg(test)]
mod subtitle_tests {
    use super::*;

    #[test]
    fn codec_only_bitmap_metadata_is_handled_on_the_actual_playback_route() {
        let mut stream = Stream {
            kind: "Subtitle".into(),
            codec: Some("HDMV_PGS_SUBTITLE".into()),
            ..Default::default()
        };
        assert!(stream.subtitle_limitation(true).is_some());
        assert!(stream.subtitle_limitation(false).is_none()); // Includes Profile 7 FEL on mpv.
        stream.codec = Some("subrip".into());
        assert!(stream.subtitle_limitation(true).is_none());
        stream.is_text_subtitle_stream = Some(false);
        assert!(stream.subtitle_limitation(true).is_some());
        stream.is_external = true;
        assert!(stream.subtitle_limitation(false).is_some());
    }
}
