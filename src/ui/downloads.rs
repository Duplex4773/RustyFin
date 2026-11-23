use super::*;
use crate::jellyfin::offline::{Download, Transfer};
use std::{
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct SyncRule {
    pub series: String,
    pub name: String,
    pub limit: usize,
    pub specials: bool,
}
#[derive(Default)]
pub(super) struct Downloads {
    pub records: Vec<Download>,
    pub profile: Option<PathBuf>,
    pub active: Option<(String, Arc<Transfer>)>,
    worker: Option<Task<()>>,
    pub rules: Vec<SyncRule>,
    rule_job: Option<Task<()>>,
    last_rules: Option<Instant>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    sync: Option<Task<()>>,
    pub notice: String,
    pub playing: Option<Download>,
    pub queue: std::collections::VecDeque<Download>,
    last_sync: Option<Instant>,
    checkpoint: Option<Instant>,
}
impl Drop for Downloads {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some((_, state)) = &self.active {
            state.pause.store(true, Ordering::Release);
        }
    }
}
impl Library {
    pub(super) fn open_downloads(&mut self) {
        self.downloads = Downloads::default();
        if let Some(server) = &self.server
            && let Ok(path) = server.profile_path()
        {
            match Download::list(&path) {
                Ok(rows) => self.downloads.records = rows,
                Err(e) => {
                    self.downloads.notice = format!("Could not read the download library. {e}")
                }
            }
            match preferences::load(&path.join("sync-rules.json")) {
                Ok(rules) => self.downloads.rules = rules,
                Err(e) => self.downloads.notice = e.to_string(),
            }
            self.downloads.profile = Some(path);
        }
    }
    pub(super) fn enqueue_download(&mut self, item: Item, w: &mut Window, cx: &mut Context<Self>) {
        let Some(server) = self.server.clone() else {
            return;
        };
        let choice = self.state.choice.clone();
        self.work(
            w,
            cx,
            move || server.add_download(&item, &choice),
            |s, record, w, cx| {
                if !s
                    .downloads
                    .records
                    .iter()
                    .any(|r| r.source.id == record.source.id)
                {
                    s.downloads.records.push(record);
                }
                s.navigate(Screen::Downloads, w, cx);
                s.download_tick(w, cx);
                Ok(())
            },
        );
    }
    pub(super) fn toggle_download(
        &mut self,
        d: &Download,
        paused: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = &self.downloads.profile else {
            return;
        };
        if let Err(e) = d.set_paused(path, paused) {
            self.downloads.notice = e.to_string();
            return;
        }
        if paused
            && let Some((id, transfer)) = &self.downloads.active
            && id == &d.source.id
        {
            transfer.pause.store(true, Ordering::Release);
        }
        self.downloads.notice.clear();
        self.download_tick(w, cx);
        cx.notify();
    }
    pub(super) fn rule(
        &mut self,
        series: &Item,
        limit: usize,
        specials: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = &self.downloads.profile else {
            return;
        };
        let mut rules = self.downloads.rules.clone();
        rules.retain(|r| r.series != series.id);
        if limit > 0 {
            rules.push(SyncRule {
                series: series.id.clone(),
                name: series.name.clone(),
                limit,
                specials,
            });
        }
        match preferences::save(&path.join("sync-rules.json"), &rules) {
            Ok(()) => {
                self.downloads.rules = rules;
                self.downloads.last_rules = None;
            }
            Err(e) => self.downloads.notice = e.to_string(),
        }
        cx.notify();
    }
    pub(super) fn queue_download_group(
        &mut self,
        parent: Item,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(server) = self.server.clone() else {
            return;
        };
        let cancel = self.downloads.cancel.clone();
        self.work(
            w,
            cx,
            move || {
                let profile = server.profile_path()?;
                let items = server.queue(None, Some(&parent), false)?;
                let mut result = Ok(());
                for item in items {
                    if cancel.load(Ordering::Acquire) {
                        break;
                    }
                    let choice = preferences::load(&preferences::item_path(&profile, &item.id))?;
                    if let Err(e) = server.add_download(&item, &choice) {
                        result = Err(e);
                        break;
                    }
                }
                Ok((Download::list(&profile)?, result))
            },
            |s, (records, result), w, cx| {
                s.downloads.records = records;
                s.navigate(Screen::Downloads, w, cx);
                result
            },
        );
    }
    fn discover_downloads(&mut self, w: &Window, cx: &mut Context<Self>) {
        if self.downloads.rules.is_empty()
            || self.downloads.rule_job.is_some()
            || self
                .downloads
                .last_rules
                .is_some_and(|t| t.elapsed() < Duration::from_secs(15 * 60))
        {
            return;
        }
        let Some(server) = self.server.clone() else {
            return;
        };
        let rules = self.downloads.rules.clone();
        let cancel = self.downloads.cancel.clone();
        self.downloads.last_rules = Some(Instant::now());
        let task = cx.background_executor().spawn(async move {
            let profile = server.profile_path()?;
            let known: std::collections::HashSet<_> = Download::list(&profile)?
                .into_iter()
                .map(|d| d.item.id)
                .collect();
            for rule in rules {
                if cancel.load(Ordering::Acquire) {
                    break;
                }
                let series = server.item(&rule.series)?;
                for ep in server
                    .queue(None, Some(&series), false)?
                    .into_iter()
                    .filter(|i| {
                        i.user_data.as_ref().and_then(|u| u.played) != Some(true)
                            && (rule.specials || i.parent_index_number != Some(0))
                    })
                    .take(rule.limit)
                {
                    if cancel.load(Ordering::Acquire) {
                        break;
                    }
                    if known.contains(&ep.id) {
                        continue;
                    }
                    let choice = preferences::load(&preferences::item_path(&profile, &ep.id))?;
                    server.add_download(&ep, &choice)?;
                }
            }
            Download::list(&profile)
        });
        self.downloads.rule_job = Some(cx.spawn_in(w, async move |view, cx| {
            let result = task.await;
            let _ = view.update(cx, |s, cx| {
                s.downloads.rule_job = None;
                match result {
                    Ok(records) => s.downloads.records = records,
                    Err(e) => s.downloads.notice = format!("Download rules will retry. {e}"),
                }
                cx.notify();
            });
        }));
    }
    pub(super) fn download_tick(&mut self, w: &Window, cx: &mut Context<Self>) {
        self.discover_downloads(w, cx);
        let (Some(path), Some(server)) = (self.downloads.profile.as_ref(), self.server.clone())
        else {
            return;
        };
        if self.downloads.sync.is_none()
            && self
                .downloads
                .last_sync
                .is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
        {
            self.downloads.last_sync = Some(Instant::now());
            let pending: Vec<_> = self
                .downloads
                .records
                .iter()
                .filter(|d| d.dir(path).join("pending.json").exists())
                .cloned()
                .collect();
            if !pending.is_empty() {
                let client = server.clone();
                let work = cx
                    .background_executor()
                    .spawn(async move { client.sync_downloads(&pending) });
                self.downloads.sync = Some(cx.spawn_in(w, async move |view, cx| {
                    let synced = work.await;
                    let _ = view.update(cx, |s, cx| {
                        s.downloads.sync = None;
                        if synced.is_err() {
                            s.downloads.notice = "Watch progress is saved locally. Sync will retry when Jellyfin is available.".into();
                        }
                        cx.notify();
                    });
                }));
            }
        }
        if self.downloads.active.is_some() {
            if matches!(self.state.screen, Screen::Downloads) {
                cx.notify();
            }
            return;
        }
        let Some(d) = self
            .downloads
            .records
            .iter()
            .find(|d| !d.ready(path) && !d.paused(path))
            .cloned()
        else {
            return;
        };
        let state = Arc::new(Transfer::default());
        self.downloads.active = Some((d.source.id.clone(), state.clone()));
        let target = d.clone();
        let work = cx
            .background_executor()
            .spawn(async move { server.transfer(&target, &state) });
        self.downloads.worker = Some(cx.spawn_in(w, async move |view, cx| {
            let result = work.await;
            let _ = view.update(cx, |s, cx| {
                s.downloads.active = None;
                if let Err(e) = result {
                    if let Some(p) = &s.downloads.profile {
                        let _ = d.set_paused(p, true);
                    }
                    s.downloads.notice = format!("{} · {e:#}", d.item.name);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }
    pub(super) fn play_download(&mut self, d: Download, w: &mut Window, cx: &mut Context<Self>) {
        let Some(profile) = &self.downloads.profile else {
            return;
        };
        let result = d.local(profile);
        let lyrics = preferences::load(&d.dir(profile).join("lyrics.json")).unwrap_or_default();
        let result = result.and_then(|(item, mut request, vtt)| {
            crate::player::resolve_profile7(&mut request, self.prefs.mel_to_81)?;
            self.stop();
            self.controls = Default::default();
            self.controls.lyrics = lyrics;
            self.controls.speed = 1.;
            self.controls.zoom = 1.;
            if request.route.native_profile().is_some() {
                self.controls.captions = super::captions::Captions::parse(&vtt);
            }
            request.allow_passthrough = self.prefs.passthrough;
            request.subtitle_size = self.prefs.subtitle_size;
            self.playing = Some(item);
            self.downloads.playing = Some(d);
            self.downloads.checkpoint = Some(Instant::now());
            self.start_player(request, w, cx)
        });
        if let Err(e) = result {
            self.message = format!("{e:#}");
        }
        cx.notify();
    }
    pub(super) fn queue_offline_group(
        &mut self,
        from: &Download,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profile) = &self.downloads.profile else {
            return;
        };
        let mut downloads: Vec<_> = self
            .downloads
            .records
            .iter()
            .filter(|d| {
                d.ready(profile)
                    && !d.removed(profile)
                    && ((from.item.album_id.is_some() && from.item.album_id == d.item.album_id)
                        || (from.item.series_id.is_some()
                            && from.item.series_id == d.item.series_id))
            })
            .cloned()
            .collect();
        downloads.sort_by_key(|d| (d.item.parent_index_number, d.item.index_number));
        let mut seen = std::collections::HashSet::new();
        downloads.retain(|d| seen.insert(d.item.id.clone()));
        let mut queue = std::collections::VecDeque::from(downloads);
        if let Some(first) = queue.pop_front() {
            self.play_download(first, w, cx);
            self.downloads.queue = queue;
        }
    }
    pub(super) fn checkpoint_download(&mut self, ticks: u64, ended: bool, force: bool) {
        if let (Some(d), Some(profile)) = (&self.downloads.playing, &self.downloads.profile)
            && (force
                || self
                    .downloads
                    .checkpoint
                    .is_none_or(|t| t.elapsed() >= Duration::from_secs(10)))
        {
            self.downloads.checkpoint = Some(Instant::now());
            if let Err(e) = d.progress(
                profile,
                ticks,
                ended || d.item.user_data.as_ref().and_then(|u| u.played) == Some(true),
            ) {
                self.downloads.notice = format!("Could not save offline progress. {e}");
            }
        }
    }
}
