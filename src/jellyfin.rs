use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use reqwest::blocking::{Client, RequestBuilder, Response};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use url::Url;

mod catalog;
pub mod discovery;
pub mod edit;
pub mod federation;
pub mod formats;
pub mod live;
mod media;
pub mod offline;
pub mod trickplay;
pub use catalog::catalog_initial;
pub use catalog::{Art, ArtKind, CatalogQuery, LIBRARY_PAGE_SIZE, PAGE_SIZE, Shelf, ShelfKind};
pub use media::{Chapter, ExtraKind, Lyric, MediaChoice, Segment};

#[derive(Clone, Deserialize, Serialize)]
pub struct Account {
    pub server: String,
    pub device: String,
    pub user: String,
    #[serde(default)]
    pub username: Option<String>,
    pub token: String,
}

#[derive(Clone)]
pub struct Server {
    base: Url,
    http: Client,
    device: String,
    account: Option<Account>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Pairing {
    pub code: String,
    pub secret: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Page {
    pub items: Vec<Item>,
    pub total_record_count: usize,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Item {
    #[serde(skip)]
    pub origin: Option<String>,
    #[serde(skip)]
    pub copies: Vec<Item>,
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_name: Option<String>,
    #[serde(default, rename = "Type")]
    pub kind: String,
    pub can_download: Option<bool>,
    pub provider_ids: Option<HashMap<String, String>>,
    pub official_rating: Option<String>,
    pub community_rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub critic_rating: Option<f64>,
    pub genres: Option<Vec<String>>,
    pub media_streams: Option<Vec<Stream>>,
    pub media_sources: Option<Vec<Source>>,
    pub chapters: Option<Vec<Chapter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trickplay: Option<trickplay::Manifest>,
    pub people: Option<Vec<Person>>,
    pub current_program: Option<Box<Item>>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub timer_id: Option<String>,
    pub series_timer_id: Option<String>,
    pub is_series: Option<bool>,
    pub channel_number: Option<String>,
    pub local_trailer_count: Option<u32>,
    pub special_feature_count: Option<u32>,
    pub series_id: Option<String>,
    pub season_id: Option<String>,
    pub album_id: Option<String>,
    pub album: Option<String>,
    pub artists: Option<Vec<String>>,
    pub album_primary_image_tag: Option<String>,
    pub series_primary_image_tag: Option<String>,
    pub parent_backdrop_item_id: Option<String>,
    pub parent_backdrop_image_tags: Option<Vec<String>>,
    pub backdrop_image_tags: Option<Vec<String>>,
    pub parent_logo_item_id: Option<String>,
    pub parent_logo_image_tag: Option<String>,
    pub collection_type: Option<String>,
    pub series_name: Option<String>,
    pub parent_index_number: Option<u32>,
    pub index_number: Option<u32>,
    pub production_year: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_last_media_added: Option<String>,
    pub overview: Option<String>,
    pub run_time_ticks: Option<u64>,
    pub image_tags: Option<HashMap<String, String>>,
    pub user_data: Option<UserData>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unplayed_item_count: Option<u32>,
    pub playback_position_ticks: Option<u64>,
    pub played: Option<bool>,
    pub is_favorite: Option<bool>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Person {
    pub id: String,
    pub name: String,
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_image_tag: Option<String>,
    #[serde(rename = "Type")]
    pub kind: Option<String>,
}

impl Item {
    pub fn caption(&self) -> String {
        if let Some(series) = &self.series_name {
            return match (self.parent_index_number, self.index_number) {
                (Some(s), Some(e)) => format!("{series}  ·  S{s} E{e}"),
                _ => series.clone(),
            };
        }
        [
            self.production_year.map(|year| year.to_string()),
            self.run_time_ticks
                .map(|ticks| format!("{} min", ticks / 600_000_000)),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("  ·  ")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PlaybackInfo {
    media_sources: Vec<Source>,
    play_session_id: String,
}

fn direct_play_profile() -> serde_json::Value {
    // Jellyfin evaluates the selected subtitle when deciding whether it can
    // serve the original file. A forced text track must be playable as-is.
    json!({
        "Name": "RustyFin",
        "MaxStreamingBitrate": i32::MAX,
        "MaxStaticBitrate": i32::MAX,
        "DirectPlayProfiles": [{"Type": "Video"}, {"Type": "Audio"}],
        "SubtitleProfiles": [
            {"Format": "subrip", "Method": "Embed"},
            {"Format": "srt", "Method": "Embed"},
            {"Format": "ass", "Method": "Embed"},
            {"Format": "ssa", "Method": "Embed"},
            {"Format": "webvtt", "Method": "Embed"},
            {"Format": "vtt", "Method": "Embed"},
            {"Format": "mov_text", "Method": "Embed"},
            {"Format": "srt", "Method": "External"},
            {"Format": "ass", "Method": "External"},
            {"Format": "vtt", "Method": "External"}
        ]
    })
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Source {
    pub id: String,
    pub name: Option<String>,
    pub path: Option<String>,
    pub size: Option<u64>,
    pub bitrate: Option<u64>,
    pub run_time_ticks: Option<u64>,
    pub supports_direct_play: bool,
    #[serde(skip_serializing)]
    pub open_token: Option<String>,
    #[serde(skip_serializing)]
    pub live_stream_id: Option<String>,
    pub requires_opening: bool,
    pub is_infinite_stream: bool,
    pub default_subtitle_stream_index: Option<i32>,
    pub container: Option<String>,
    pub media_streams: Option<Vec<Stream>>,
    pub default_audio_stream_index: Option<i32>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Stream {
    pub display_title: Option<String>,
    pub title: Option<String>,
    pub language: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub channels: Option<u32>,
    pub average_frame_rate: Option<f64>,
    pub real_frame_rate: Option<f64>,
    pub is_external: bool,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
    pub is_text_subtitle_stream: Option<bool>,
    pub supports_external_stream: Option<bool>,
    #[serde(rename = "Type")]
    pub kind: String,
    pub index: Option<i32>,
    pub codec: Option<String>,
    codec_tag: Option<String>,
    pub video_range_type: Option<String>,
    pub video_range: Option<String>,
    pub is_interlaced: Option<bool>,
    pub is_anamorphic: Option<bool>,
    pub is_attached_pic: bool,
    pub profile: Option<String>,
    pub level: Option<f64>,
    #[serde(rename = "BitRate", alias = "Bitrate")]
    pub bitrate: Option<u64>,
    pub bit_depth: Option<u32>,
    pub pixel_format: Option<String>,
    pub aspect_ratio: Option<String>,
    pub color_space: Option<String>,
    pub color_primaries: Option<String>,
    pub color_transfer: Option<String>,
    pub channel_layout: Option<String>,
    pub sample_rate: Option<u32>,
    pub audio_spatial_format: Option<String>,
    pub time_base: Option<String>,
    pub dv_profile: Option<u8>,
    dv_version_major: Option<u8>,
    rpu_present_flag: Option<i32>,
    el_present_flag: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Route {
    Mpv,
    MpvDolby,
    MpvProfile7 {
        fel: bool,
    },
    Dolby(String),
    #[serde(alias = "DolbyMel81")]
    Profile7,
    Mel81,
    FelHdr10,
    MelHdr10,
    Sdr {
        source: HdrSource,
        hdr_available: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HdrSource {
    Hdr,
    DolbyVision,
    Profile7 { fel: bool },
    Profile7Base,
}

impl Route {
    pub fn startup_notice(&self) -> Option<&'static str> {
        match self {
            Self::Mel81 => Some("Dolby Vision · Profile 7 MEL → 8.1"),
            Self::MelHdr10 => Some("Profile 7 MEL · Playing HDR10"),
            Self::MpvProfile7 { fel: true } => Some("Dolby Vision Profile 7 · FEL reconstruction"),
            Self::FelHdr10 => Some("Profile 7 · HDR10 base layer fallback"),
            Self::Sdr {
                source,
                hdr_available,
            } => Some(match (source, hdr_available) {
                (HdrSource::Hdr, false) => "HDR video tone-mapped to SDR",
                (HdrSource::Hdr, true) => {
                    "HDR video tone-mapped to SDR\nHDR is available for this display in Windows Settings."
                }
                (HdrSource::Profile7Base, false) => {
                    "Profile 7 · HDR10 base layer tone-mapped to SDR"
                }
                (HdrSource::Profile7Base, true) => {
                    "Profile 7 · HDR10 base layer tone-mapped to SDR\nHDR is available for this display in Windows Settings."
                }
                (HdrSource::Profile7 { fel: true }, false) => "Profile 7 FEL · Tone-mapped to SDR",
                (HdrSource::Profile7 { fel: true }, true) => {
                    "Profile 7 FEL · Tone-mapped to SDR\nHDR is available for this display in Windows Settings."
                }
                (_, false) => "Dolby Vision video tone-mapped to SDR",
                (_, true) => {
                    "Dolby Vision video tone-mapped to SDR\nHDR is available for this display in Windows Settings."
                }
            }),
            _ => None,
        }
    }

    pub fn native_profile(&self) -> Option<&str> {
        match self {
            Self::Dolby(profile) => Some(profile),
            Self::Mel81 => Some("dvhe.08"),
            _ => None,
        }
    }

    pub fn mpv_profile7(&self) -> Option<bool> {
        match self {
            Self::MpvProfile7 { fel }
            | Self::Sdr {
                source: HdrSource::Profile7 { fel },
                ..
            } => Some(*fel),
            _ => None,
        }
    }

    pub fn hdr10_base_layer(&self) -> bool {
        matches!(
            self,
            Self::FelHdr10
                | Self::MelHdr10
                | Self::Sdr {
                    source: HdrSource::Profile7Base,
                    ..
                }
        )
    }

    pub fn profile7_fallback(&self) -> Option<Self> {
        self.mpv_profile7()?;
        Some(match self {
            Self::Sdr { hdr_available, .. } => Self::Sdr {
                source: HdrSource::Profile7Base,
                hdr_available: *hdr_available,
            },
            _ => Self::FelHdr10,
        })
    }
}

fn route(streams: &[Stream]) -> Result<Route> {
    let video: Vec<_> = streams.iter().filter(|s| s.kind == "Video").collect();
    ensure!(
        !video.is_empty(),
        "Jellyfin has no video stream details for this source. Refresh its media information and retry."
    );
    for s in &video {
        if !s.is_dolby_vision() {
            continue;
        }
        let range = s
            .video_range_type
            .as_deref()
            .unwrap_or("")
            .to_ascii_uppercase();
        ensure!(
            video.len() == 1,
            "This source contains multiple video tracks. Native Dolby Vision track selection is unavailable."
        );
        ensure!(
            range != "DOVIINVALID" && s.rpu_present_flag != Some(0),
            "Jellyfin reports invalid or missing Dolby Vision metadata."
        );
        let profile = s.dv_profile.context("Jellyfin identified Dolby Vision but did not report a profile. Refresh this title’s media information.")?;
        let family = match s
            .codec
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "hevc" | "h265" => "dvhe",
            "h264" | "avc" => "dvav",
            other => bail!("Native Dolby Vision playback for the {other} codec is unsupported."),
        };
        if profile == 7 {
            ensure!(
                family == "dvhe" && s.el_present_flag != Some(0),
                "Invalid Dolby Vision Profile 7 stream configuration."
            );
            return Ok(Route::Profile7);
        }
        ensure!(
            s.el_present_flag != Some(1) && !range.contains("WITHEL"),
            "Dolby Vision profile {profile} with an enhancement layer is unsupported."
        );
        return Ok(Route::Dolby(format!("{family}.{profile:02}")));
    }
    // A legacy server can return HEVC with no dynamic range analysis at all.
    // Avoid sending an unidentified Dolby-capable stream through mpv.
    for s in video {
        ensure!(
            s.video_range_type
                .as_deref()
                .is_some_and(|v| !v.is_empty() && !v.eq_ignore_ascii_case("Unknown"))
                || matches!(
                    s.codec.as_deref(),
                    Some("mpeg2video" | "mpeg4" | "vp8" | "vp9" | "vc1")
                ),
            "Jellyfin has not identified the video range. Refresh this title’s media information before playback."
        );
    }
    Ok(Route::Mpv)
}

#[derive(Clone)]
pub struct PlayRequest {
    pub route: Route,
    pub url: Url,
    pub source: Source,
    pub session: String,
    pub item: String,
    pub resume: f64,
    pub format: String,
    pub audio_index: Option<i32>,
    pub subtitle_index: Option<i32>,
    pub subtitle_track: Option<Stream>,
    pub subtitle_size: u32,
    pub external_subtitle: Option<Url>,
    pub allow_passthrough: bool,
    pub start_paused: bool,
    pub audio_only: bool,
    pub frame_rate: Option<f64>,
    pub seekable: bool,
    pub _live: Option<std::sync::Arc<live::LiveStream>>,
}

impl PlayRequest {
    pub fn tone_map_to_sdr(&mut self, hdr_available: bool) {
        let source = match self.route {
            Route::Dolby(_) | Route::MpvDolby => HdrSource::DolbyVision,
            Route::MpvProfile7 { fel } => HdrSource::Profile7 { fel },
            Route::Profile7 | Route::Mel81 => HdrSource::Profile7 { fel: false },
            Route::FelHdr10 | Route::MelHdr10 => HdrSource::Profile7Base,
            Route::Mpv if !self.audio_only && self.source.has_hdr_video() => HdrSource::Hdr,
            Route::Mpv | Route::Sdr { .. } => return,
        };
        self.route = Route::Sdr {
            source,
            hdr_available,
        };
    }
}

impl Server {
    pub fn new(address: &str, device: String) -> Result<Self> {
        let mut base = Url::parse(address.trim())
            .context("Enter a full server address, including http:// or https://.")?;
        ensure!(
            matches!(base.scheme(), "http" | "https")
                && base.host().is_some()
                && base.username().is_empty()
                && base.password().is_none()
                && base.query().is_none()
                && base.fragment().is_none(),
            "Enter an HTTP or HTTPS server address with an optional base path."
        );
        base.path_segments_mut().unwrap().pop_if_empty().push("");
        let http = Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if let Some(first) = attempt.previous().first()
                    && first.path().ends_with("/Users/AuthenticateByName")
                    && first.origin() != attempt.url().origin()
                {
                    // A 307 or 308 can replay the password in the request body.
                    return attempt.stop();
                }
                reqwest::redirect::Policy::limited(10).redirect(attempt)
            }))
            .build()?;
        Ok(Self {
            base,
            http,
            device,
            account: None,
        })
    }

    pub fn identity(&self) -> Option<&Account> {
        self.account.as_ref()
    }
    pub fn restore(account: Account) -> Result<Self> {
        let mut server = Self::new(&account.server, account.device.clone())?;
        server.account = Some(account);
        Ok(server)
    }

    fn request(&self, method: reqwest::Method, segments: &[&str]) -> RequestBuilder {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .unwrap()
            .pop_if_empty()
            .extend(segments.iter().copied());
        let mut auth = format!(
            r#"MediaBrowser Client="RustyFin", Device="Windows", DeviceId="{}", Version="{}""#,
            self.device,
            env!("CARGO_PKG_VERSION")
        );
        if let Some(a) = &self.account {
            auth.push_str(&format!(r#", Token="{}""#, a.token));
        }
        self.http.request(method, url).header("Authorization", auth)
    }

    fn get(&self, path: &[&str]) -> RequestBuilder {
        self.request(reqwest::Method::GET, path)
    }
    fn post(&self, path: &[&str]) -> RequestBuilder {
        self.request(reqwest::Method::POST, path)
    }

    pub fn pair(&self) -> Result<Option<Pairing>> {
        let response = self
            .post(&["QuickConnect", "Initiate"])
            .send()
            .map_err(|e| e.without_url())
            .context("Could not reach Jellyfin for Quick Connect.")?;
        if matches!(response.status().as_u16(), 401 | 403 | 404 | 405 | 501) {
            return Ok(None);
        }
        read_response(response)
            .map(Some)
            .context("Quick Connect could not start. Check the server address.")
    }

    pub fn authenticate_by_name(&self, username: &str, password: &str) -> Result<Account> {
        let response = self
            .post(&["Users", "AuthenticateByName"])
            .json(&json!({ "Username": username, "Pw": password }))
            .send()
            .map_err(|e| e.without_url())
            .context("Could not reach Jellyfin to sign in.")?;
        ensure!(
            !response.status().is_redirection(),
            "Sign-in was redirected. Enter the final Jellyfin server address."
        );
        ensure!(
            !matches!(response.status().as_u16(), 401 | 403),
            "Jellyfin declined sign-in. Check your username and password or contact your server administrator."
        );
        self.authenticated_account(response)
    }

    pub fn claim(&self, secret: &str) -> Result<Option<Account>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Status {
            authenticated: bool,
        }
        let status: Status = read(
            self.get(&["QuickConnect", "Connect"])
                .query(&[("secret", secret)]),
        )?;
        if !status.authenticated {
            return Ok(None);
        }
        let response = self
            .post(&["Users", "AuthenticateWithQuickConnect"])
            .json(&json!({"Secret": secret}))
            .send()
            .map_err(|e| e.without_url())?;
        self.authenticated_account(response).map(Some)
    }

    fn authenticated_account(&self, response: Response) -> Result<Account> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Identity {
            id: String,
            name: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Auth {
            user: Identity,
            access_token: String,
        }
        let auth: Auth = read_response(response)?;
        Ok(Account {
            server: self.base.to_string(),
            device: self.device.clone(),
            user: auth.user.id,
            username: auth.user.name,
            token: auth.access_token,
        })
    }

    pub fn playback(&self, item: &Item, choice: &MediaChoice) -> Result<PlayRequest> {
        let account = self.account.as_ref().context("Sign in before playback.")?;
        let mut request = self.post(&["Items", &item.id, "PlaybackInfo"]);
        if let Some(id) = &choice.source {
            request = request.query(&[("mediaSourceId", id)]);
        }
        let info: PlaybackInfo = read(request.json(&json!({
            "UserId": account.user, "EnableDirectPlay": true, "EnableDirectStream": false,
            "EnableTranscoding": false, "MaxStreamingBitrate": i32::MAX,
            "DeviceProfile": direct_play_profile()
        })))?;
        let source =
            match &choice.source {
                Some(id) => info.media_sources.iter().find(|s| &s.id == id).context(
                    "The selected version is no longer available. Choose another version.",
                )?,
                None => info
                    .media_sources
                    .iter()
                    .find(|s| s.supports_direct_play)
                    .or_else(|| info.media_sources.first())
                    .context("Jellyfin supplied no media sources.")?,
            };
        let (source, lease) = if source.requires_opening {
            self.open_tuner(item, source, &info.play_session_id)?
        } else {
            (source.clone(), None)
        };
        ensure!(
            source.supports_direct_play,
            "Jellyfin does not allow Direct Play for this source."
        );
        let streams = source.media_streams.as_deref().unwrap_or_default();
        let music = matches!(item.kind.as_str(), "Audio" | "TvChannel")
            && streams.iter().any(|s| s.kind == "Audio")
            && !streams.iter().any(|s| s.kind == "Video");
        let route = if music { Route::Mpv } else { route(streams)? };
        let container = source
            .container
            .as_deref()
            .and_then(|s| s.split(',').next())
            .filter(|s| !s.is_empty())
            .context("Missing container")?;
        let mut url = self.base.clone();
        url.path_segments_mut().unwrap().pop_if_empty().extend([
            if item.kind == "Audio" {
                "Audio"
            } else {
                "Videos"
            },
            &item.id,
            &format!("stream.{container}"),
        ]);
        // Media Foundation opens its own HTTP connections, including byte-range requests.
        url.query_pairs_mut().extend_pairs([
            ("static", "true"),
            ("mediaSourceId", &source.id),
            ("playSessionId", &info.play_session_id),
            ("deviceId", &self.device),
            ("api_key", &account.token),
        ]);
        if let Some(id) = &source.live_stream_id {
            url.query_pairs_mut().append_pair("liveStreamId", id);
        }
        let audio = source.selected_audio(choice.audio)?;
        let sub_index = choice
            .subtitle
            .or(source.default_subtitle_stream_index)
            .unwrap_or(-1);
        let subtitle = streams
            .iter()
            .find(|s| s.kind == "Subtitle" && s.index == Some(sub_index));
        if choice.subtitle.is_some_and(|i| i >= 0) {
            ensure!(
                subtitle.is_some(),
                "The selected subtitle track is unavailable."
            );
        }
        let external_subtitle = match subtitle {
            Some(s) if s.is_external && !s.bitmap_subtitle() => {
                Some(self.subtitle_url(&item.id, &source.id, s)?)
            }
            _ => None,
        };
        let format = format!(
            "{} · {}",
            container.to_ascii_uppercase(),
            audio
                .and_then(|s| s.codec.as_deref())
                .unwrap_or("no audio")
                .to_ascii_uppercase()
        );
        Ok(PlayRequest {
            url,
            route,
            frame_rate: source.frame_rate(),
            seekable: !source.is_infinite_stream && item.kind != "TvChannel",
            session: info.play_session_id,
            item: item.id.clone(),
            allow_passthrough: true,
            start_paused: false,
            audio_only: music,
            _live: lease,
            format,
            audio_index: audio.and_then(|s| s.index),
            subtitle_size: 36,
            subtitle_index: subtitle.and_then(|s| s.index),
            subtitle_track: subtitle.cloned(),
            source,
            external_subtitle,
            resume: item
                .user_data
                .as_ref()
                .and_then(|u| u.playback_position_ticks)
                .unwrap_or(0) as f64
                / 10_000_000.0,
        })
    }

    pub fn report(
        &self,
        event: &str,
        ids: &(String, String, String),
        position: f64,
        paused: bool,
        seekable: bool,
    ) -> Result<()> {
        let mut path = vec!["Sessions", "Playing"];
        if !event.is_empty() {
            path.push(event);
        }
        self.post(&path).json(&json!({ "ItemId": ids.0, "MediaSourceId": ids.1, "PlaySessionId": ids.2,
            "PositionTicks": (position * 10_000_000.0) as u64, "IsPaused": paused, "PlayMethod": "DirectPlay", "CanSeek": seekable }))
            .send().and_then(|r| r.error_for_status()).map_err(|e| e.without_url())?;
        Ok(())
    }
}

fn read<T: DeserializeOwned>(request: RequestBuilder) -> Result<T> {
    read_response(request.send().map_err(|e| e.without_url())?)
}

fn read_response<T: DeserializeOwned>(response: Response) -> Result<T> {
    let response = response.error_for_status().map_err(|e| e.without_url())?;
    Ok(response.json().map_err(|e| e.without_url())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn disabled_quick_connect_falls_back_to_password_authentication() {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/jellyfin/", listener.local_addr().unwrap());
        let server = Server::new(&base, "test-device".into()).unwrap();
        let fake_api = std::thread::spawn(move || {
            let password_endpoint = "/jellyfin/Users/AuthenticateByName";
            let account = r#"{"User":{"Id":"user-id","Name":"Viewer"},"AccessToken":"token"}"#;
            for (endpoint, status, reply, password) in [
                (
                    "/jellyfin/QuickConnect/Initiate",
                    "401 Unauthorized",
                    "",
                    None,
                ),
                (password_endpoint, "401 Unauthorized", "", Some("incorrect")),
                (password_endpoint, "200 OK", account, Some("")),
                (
                    password_endpoint,
                    "307 Temporary Redirect",
                    "",
                    Some("another"),
                ),
            ] {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(&socket);
                let mut headers = String::new();
                while !headers.ends_with("\r\n\r\n") {
                    assert_ne!(reader.read_line(&mut headers).unwrap(), 0);
                }
                assert!(headers.starts_with(&format!("POST {endpoint} HTTP/")));
                assert!(headers.contains(r#"DeviceId="test-device""#));
                if let Some(password) = password {
                    let length = headers
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .unwrap()
                        .1
                        .trim()
                        .parse()
                        .unwrap();
                    let mut body = vec![0; length];
                    reader.read_exact(&mut body).unwrap();
                    assert_eq!(
                        serde_json::from_slice::<Value>(&body).unwrap(),
                        json!({"Username": "Viewer", "Pw": password})
                    );
                }
                let redirect = if status.starts_with("307") {
                    "Location: http://127.0.0.1:1/other\r\n"
                } else {
                    ""
                };
                write!(socket, "HTTP/1.1 {status}\r\n{redirect}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}", reply.len()).unwrap();
            }
        });
        assert!(server.pair().unwrap().is_none());
        let denied = server
            .authenticate_by_name("Viewer", "incorrect")
            .err()
            .unwrap();
        assert!(denied.to_string().contains("declined sign-in"));
        let account = server.authenticate_by_name("Viewer", "").unwrap();
        assert_eq!(account.user, "user-id");
        assert_eq!(account.username.as_deref(), Some("Viewer"));
        assert_eq!(account.server, base);
        assert_eq!(account.device, "test-device");
        assert_eq!(account.token, "token");
        let redirected = server
            .authenticate_by_name("Viewer", "another")
            .err()
            .unwrap();
        assert!(redirected.to_string().contains("redirected"));
        fake_api.join().unwrap();
    }

    #[test]
    fn dolby_with_incompatible_audio_requests_only_the_original_file() {
        exercise_direct_play(false, false);
    }
    #[test]
    fn alternate_version_uses_its_own_tracks_and_engine() {
        exercise_direct_play(true, false);
    }
    #[test]
    fn audio_choice_precedes_server_default_then_file_default() {
        let mut source: Source = serde_json::from_value(json!({"MediaStreams":[
            {"Type":"Audio","Index":1},
            {"Type":"Audio","Index":2,"IsDefault":true,"IsExternal":true},
            {"Type":"Audio","Index":3,"IsDefault":true},
            {"Type":"Audio","Index":4}
        ]}))
        .unwrap();
        let selected = |s: &Source, choice| s.selected_audio(choice).unwrap().unwrap().index;
        assert_eq!(selected(&source, None), Some(3));
        source.default_audio_stream_index = Some(4);
        assert_eq!(selected(&source, None), Some(4));
        assert_eq!(selected(&source, Some(1)), Some(1));
        assert!(source.selected_audio(Some(2)).is_err());
    }
    #[test]
    fn mel_compatibility_requests_the_original_file_and_selected_audio() {
        exercise_direct_play(false, true);
    }
    fn exercise_direct_play(alternate: bool, mel: bool) {
        use std::io::{BufRead, BufReader, Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/jellyfin/", listener.local_addr().unwrap());
        let api = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&socket);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let path = if alternate {
                "POST /jellyfin/Items/episode/PlaybackInfo?mediaSourceId=other "
            } else {
                "POST /jellyfin/Items/episode/PlaybackInfo "
            };
            assert!(line.starts_with(path));
            let mut length = 0;
            loop {
                line.clear();
                assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                if line.trim().is_empty() {
                    break;
                }
                if let Some((key, value)) = line.split_once(':')
                    && key.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut data = vec![0; length];
            reader.read_exact(&mut data).unwrap();
            let body: Value = serde_json::from_slice(&data).unwrap();
            assert_eq!(body["EnableDirectPlay"], true);
            assert_eq!(body["EnableTranscoding"], false);
            assert_eq!(body["EnableDirectStream"], false);
            let can_embed_subrip = body["DeviceProfile"]["SubtitleProfiles"]
                .as_array()
                .is_some_and(|profiles| {
                    profiles.contains(&json!({"Format":"subrip","Method":"Embed"}))
                });
            let profile = if mel { 7 } else { 5 };
            let response=json!({"PlaySessionId":"session","MediaSources":[{"Id":"source","Container":"mkv",
                "SupportsDirectPlay":can_embed_subrip,"DefaultAudioStreamIndex":2,"DefaultSubtitleStreamIndex":3,"MediaStreams":[
                    {"Type":"Video","Codec":"hevc","DvProfile":profile},{"Type":"Audio","Codec":"aac","Index":1},
                    {"Type":"Audio","Codec":"eac3","Index":2},
                    {"Type":"Subtitle","Codec":"subrip","Index":3,"IsForced":true,"IsTextSubtitleStream":true}]},
                {"Id":"other","Container":"mp4","SupportsDirectPlay":true,"MediaStreams":[
                    {"Type":"Video","VideoRangeType":"SDR","Codec":"h264"},{"Type":"Audio","Codec":"aac","Index":3},
                    {"Type":"Subtitle","Codec":"srt","Index":4,"IsTextSubtitleStream":true}]}]}).to_string();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.len(),response).unwrap();
        });
        let server = Server::restore(Account {
            server: base,
            device: "test".into(),
            user: "test".into(),
            username: None,
            token: "test".into(),
        })
        .unwrap();
        let item = serde_json::from_value(json!({"Id":"episode","Name":"Test"})).unwrap();
        let choice = if alternate {
            MediaChoice {
                source: Some("other".into()),
                audio: Some(3),
                subtitle: Some(4),
            }
        } else {
            MediaChoice::default()
        };
        let request = server.playback(&item, &choice).unwrap();
        api.join().unwrap();
        if alternate {
            assert_eq!(request.route, Route::Mpv);
            assert_eq!(request.source.id, "other");
            assert_eq!(request.audio_index, Some(3));
            assert_eq!(request.subtitle_index, Some(4));
            assert_eq!(request.url.path(), "/jellyfin/Videos/episode/stream.mp4");
            return;
        }
        assert_eq!(request.url.path(), "/jellyfin/Videos/episode/stream.mkv");
        assert!(
            request
                .url
                .query_pairs()
                .any(|(name, value)| name == "static" && value == "true")
        );
        assert_eq!(request.audio_index, Some(2));
        assert_eq!(request.subtitle_index, Some(3));
        assert!(request.external_subtitle.is_none());
        let expected = if mel {
            Route::Profile7
        } else {
            Route::Dolby("dvhe.05".into())
        };
        assert_eq!(request.route, expected);
    }

    fn select(fields: Value) -> Result<Route> {
        let mut s = json!({"Type":"Video","Codec":"hevc"});
        s.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        route(&[serde_json::from_value(s)?])
    }
    #[test]
    fn routes_from_stream_metadata() {
        for range in ["SDR", "HDR10", "HDR10Plus", "HLG"] {
            assert_eq!(
                select(json!({"VideoRangeType": range})).unwrap(),
                Route::Mpv
            );
        }
        for range in [
            "DOVI",
            "DOVIWithHDR10",
            "DOVIWithHLG",
            "DOVIWithSDR",
            "DOVIWithHDR10Plus",
        ] {
            assert_eq!(
                select(json!({"VideoRangeType":range,"DvProfile":8})).unwrap(),
                Route::Dolby("dvhe.08".into())
            );
        }
        assert_eq!(
            select(json!({"VideoRangeType":"HDR10","DvProfile":5})).unwrap(),
            Route::Dolby("dvhe.05".into())
        );
    }
    #[test]
    fn profile_7_fallback_keeps_sdr_output_and_stops_retrying() {
        let fel = Route::MpvProfile7 { fel: true };
        assert_eq!(fel.profile7_fallback(), Some(Route::FelHdr10));
        assert_eq!(Route::FelHdr10.profile7_fallback(), None);
        let sdr = Route::Sdr {
            source: HdrSource::Profile7 { fel: true },
            hdr_available: true,
        };
        let fallback = sdr.profile7_fallback().unwrap();
        assert_eq!(
            fallback,
            Route::Sdr {
                source: HdrSource::Profile7Base,
                hdr_available: true,
            }
        );
        assert!(fallback.hdr10_base_layer());
        assert_eq!(fallback.profile7_fallback(), None);
        assert_eq!(Route::MpvDolby.profile7_fallback(), None);
    }

    #[test]
    fn profile_7_requires_hevc_and_defers_layer_selection_to_the_player() {
        let candidate = |codec: &str, el: i32| {
            serde_json::from_value::<Stream>(json!({
            "Type": "Video", "Codec": codec, "DvProfile": 7, "RpuPresentFlag": 1, "ElPresentFlag": el,
            "VideoRangeType": "DOVIWithEL"
        })).unwrap()
        };
        assert_eq!(route(&[candidate("hevc", 1)]).unwrap(), Route::Profile7);
        assert!(route(&[candidate("hevc", 0)]).is_err());
        assert!(route(&[candidate("h264", 1)]).is_err());
        assert!(route(&[candidate("hevc", 1), candidate("hevc", 1)]).is_err());
        let legacy: MediaChoice =
            serde_json::from_value(json!({"audio": 1, "mel_compatibility": false})).unwrap();
        assert_eq!(legacy.audio, Some(1));
        assert_eq!(
            serde_json::from_str::<Route>(r#""DolbyMel81""#).unwrap(),
            Route::Profile7
        );
    }

    #[test]
    fn rejects_ambiguous_or_unsupported_dolby() {
        for fields in [
            json!({"VideoRangeType":"DOVIInvalid"}),
            json!({"CodecTag":"dvh1"}),
            json!({"DvProfile":8,"ElPresentFlag":1}),
            json!({"VideoRangeType":"Unknown"}),
            json!({"DvProfile":5,"RpuPresentFlag":0}),
            json!({"RpuPresentFlag":1}),
        ] {
            assert!(select(fields).is_err());
        }
        assert!(route(&[]).is_err());
    }
}
