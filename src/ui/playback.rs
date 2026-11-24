use super::captions::Captions;
use super::*;
use crate::{
    jellyfin::{MediaChoice, Segment},
    preferences::{self, Shader},
};
use std::collections::VecDeque;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlayerMenu {
    #[default]
    Settings,
    Speed,
    Zoom,
    Shaders,
    Quality,
    Timer,
    AudioSync,
    SubtitleSync,
    Audio,
    Subtitles,
    Chapters,
    Episodes,
    EpisodeSeasons,
    Queue,
    Info,
}

impl PlayerMenu {
    pub(super) fn episode_drawer(self) -> bool {
        matches!(self, Self::Episodes | Self::EpisodeSeasons)
    }
}

pub(super) struct MenuFocus {
    pub scope: FocusHandle,
    pub current: FocusHandle,
    pub close: FocusHandle,
    launchers: [FocusHandle; 4],
    pub volume: FocusHandle,
    pub timeline: FocusHandle,
    pub play_pause: FocusHandle,
    pub fullscreen: FocusHandle,
    pub next: FocusHandle,
    pub credits: FocusHandle,
    pub rows: std::cell::RefCell<Vec<FocusHandle>>,
    pub scroll: ScrollHandle,
    pub entering: std::cell::Cell<bool>,
    pub selected: std::cell::Cell<bool>,
    pub origin: PlayerMenu,
    pub settings_row: PlayerMenu,
}

impl MenuFocus {
    pub fn new(cx: &mut Context<Library>) -> Self {
        Self {
            scope: cx.focus_handle(),
            // GPUI uses the tab-stop setting from the tracked handle.
            current: cx.focus_handle().tab_stop(true),
            close: cx.focus_handle().tab_stop(true),
            launchers: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            volume: cx.focus_handle().tab_stop(true),
            timeline: cx.focus_handle().tab_stop(true),
            play_pause: cx.focus_handle().tab_stop(true),
            fullscreen: cx.focus_handle().tab_stop(true),
            next: cx.focus_handle().tab_stop(true),
            credits: cx.focus_handle().tab_stop(true),
            rows: Default::default(),
            scroll: ScrollHandle::new(),
            entering: Default::default(),
            selected: Default::default(),
            origin: PlayerMenu::Settings,
            settings_row: PlayerMenu::Speed,
        }
    }

    pub fn launcher(&self, page: PlayerMenu) -> &FocusHandle {
        &self.launchers[match page {
            PlayerMenu::Subtitles => 1,
            PlayerMenu::Audio => 2,
            PlayerMenu::Episodes => 3,
            _ => 0,
        }]
    }

    pub fn reveal(&self, w: &Window, cx: &App, warning: bool, at_top: bool) {
        if let Some(index) = self
            .rows
            .borrow()
            .iter()
            .position(|row| row.contains_focused(w, cx))
        {
            let index = index + usize::from(warning);
            if at_top {
                self.scroll.scroll_to_top_of_item(index);
            } else {
                self.scroll.scroll_to_item(index);
            }
        }
    }
}

#[derive(Default)]
pub(super) struct Controls {
    pub segments: Vec<Segment>,
    pub lyrics: Vec<crate::jellyfin::Lyric>,
    pub mini: bool,
    pub source: String,
    pub choice: MediaChoice,
    pub hover: Option<f32>,
    pub volume_hovered: bool,
    pub animate_chrome: bool,
    pub remote_scrub: bool,
    pub lyric_scroll: ScrollHandle,
    pub lyric_line: Option<usize>,
    pub captions: Captions,
    pub queue: Option<VecDeque<Item>>,
    pub speed: f64,
    pub zoom: f64,
    pub delay: f64,
    pub subtitle_delay: f64,
    pub menu: PlayerMenu,
    pub episodes: super::episodes::EpisodeDrawer,
    pub next_episode: Option<Item>,
    pub previous_episode: Option<Item>,
    pub episode_hover: Option<(EpisodeDirection, Instant)>,
    pub next_lookup_finished: bool,
    pub watching_credits: bool,
    pub sleep: Option<Instant>,
    pub sleep_at_end: bool,
    pub skipped: Option<u64>,
    pub outro_deadline: Option<(u64, f64)>,
    pub warning: String,
    pub profile_notice: Option<(&'static str, Instant)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EpisodeDirection {
    Previous,
    Next,
}
impl Controls {
    pub(super) fn auto_skip_target(&mut self, position: f64, offer_choice: bool) -> Option<f64> {
        let Some(segment) = self
            .segments
            .iter()
            .find(|s| matches!(s.kind.as_str(), "Intro" | "Outro") && s.active(position))
        else {
            self.skipped = None;
            self.outro_deadline = None;
            return None;
        };
        if self.skipped == Some(segment.start_ticks) {
            return None;
        }
        if segment.kind == "Outro" {
            if self.watching_credits {
                return None;
            }
            if offer_choice {
                // Count playback time, so pausing also pauses the credits decision.
                let (marker, until) = self
                    .outro_deadline
                    .get_or_insert((segment.start_ticks, position + 10.));
                if *marker != segment.start_ticks || position + 10. < *until {
                    *marker = segment.start_ticks;
                    *until = position + 10.;
                }
                if position < *until {
                    return None;
                }
            }
        }
        self.skipped = Some(segment.start_ticks);
        self.outro_deadline = None;
        Some(segment.end_ticks as f64 / 10_000_000.)
    }
}

pub(super) struct Retry {
    pub item: Item,
    pub choice: MediaChoice,
    pub position: f64,
    pub paused: bool,
    pub offline: Option<crate::jellyfin::offline::Download>,
    pub dolby: bool,
}
pub(super) struct Prepared {
    item: Item,
    request: PlayRequest,
    choice: MediaChoice,
    segments: Vec<Segment>,
    lyrics: Vec<crate::jellyfin::Lyric>,
    captions: Captions,
    warning: String,
}

impl Server {
    fn prepare(
        &self,
        item: Item,
        position: Option<f64>,
        prefs: preferences::Preferences,
        selected: Option<MediaChoice>,
    ) -> Result<Prepared> {
        let mut choice = match selected {
            Some(c) => c,
            None => preferences::load(&preferences::item_path(&self.profile_path()?, &item.id))?,
        };
        let mut request = self.playback(&item, &choice)?;
        crate::player::resolve_profile7(&mut request, prefs.mel_to_81)?;
        request.allow_passthrough = prefs.passthrough;
        request.subtitle_size = prefs.subtitle_size;
        if let Some(seconds) = position {
            request.resume = seconds;
        }
        let item = if item.media_sources.is_some()
            && item.chapters.is_some()
            && (request.audio_only || !request.seekable || item.trickplay.is_some())
        {
            item
        } else {
            self.item(&item.id).unwrap_or(item)
        };
        let mut captions = Captions::default();
        let mut warning = String::new();
        let native = request.route.native_profile().is_some();
        if let Some(reason) = request
            .subtitle_track
            .as_ref()
            .and_then(|t| t.subtitle_limitation(native))
        {
            warning = reason.to_owned();
            choice.subtitle = Some(-1);
            request.subtitle_index = None;
            request.external_subtitle = None;
        } else if native && let Some(index) = request.subtitle_index {
            match self.captions(&item.id, &request.source.id, index) {
                Ok(vtt) => captions = Captions::parse(&vtt),
                Err(error) => {
                    choice.subtitle = Some(-1);
                    request.subtitle_index = None;
                    warning = format!("Could not load subtitles. {error}");
                }
            }
        }
        let segments = if item.kind == "Audio" {
            vec![]
        } else {
            self.segments(&item.id).unwrap_or_default()
        };
        let lyrics = if item.kind == "Audio" {
            self.lyrics(&item.id).unwrap_or_default()
        } else {
            vec![]
        };
        Ok(Prepared {
            choice,
            lyrics,
            item,
            request,
            segments,
            captions,
            warning,
        })
    }
}

impl Library {
    pub(super) fn select_shader(&mut self, shader: Shader, cx: &mut Context<Self>) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        match player.set_shader(shader.clone()) {
            Ok(()) => {
                if let Shader::Custom(path) = &shader
                    && !self.prefs.mpv.custom_shaders.contains(path)
                {
                    self.prefs.mpv.custom_shaders.push(path.clone());
                }
                self.prefs.mpv.shader = shader;
                self.controls.warning.clear();
                self.save_preferences(cx);
            }
            Err(e) => self.controls.warning = format!("{e:#}"),
        }
        cx.notify();
    }

    pub(super) fn import_shader(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(player) = self.player.as_ref().filter(|p| p.shader().is_some()) else {
            return;
        };
        let opened = player.opened_at();
        let selected = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Add an mpv shader".into()),
        });
        cx.spawn_in(w, async move |view, cx| {
            let result: Result<Option<Shader>> = async {
                let Some(path) = selected.await??.and_then(|paths| paths.into_iter().next()) else {
                    return Ok(None);
                };
                cx.background_executor()
                    .spawn(async move {
                        validate_shader_file(&path)?;
                        Ok(Some(Shader::Custom(path)))
                    })
                    .await
            }
            .await;
            let _ = view.update_in(cx, |s, _, cx| {
                if !s.player.as_ref().is_some_and(|p| p.opened_at() == opened) {
                    return;
                }
                match result {
                    Ok(Some(shader)) => s.select_shader(shader, cx),
                    Ok(None) => {}
                    Err(e) => {
                        s.controls.warning = format!("Could not add the shader. {e:#}");
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    pub(super) fn load_preferences(&mut self) {
        self.formats.clear();
        let result = self
            .server
            .as_ref()
            .and_then(|s| s.profile_path().ok())
            .map(|p| preferences::load(&p.join("preferences.json")));
        match result.transpose() {
            Ok(Some(p)) => self.prefs = p,
            Ok(None) => self.prefs = Default::default(),
            Err(e) => {
                self.prefs = Default::default();
                self.message = format!("Could not read playback preferences. {e}");
            }
        }
        self.recent_searches = self
            .server
            .as_ref()
            .and_then(|s| s.profile_path().ok())
            .and_then(|p| preferences::load(&p.join("recent-searches.json")).ok())
            .unwrap_or_default();
        self.rotate = self.prefs.rotate;
        self.open_downloads();
        self.restore_seerr();
        self.restore_services();
        self.restore_anime();
        self.accounts = credentials::profiles().unwrap_or_default();
        if let Ok(Some(a)) = credentials::load()
            && !self.accounts.iter().any(|saved| saved.key() == a.key())
        {
            self.accounts.push(a);
        }
        self.connected = self
            .server
            .as_ref()
            .map(|s| crate::jellyfin::federation::connections(s, &self.accounts))
            .unwrap_or_default();
    }
    pub(super) fn save_preferences(&mut self, cx: &mut Context<Self>) {
        self.prefs.rotate = self.rotate;
        if let Some(server) = &self.server
            && let Err(e) = server
                .profile_path()
                .and_then(|p| preferences::save(&p.join("preferences.json"), &self.prefs))
        {
            self.message = format!("Could not save preferences. {e}");
        }
        cx.notify();
    }
    pub(super) fn choose(&mut self, choice: MediaChoice, cx: &mut Context<Self>) {
        let (Some(server), Screen::Detail(item, _)) = (&self.server, &self.state.screen) else {
            return;
        };
        match server
            .profile_path()
            .and_then(|p| preferences::save(&preferences::item_path(&p, &item.id), &choice))
        {
            Ok(()) => {
                if self.state.choice.source != choice.source {
                    self.formats.clear();
                }
                self.state.choice = choice;
            }
            Err(e) => self.message = format!("Could not save the track selection. {e}"),
        }
        cx.notify();
    }
    pub(super) fn begin(
        &mut self,
        item: Item,
        position: Option<f64>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(server) = self.server.clone() else {
            return;
        };
        self.downloads.playing = None;
        self.reset_episode_transition();
        let prefs = self.prefs.clone();
        self.work(
            w,
            cx,
            move || server.prepare(item, position, prefs, None),
            Self::play_prepared,
        );
    }

    fn reset_episode_transition(&mut self) {
        self.controls.next_episode = None;
        self.controls.previous_episode = None;
        self.controls.episode_hover = None;
        self.controls.next_lookup_finished = false;
        self.controls.watching_credits = false;
        self.controls.outro_deadline = None;
        self.controls.skipped = None;
        self.controls.profile_notice = None;
    }

    pub(super) fn play_drawer_episode(
        &mut self,
        index: usize,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.controls.episodes.items().get(index).cloned() else {
            return;
        };
        if self.playing.as_ref().is_some_and(|i| i.id == item.id) {
            self.back_player_menu(w, cx);
            return;
        }
        self.play_episode(item, w, cx);
    }

    fn play_episode(&mut self, item: Item, w: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.downloads.playing.is_some() {
            let ready = self.downloads.profile.as_ref().and_then(|profile| {
                self.downloads
                    .records
                    .iter()
                    .find(|d| d.item.id == item.id && d.ready(profile))
            });
            if let Some(download) = ready.cloned() {
                self.play_download(download, w, cx);
            } else {
                self.controls.warning = "This episode is no longer available offline.".into();
                cx.notify();
            }
            return;
        }
        let Some(server) = self.server.clone() else {
            return;
        };
        let prefs = self.prefs.clone();
        let generation = self.generation;
        self.controls.warning.clear();
        self.work(
            w,
            cx,
            move || Ok(server.prepare(item, None, prefs, None)),
            move |s, prepared, w, cx| {
                if s.generation != generation {
                    return Ok(());
                }
                match prepared {
                    Ok(prepared) => {
                        s.release_player();
                        s.reset_episode_transition();
                        s.controls.queue = None;
                        s.play_prepared(prepared, w, cx)
                    }
                    Err(error) => {
                        s.controls.warning = format!("Could not open the episode. {error:#}");
                        Ok(())
                    }
                }
            },
        );
    }
    fn play_prepared(
        &mut self,
        prepared: Prepared,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if self.controls.source != prepared.request.source.id {
            self.controls.profile_notice = None;
        }
        self.controls.source = prepared.request.source.id.clone();
        self.controls.choice = prepared.choice;
        if !prepared.request.audio_only {
            self.controls.mini = false;
        }
        self.controls.hover = None;
        self.playing = Some(prepared.item);
        self.controls.segments = prepared.segments;
        self.controls.lyrics = prepared.lyrics;
        self.controls.lyric_line = None;
        self.controls.captions = prepared.captions;
        self.controls.warning = prepared.warning;
        self.controls.speed = 1.;
        self.controls.zoom = 1.;
        self.controls.delay = 0.;
        self.controls.subtitle_delay = 0.;
        self.controls.menu = PlayerMenu::Settings;
        self.start_player(prepared.request, w, cx)
    }
    pub(super) fn retry_playback(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(retry) = self.failure.take() else {
            self.refresh(w, cx);
            return;
        };
        if let Some(download) = retry.offline {
            self.play_download(download, w, cx);
            return;
        }
        let Some(server) = self.server.clone() else {
            return;
        };
        let prefs = self.prefs.clone();
        self.work(
            w,
            cx,
            move || {
                let mut prepared =
                    server.prepare(retry.item, Some(retry.position), prefs, Some(retry.choice))?;
                prepared.request.start_paused = retry.paused;
                Ok(prepared)
            },
            Self::play_prepared,
        );
    }
    pub(super) fn switch_tracks(
        &mut self,
        choice: MediaChoice,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.downloads.playing.is_some() {
            return;
        }
        let (Some(server), Some(item), Some(player)) =
            (self.server.clone(), self.playing.clone(), &self.player)
        else {
            return;
        };
        if !player.status.ready {
            return;
        }
        let id = item.id.clone();
        let prefs = self.prefs.clone();
        let key = choice.clone();
        let generation = self.generation;
        self.work(
            w,
            cx,
            move || server.prepare(item, None, prefs, Some(key)),
            move |s, mut prepared, w, cx| {
                let Some(p) = &s.player else { return Ok(()) };
                if s.generation != generation {
                    return Ok(());
                }
                prepared.request.resume = p.status.position;
                prepared.request.start_paused = p.status.paused;
                let open = p.panel_open;
                let menu = s.controls.menu;
                s.release_player();
                s.play_prepared(prepared, w, cx)?;
                if let Some(p) = s.player.as_mut() {
                    p.panel_open = open;
                    s.controls.menu = menu;
                    s.menu_focus.entering.set(open);
                    if let Some(server) = &s.server {
                        preferences::save(
                            &preferences::item_path(&server.profile_path()?, &id),
                            &choice,
                        )?;
                        s.formats.clear();
                    }
                    if matches!(&s.state.screen,Screen::Detail(i,_) if i.id==id) {
                        s.state.choice = choice;
                    }
                }
                Ok(())
            },
        )
    }
    pub(super) fn play_queue(
        &mut self,
        library: Option<CatalogQuery>,
        parent: Option<Item>,
        shuffle: bool,
        mix: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(server) = self.server.clone() else {
            return;
        };
        self.work(
            w,
            cx,
            move || {
                let mut items = if mix {
                    server.instant_mix(
                        &parent
                            .as_ref()
                            .context("Choose music for an Instant Mix.")?
                            .id,
                    )?
                } else if parent.as_ref().is_some_and(|p| p.kind == "MusicAlbum") && !shuffle {
                    let mut q = Vec::new();
                    let item = parent.as_ref().unwrap();
                    loop {
                        let page = server.children(item, None, q.len())?;
                        let n = page.items.len();
                        q.extend(page.items);
                        if n == 0 || q.len() >= page.total_record_count {
                            break;
                        }
                    }
                    q
                } else {
                    server.queue(library.as_ref(), parent.as_ref(), shuffle)?
                };
                let mut items = VecDeque::from(std::mem::take(&mut items));
                let first = items
                    .pop_front()
                    .context("There are no playable items in this selection.")?;
                Ok((first, items))
            },
            |s, (first, items), w, cx| {
                s.stop();
                s.controls = Controls::default();
                s.controls.queue = Some(items);
                s.begin(first, None, w, cx);
                Ok(())
            },
        );
    }
    pub(super) fn next_title(&self) -> Option<&Item> {
        if self.downloads.playing.is_some() {
            return self.downloads.queue.front().map(|d| &d.item);
        }
        match &self.controls.queue {
            Some(queue) => queue.front(),
            None => self.controls.next_episode.as_ref(),
        }
    }

    pub(super) fn episode_target(&self, direction: EpisodeDirection) -> Option<&Item> {
        match direction {
            EpisodeDirection::Previous => self.controls.previous_episode.as_ref(),
            EpisodeDirection::Next => self
                .next_title()
                .filter(|item| item.kind == "Episode")
                .or(self.controls.next_episode.as_ref()),
        }
    }

    pub(super) fn navigate_episode(
        &mut self,
        direction: EpisodeDirection,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy || self.player.is_none() {
            return;
        }
        let Some(item) = self.episode_target(direction).cloned() else {
            return;
        };
        self.controls.episode_hover = None;
        let queued = self.controls.queue.is_some()
            || self.downloads.playing.is_some() && !self.downloads.queue.is_empty();
        if direction == EpisodeDirection::Next
            && queued
            && self.next_title().is_some_and(|next| next.id == item.id)
        {
            self.advance(w, cx);
        } else {
            self.play_episode(item, w, cx);
        }
    }

    pub(super) fn load_next_episode(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.controls.next_lookup_finished {
            return;
        }
        let Some(item) = self.playing.clone().filter(|i| i.kind == "Episode") else {
            return;
        };
        let Some(server) = self.server.clone() else {
            return;
        };
        let generation = self.generation;
        let offline = self
            .downloads
            .playing
            .as_ref()
            .and(self.downloads.profile.as_ref())
            .map(|profile| {
                let candidates = self
                    .downloads
                    .records
                    .iter()
                    .filter(|d| {
                        d.item.kind == "Episode"
                            && d.item.series_id.is_some()
                            && d.item.series_id == item.series_id
                            && d.item.id != item.id
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                (profile.clone(), candidates)
            });
        let worker = cx.background_executor().spawn(async move {
            if let Some((profile, candidates)) = offline {
                let sort = |episode: &Item| episode.parent_index_number.zip(episode.index_number);
                let Some(at) = sort(&item) else {
                    return Ok((None, None));
                };
                let mut previous = None;
                let mut next = None;
                for download in candidates.into_iter().filter(|d| d.ready(&profile)) {
                    if let Some(key) = sort(&download.item) {
                        if key < at && previous.as_ref().is_none_or(|(then, _)| key > *then) {
                            previous = Some((key, download.item));
                        } else if key > at && next.as_ref().is_none_or(|(then, _)| key < *then) {
                            next = Some((key, download.item));
                        }
                    }
                }
                Ok((previous.map(|(_, i)| i), next.map(|(_, i)| i)))
            } else {
                server.episode_neighbors(&item)
            }
        });
        cx.spawn_in(w, async move |view, cx| {
            let result = worker.await;
            let _ = view.update(cx, |s, cx| {
                if s.generation == generation {
                    s.controls.next_lookup_finished = result.is_ok();
                    (s.controls.previous_episode, s.controls.next_episode) =
                        result.unwrap_or_default();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(super) fn credits_prompt(&self) -> Option<&Item> {
        let p = self.player.as_ref()?;
        if !p.status.ready
            || p.panel_open
            || self.scrub.is_some()
            || self.controls.watching_credits
            || self.controls.sleep_at_end
            || !self.playing.as_ref().is_some_and(|i| i.kind == "Episode")
            || !in_credits(
                p.status.position,
                p.status.duration,
                &self.controls.segments,
            )
        {
            return None;
        }
        self.next_title()
    }

    pub(super) fn watch_credits(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.controls.watching_credits = true;
        self.focus.focus(w);
        cx.notify();
    }

    pub(super) fn play_from_credits(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.credits_prompt().is_none() {
            return;
        }
        if let Some(player) = &mut self.player {
            player.status.ended = true;
            player.status.position = player.status.duration;
        }
        self.anime_finished(w, cx);
        self.advance(w, cx);
        cx.notify();
    }

    pub(super) fn advance(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.release_player();
        let offline = self.downloads.playing.take().is_some();
        if self.controls.sleep_at_end
            || self
                .controls
                .sleep
                .is_some_and(|time| Instant::now() >= time)
        {
            self.controls = Controls::default();
            self.downloads.queue.clear();
            return;
        }
        if offline {
            let mut queue = std::mem::take(&mut self.downloads.queue);
            if let Some(next) = queue.pop_front() {
                let mini = self.controls.mini;
                let deadline = self.controls.sleep;
                self.play_download(next, w, cx);
                self.downloads.queue = queue;
                self.controls.mini = mini && self.player.as_ref().is_some_and(|p| p.audio_only);
                self.controls.sleep = deadline;
            } else {
                self.controls = Default::default();
            }
            return;
        }
        if let Some(queue) = &mut self.controls.queue {
            if let Some(next) = queue.pop_front() {
                self.begin(next, None, w, cx);
            } else {
                self.controls = Controls::default();
            }
            return;
        }
        if let Some(next) = self.controls.next_episode.take() {
            self.begin(next, None, w, cx);
            return;
        }
        if self.prefs.autoplay
            && !self.controls.next_lookup_finished
            && let Some(item) = self.playing.clone()
            && item.kind == "Episode"
            && let Some(server) = self.server.clone()
        {
            self.work(
                w,
                cx,
                move || server.next_episode(&item),
                |s, next, w, cx| {
                    if let Some(next) = next {
                        s.begin(next, None, w, cx);
                    }
                    Ok(())
                },
            );
        }
    }
    pub(super) fn play_item_at(
        &mut self,
        item: Item,
        position: Option<f64>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !item.copies.is_empty() {
            self.navigate(
                Screen::Copies {
                    item: Box::new(item),
                    action: CopyAction::Play { position },
                },
                w,
                cx,
            );
            return;
        }
        if !self.adopt_server(&item, w, cx) {
            return;
        }
        if self.player.as_ref().is_some_and(|p| p.audio_only) {
            self.stop();
        }
        if self.busy || self.player.is_some() {
            return;
        }
        self.controls = Controls::default();
        self.begin(item, position, w, cx);
    }
}

fn validate_shader_file(path: &std::path::Path) -> Result<()> {
    use std::io::Read;
    const MAX_SIZE: u64 = 32 * 1024 * 1024;
    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    anyhow::ensure!(
        path.is_absolute()
            && (extension.eq_ignore_ascii_case("glsl") || extension.eq_ignore_ascii_case("hook")),
        "Choose an mpv .glsl or .hook file."
    );
    let mut text = String::new();
    std::fs::File::open(path)?
        .take(MAX_SIZE + 1)
        .read_to_string(&mut text)?;
    anyhow::ensure!(
        text.len() as u64 <= MAX_SIZE,
        "The shader file exceeds 32 MB."
    );
    anyhow::ensure!(
        text.trim_start_matches('\u{feff}')
            .lines()
            .any(|line| line.starts_with("//!HOOK ")),
        "The file does not contain an mpv shader hook."
    );
    Ok(())
}

fn in_credits(position: f64, duration: f64, segments: &[Segment]) -> bool {
    duration > 0.
        && position > 0.
        && (segments
            .iter()
            .any(|s| s.kind == "Outro" && s.active(position))
            || duration - position <= 15_f64.min(duration * 0.1))
}

#[cfg(test)]
mod tests {
    use super::{Controls, Segment, in_credits};

    fn marker(kind: &str, start: u64, end: u64) -> Segment {
        Segment {
            kind: kind.into(),
            start_ticks: start * 10_000_000,
            end_ticks: end * 10_000_000,
        }
    }

    #[test]
    fn credits_use_markers_and_a_short_tail_for_unmarked_episodes() {
        let markers = [marker("Outro", 100, 110)];
        assert!(in_credits(102., 300., &markers));
        assert!(!in_credits(111., 300., &markers)); // Post-credits scene.
        assert!(in_credits(289., 300., &[]));
        assert!(!in_credits(5., 10., &[]));
        assert!(!in_credits(0., 0., &[]));
    }

    #[test]
    fn credit_choice_preserves_auto_skip_and_watch_credits_overrides_it() {
        let mut controls = Controls {
            segments: vec![marker("Intro", 5, 15), marker("Outro", 100, 140)],
            ..Default::default()
        };
        assert_eq!(controls.auto_skip_target(6., true), Some(15.));
        assert_eq!(controls.auto_skip_target(7., true), None);
        assert_eq!(controls.auto_skip_target(101., true), None);
        assert_eq!(controls.auto_skip_target(101., true), None);
        assert_eq!(controls.auto_skip_target(110., true), None);
        assert_eq!(controls.auto_skip_target(111., true), Some(140.));
        controls.auto_skip_target(50., true);
        controls.watching_credits = true;
        assert_eq!(controls.auto_skip_target(120., false), None);
        controls.watching_credits = false;
        assert_eq!(controls.auto_skip_target(120., false), Some(140.));
    }
}
