use super::*;
use crate::preferences;
use reqwest::header::{ACCEPT_ENCODING, CONTENT_RANGE, ETAG, IF_RANGE, LAST_MODIFIED, RANGE};
use std::{
    fs::{self, File, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Download {
    pub item: Item,
    pub source: Source,
    pub route: Route,
    pub subtitle: Option<i32>,
    pub audio: Option<i32>,
    pub format: String,
    pub queued: u64,
}
#[derive(Default, Serialize, Deserialize)]
struct PartialFile {
    validator: Option<String>,
}
#[derive(Default, Serialize, Deserialize, PartialEq, Clone)]
pub struct Progress {
    pub ticks: u64,
    pub played: bool,
    revision: String,
}
#[derive(Default)]
pub struct Transfer {
    pub pause: AtomicBool,
    pub received: AtomicU64,
    pub total: AtomicU64,
}

impl Download {
    pub fn dir(&self, profile: &Path) -> PathBuf {
        profile
            .join("downloads")
            .join(preferences::item_key(&self.source.id))
    }
    pub fn ready(&self, profile: &Path) -> bool {
        self.dir(profile).join("original").is_file()
    }
    pub fn paused(&self, profile: &Path) -> bool {
        self.dir(profile).join("paused").exists()
    }
    pub fn removed(&self, profile: &Path) -> bool {
        self.dir(profile).join("removed").exists()
    }
    pub fn remove_local(&self, profile: &Path) -> Result<()> {
        let dir = self.dir(profile);
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join("download.lock"))?;
        lock.try_lock()
            .context("Pause this transfer before removing it.")?;
        self.set_paused(profile, true)?;
        for name in [
            "original",
            "partial",
            "subtitles.vtt",
            "partial.json",
            "cover.jpg",
            "lyrics.json",
        ] {
            if let Err(e) = fs::remove_file(dir.join(name))
                && e.kind() != ErrorKind::NotFound
            {
                return Err(e.into());
            }
        }
        fs::write(dir.join("removed"), [])?;
        Ok(())
    }
    pub fn set_paused(&self, profile: &Path, paused: bool) -> Result<()> {
        let marker = self.dir(profile).join("paused");
        if paused {
            fs::write(marker, [])?;
        } else if let Err(e) = fs::remove_file(marker)
            && e.kind() != ErrorKind::NotFound
        {
            return Err(e.into());
        }
        Ok(())
    }
    pub fn list(profile: &Path) -> Result<Vec<Self>> {
        let root = profile.join("downloads");
        let entries = match fs::read_dir(root) {
            Ok(e) => e,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e.into()),
        };
        let mut all = Vec::new();
        for entry in entries {
            let path = entry?.path().join("metadata.json");
            if path.is_file() {
                all.push(serde_json::from_reader::<_, Self>(File::open(path)?)?);
            }
        }
        all.sort_by_key(|d| d.queued);
        Ok(all)
    }
    pub fn item_with_progress(&self, profile: &Path) -> Result<Item> {
        let dir = self.dir(profile);
        let mut item = self.item.clone();
        let _lock = progress_lock(&dir)?;
        let pending = dir.join("pending.json");
        let progress_path = if pending.is_file() {
            pending
        } else {
            dir.join("progress.json")
        };
        if progress_path.is_file() {
            let progress: Progress = preferences::load(&progress_path)?;
            let data = item.user_data.get_or_insert_with(Default::default);
            data.playback_position_ticks = Some(progress.ticks);
            data.played = Some(progress.played);
        }
        Ok(item)
    }

    pub fn local(&self, profile: &Path) -> Result<(Item, PlayRequest, String)> {
        let dir = self.dir(profile);
        let file = dir.join("original");
        ensure!(file.is_file(), "This download is incomplete.");
        let item = self.item_with_progress(profile)?;
        let sub = dir.join("subtitles.vtt");
        let vtt = if sub.is_file() {
            fs::read_to_string(&sub)?
        } else {
            String::new()
        };
        let external = self
            .source
            .media_streams
            .iter()
            .flatten()
            .any(|s| s.index == self.subtitle && s.is_external && s.kind == "Subtitle");
        let local_url = |path: &Path| {
            Url::from_file_path(path).map_err(|_| anyhow::anyhow!("Invalid download path."))
        };
        let mut source = self.source.clone();
        source.path = Some(file.to_string_lossy().into_owned());
        let req = PlayRequest {
            seekable: true,
            frame_rate: self.source.frame_rate(),
            url: local_url(&file)?,
            route: self.route.clone(),
            source,
            session: uuid::Uuid::new_v4().to_string(),
            item: item.id.clone(),
            audio_index: self.audio,
            subtitle_track: self
                .source
                .media_streams
                .iter()
                .flatten()
                .find(|s| s.kind == "Subtitle" && s.index == self.subtitle)
                .cloned(),
            subtitle_index: if external && vtt.is_empty() {
                None
            } else {
                self.subtitle
            },
            subtitle_size: 36,
            external_subtitle: if external && !vtt.is_empty() {
                Some(local_url(&sub)?)
            } else {
                None
            },
            format: self.format.clone(),
            resume: item.position() as f64 / 10_000_000.,
            allow_passthrough: true,
            start_paused: false,
            audio_only: item.kind == "Audio",
            _live: None,
        };
        Ok((item, req, vtt))
    }
    pub fn progress(&self, profile: &Path, ticks: u64, played: bool) -> Result<()> {
        let dir = self.dir(profile);
        let _lock = progress_lock(&dir)?;
        let saved: Progress = preferences::load(&dir.join("progress.json"))?;
        let pending: Progress = preferences::load(&dir.join("pending.json"))?;
        let data = Progress {
            ticks,
            played: played || saved.played || pending.played,
            revision: uuid::Uuid::new_v4().to_string(),
        };
        preferences::save(&dir.join("pending.json"), &data)?;
        preferences::save(&dir.join("progress.json"), &data)
    }
}

fn progress_lock(dir: &Path) -> Result<File> {
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(dir.join("progress.lock"))?;
    f.lock()?;
    Ok(f)
}

impl Server {
    pub fn add_download(&self, item: &Item, choice: &MediaChoice) -> Result<Download> {
        let profile = self.profile_path()?;
        let req = self.playback(item, choice)?;
        let folder = profile
            .join("downloads")
            .join(preferences::item_key(&req.source.id));
        let path = folder.join("metadata.json");
        if path.is_file() {
            let record: Download = serde_json::from_reader(File::open(path)?)?;
            if record.removed(&profile) {
                fs::remove_file(folder.join("removed"))?;
                record.set_paused(&profile, false)?;
            }
            return Ok(record);
        }
        let mut item = self.item(&item.id)?;
        ensure!(
            item.can_download != Some(false),
            "Jellyfin disallows downloading this title."
        );
        let mut source = item
            .media_sources
            .iter()
            .flatten()
            .find(|s| s.id == req.source.id)
            .cloned()
            .context("The source is no longer available.")?;
        source.path = None;
        item.media_sources = Some(vec![source.clone()]);
        let download = Download {
            item,
            source,
            route: req.route,
            subtitle: req.subtitle_index,
            audio: req.audio_index,
            format: req.format,
            queued: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_millis() as u64,
        };
        preferences::save(&path, &download)?;
        Ok(download)
    }

    pub fn transfer(&self, d: &Download, state: &Transfer) -> Result<()> {
        let profile = self.profile_path()?;
        let dir = d.dir(&profile);
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(dir.join("download.lock"))?;
        lock.try_lock()
            .context("Another RustyFin window is downloading this version.")?;
        if d.ready(&profile) {
            return Ok(());
        }
        let part = dir.join("partial");
        let meta = dir.join("partial.json");
        let prior: PartialFile = preferences::load(&meta)?;
        let existing = fs::metadata(&part).map_or(0, |m| m.len());
        let get = || {
            self.get(&["Items", &d.source.id, "Download"])
                .timeout(Duration::from_secs(24 * 3600))
                .header(ACCEPT_ENCODING, "identity")
        };
        let mut req = get();
        let resume = existing > 0 && prior.validator.is_some();
        if let Some(tag) = prior.validator.as_ref().filter(|_| resume) {
            req = req
                .header(RANGE, format!("bytes={existing}-"))
                .header(IF_RANGE, tag);
        }
        let mut response = req.send().map_err(|e| e.without_url())?;
        let validator = |response: &reqwest::blocking::Response| {
            response
                .headers()
                .get(ETAG)
                .and_then(|v| v.to_str().ok())
                .filter(|s| !s.starts_with("W/"))
                .or_else(|| response.headers().get(LAST_MODIFIED)?.to_str().ok())
                .map(str::to_owned)
        };
        let range_total = |response: &reqwest::blocking::Response| -> Option<u64> {
            if !resume || validator(response) != prior.validator {
                return None;
            }
            let range = response
                .headers()
                .get(CONTENT_RANGE)?
                .to_str()
                .ok()?
                .strip_prefix("bytes ")?;
            let (start, rest) = range.split_once('-')?;
            let (end, total) = rest.split_once('/')?;
            let total = total.parse::<u64>().ok()?;
            (start.parse::<u64>().ok()? == existing
                && existing < total
                && end.parse::<u64>().ok()?.checked_add(1) == Some(total)
                && response
                    .content_length()
                    .is_none_or(|size| size == total - existing))
            .then_some(total)
        };
        let mut ranged_total = None;
        if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
            ranged_total = range_total(&response);
        }
        if response.status() == reqwest::StatusCode::RANGE_NOT_SATISFIABLE
            || (response.status() == reqwest::StatusCode::PARTIAL_CONTENT && ranged_total.is_none())
        {
            response = get().send().map_err(|e| e.without_url())?;
            ensure!(
                response.status() != reqwest::StatusCode::PARTIAL_CONTENT,
                "The server returned a partial file to a full download request."
            );
        }
        let mut response = response.error_for_status().map_err(|e| e.without_url())?;
        let partial = ranged_total.is_some();
        let offset = if partial { existing } else { 0 };
        let total = ranged_total
            .or_else(|| response.content_length())
            .or(d.source.size);
        let validator = validator(&response);
        state.total.store(total.unwrap_or(0), Ordering::Relaxed);
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(partial)
            .truncate(!partial)
            .open(&part)?;
        preferences::save(&meta, &PartialFile { validator })?;
        let mut received = offset;
        state.received.store(received, Ordering::Relaxed);
        let mut buf = vec![0; 256 * 1024];
        while !state.pause.load(Ordering::Acquire) {
            let n = response.read(&mut buf)?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])?;
            received += n as u64;
            state.received.store(received, Ordering::Relaxed);
        }
        file.sync_all()?;
        drop(file);
        if state.pause.load(Ordering::Acquire) {
            return Ok(());
        }
        ensure!(
            received > 0
                && total.is_none_or(|n| n == received)
                && d.source.size.is_none_or(|n| n == received),
            "Download is incomplete or its size differs from the selected file."
        );
        if let Some(index) = d.subtitle
            && let Some(track) = d
                .source
                .media_streams
                .iter()
                .flatten()
                .find(|s| s.index == Some(index) && s.kind == "Subtitle")
            && track.is_text_subtitle_stream != Some(false)
            && (track.is_external
                || d.route.native_profile().is_some()
                || d.route == Route::Profile7)
        {
            let captions = self.captions(&d.item.id, &d.source.id, index)?;
            fs::write(dir.join("subtitles.vtt"), captions)?;
        }
        if let Some(art) = d.item.art(ArtKind::Poster)
            && let Ok(bytes) = self.artwork(&art)
            && !bytes.is_empty()
        {
            fs::write(dir.join("cover.jpg"), bytes)?;
        }
        if d.item.kind == "Audio"
            && let Ok(lines) = self.lyrics(&d.item.id)
            && !lines.is_empty()
        {
            preferences::save(&dir.join("lyrics.json"), &lines)?;
        }
        fs::rename(part, dir.join("original"))?;
        Ok(())
    }
    pub fn sync_downloads(&self, downloads: &[Download]) -> Result<()> {
        let profile = self.profile_path()?;
        for d in downloads {
            let dir = d.dir(&profile);
            let path = dir.join("pending.json");
            if !path.is_file() {
                continue;
            }
            let sync_lock = OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .open(dir.join("sync.lock"))?;
            if sync_lock.try_lock().is_err() || !path.is_file() {
                continue;
            }
            let value: Progress = preferences::load(&path)?;
            self.post(&["UserItems", &d.item.id, "UserData"])
                .query(&[("userId", self.user_id()?)])
                .json(&json!({"PlaybackPositionTicks":value.ticks,"Played":value.played}))
                .send()
                .and_then(|r| r.error_for_status())
                .map_err(|e| e.without_url())?;
            let _lock = progress_lock(&dir)?;
            if path.is_file() && preferences::load::<Progress>(&path)? == value {
                preferences::save(&dir.join("progress.json"), &value)?;
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }
}
