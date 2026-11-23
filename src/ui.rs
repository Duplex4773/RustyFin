mod anime;
mod artwork;
mod captions;
mod catalog_wall;
mod customization;
mod downloads;
mod editor;
mod explore;
mod formats;
mod gamepad;
mod glass;
mod history;
mod scrolling;
mod services;
mod trickplay;
use downloads::Downloads;
use explore::Explore;
mod playback;
use crate::{
    jellyfin::MediaChoice,
    preferences::{self, Preferences},
};
use playback::{Controls, PlayerMenu};
mod screens;
use crate::{
    credentials,
    jellyfin::{CatalogQuery, Item, PAGE_SIZE, Page, PlayRequest, Route, Server, Shelf, ShelfKind},
    player::{OutputLost, Player},
};
use anyhow::{Context as _, Result};
use artwork::Artwork;
use gpui::{prelude::*, *};
use gpui_component::input::{Input, InputEvent, InputState};
use screens::SettingsPage;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Recommended,
    Browse,
    Collections,
    Playlists,
}

#[derive(Clone)]
enum Screen {
    Discover,
    Library(Tab, CatalogQuery),
    Search(String),
    Detail(Box<Item>, Option<String>),
    Settings(SettingsPage),
    Downloads,
    Explore,
    ExploreTitle(Box<crate::seerr::Entry>),
    Anime,
    AnimeTitle(u64),
    Copies { item: Box<Item>, play: bool },
    Live(bool),
    Guide(Box<Item>),
    Timers(bool),
}

impl Screen {
    fn page_size(&self) -> usize {
        if matches!(self, Self::Library(..)) {
            crate::jellyfin::LIBRARY_PAGE_SIZE
        } else {
            PAGE_SIZE
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum DetailPanel {
    More,
    Editions,
}

#[derive(Clone, Copy, PartialEq)]
enum SeriesFocus {
    Hero,
    Seasons,
    Episodes,
}

#[derive(Clone, Copy)]
enum LibraryReturn {
    Leaving(Instant),
    Arriving(Instant),
}

struct ViewState {
    screen: Screen,
    reload_on_return: bool,
    people_search: bool,
    page: Option<Page>,
    seasons: Vec<Item>,
    series_target: Option<Item>,
    reload_series_target: bool,
    extras: Vec<Item>,
    timers: Vec<crate::jellyfin::live::Timer>,
    choice: MediaChoice,
    info_open: bool,
    detail_panel: Option<DetailPanel>,
    series_focus: SeriesFocus,
    season_scroll: ScrollHandle,
    cast_start: usize,
    extra_start: usize,
    similar: Option<Result<Vec<Item>, String>>,
    similar_start: usize,
    similar_focus: Vec<FocusHandle>,
    return_to_similar: Option<usize>,
    editor: Option<editor::Editor>,
    subtitle_search: Option<editor::SubtitleSearch>,
    ratings: Option<std::result::Result<Vec<crate::services::Rating>, String>>,
    ratings_pending: bool,
    offset: usize,
    selection: usize,
    shelf: Option<usize>,
    scroll: ScrollHandle,
    wheel_motion: Option<scrolling::WheelMotion>,
    wall: catalog_wall::CatalogWall,
}

impl ViewState {
    fn new(screen: Screen) -> Self {
        Self {
            screen,
            reload_on_return: false,
            people_search: false,
            page: None,
            seasons: Vec::new(),
            series_target: None,
            reload_series_target: false,
            extras: vec![],
            timers: vec![],
            choice: Default::default(),
            info_open: false,
            detail_panel: None,
            series_focus: SeriesFocus::Hero,
            season_scroll: ScrollHandle::new(),
            cast_start: 0,
            extra_start: 0,
            similar: None,
            similar_start: 0,
            similar_focus: Vec::new(),
            return_to_similar: None,
            editor: None,
            subtitle_search: None,
            ratings: None,
            ratings_pending: false,
            offset: 0,
            selection: 0,
            shelf: None,
            scroll: ScrollHandle::new(),
            wheel_motion: None,
            wall: Default::default(),
        }
    }
}

pub struct Library {
    address: Entity<InputState>,
    search: Entity<InputState>,
    focus: FocusHandle,
    more_focus: FocusHandle,
    edition_focus: FocusHandle,
    detail_panel_focus: FocusHandle,
    detail_current_focus: FocusHandle,
    server: Option<Server>,
    code: Option<String>,
    nearby: Vec<crate::jellyfin::discovery::Nearby>,
    busy: bool,
    message: String,
    job: Option<Task<()>>,
    playback_poll: Option<Task<()>>,
    flag_job: Option<Task<()>>,
    home_dirty: bool,
    search_delay: Option<Task<()>>,
    state: ViewState,
    history: history::History,
    return_fade: Option<LibraryReturn>,
    shelves: Vec<Shelf>,
    libraries: Vec<Item>,
    library_list_job: Option<Task<()>>,
    similar_job: Option<Task<()>>,
    featured: Vec<Item>,
    hero: usize,
    rotate: bool,
    hdr: Option<crate::player::HdrSettings>,
    last_slide: Instant,
    artwork: Artwork,
    formats: formats::CardFormats,
    navigation_focus: FocusHandle,
    shelf_light: glass::ShelfLight,
    menu_focus: playback::MenuFocus,
    filters_open: bool,
    browse_menu: Option<(&'static str, Entity<gpui_component::menu::PopupMenu>)>,
    controls: Controls,
    prefs: Preferences,
    accounts: Vec<crate::jellyfin::Account>,
    connected: Vec<Server>,
    explore: Explore,
    services: services::Services,
    anime: anime::Anime,
    downloads: Downloads,
    keyboard: bool,
    rebind: Option<&'static str>,
    shelf_scroll: Vec<ScrollHandle>,
    scrub: Option<f32>,
    volume: f64,
    unmuted_volume: f64,
    playing: Option<Item>,
    player: Option<Player>,
    failure: Option<playback::Retry>,
    generation: u64,
}

impl Library {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let address =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://jellyfin.example.com"));
        let search = cx
            .new(|cx| InputState::new(window, cx).placeholder("Search movies, shows and episodes"));
        cx.subscribe_in(&address, window, |this, _, event, w, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.connect(w, cx);
            }
        })
        .detach();
        cx.subscribe_in(&search, window, |this, input, event, w, cx| {
            if !matches!(this.state.screen, Screen::Search(_)) {
                return;
            }
            match event {
                InputEvent::Change => {
                    let term = input.read(cx).value().trim().to_string();
                    if matches!(&this.state.screen, Screen::Search(current) if current == &term) {
                        return;
                    }
                    this.history.clear_forward();
                    this.search_delay = None;
                    this.job = None;
                    this.busy = !term.is_empty();
                    let people = this.state.people_search;
                    this.state = ViewState::new(Screen::Search(term));
                    this.state.people_search = people;
                    this.message.clear();
                    this.search_delay = Some(cx.spawn_in(w, async move |view, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(250))
                            .await;
                        let _ = view.update_in(cx, |s, w, cx| {
                            s.search_delay = None;
                            s.load(w, cx);
                        });
                    }));
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.search_delay = None;
                    this.focus.focus(w);
                    this.keyboard = true;
                    this.load(w, cx);
                }
                _ => {}
            }
        })
        .detach();
        let mut message = String::new();
        let server = credentials::load()
            .and_then(|a| a.map(Server::restore).transpose())
            .unwrap_or_else(|e| {
                message = format!("Could not restore your sign-in. {e}");
                None
            });
        let mut view = Self {
            focus: cx.focus_handle(),
            more_focus: cx.focus_handle(),
            edition_focus: cx.focus_handle(),
            detail_panel_focus: cx.focus_handle(),
            detail_current_focus: cx.focus_handle(),
            address,
            search,
            server,
            message,
            code: None,
            nearby: vec![],
            busy: false,
            job: None,
            playback_poll: None,
            flag_job: None,
            home_dirty: false,
            search_delay: None,
            state: ViewState::new(Screen::Discover),
            history: Default::default(),
            return_fade: None,
            shelves: vec![],
            libraries: vec![],
            library_list_job: None,
            similar_job: None,
            featured: vec![],
            hero: 0,
            rotate: true,
            hdr: None,
            last_slide: Instant::now(),
            artwork: Artwork::default(),
            formats: Default::default(),
            navigation_focus: cx.focus_handle(),
            shelf_light: Default::default(),
            menu_focus: playback::MenuFocus::new(cx),
            filters_open: false,
            browse_menu: None,
            controls: Default::default(),
            prefs: Default::default(),
            accounts: vec![],
            connected: vec![],
            explore: Explore::new(window, cx),
            services: services::Services::new(window, cx),
            anime: anime::Anime::new(window, cx),
            downloads: Default::default(),
            shelf_scroll: vec![],
            keyboard: false,
            rebind: None,
            scrub: None,
            volume: 1.,
            unmuted_volume: 1.,
            playing: None,
            player: None,
            failure: None,
            generation: 0,
        };
        gamepad::listen(window, cx);
        view.load_preferences();
        if view.server.is_some() {
            view.focus.focus(window);
            view.load(window, cx);
        } else {
            view.address.read(cx).focus_handle(cx).focus(window);
        }
        cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if view
                    .update_in(cx, |s, w, cx| {
                        s.download_tick(w, cx);
                        s.update_presence();
                        if s.rotate
                            && !s.keyboard
                            && matches!(s.state.screen, Screen::Discover)
                            && s.state.scroll.offset().y > px(-40.)
                            && s.player.is_none()
                            && !s.busy
                            && s.featured.len() > 1
                            && s.last_slide.elapsed() >= Duration::from_secs(9)
                        {
                            s.set_hero((s.hero + 1) % s.featured.len(), cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        view
    }

    // Keep just the current foreground operation. Replacing its task cancels
    // delivery of stale results after navigation or an account change.
    fn work<T: Send + 'static>(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
        work: impl FnOnce() -> Result<T> + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Window, &mut Context<Self>) -> Result<()> + 'static,
    ) {
        self.message.clear();
        self.failure = None;
        self.busy = true;
        let worker = cx.background_executor().spawn(async move { work() });
        self.job = Some(cx.spawn_in(window, async move |view, cx| {
            let result = worker.await;
            let _ = view.update_in(cx, |this, window, cx| {
                this.busy = false;
                if let Err(e) = result.and_then(|v| done(this, v, window, cx)) {
                    if let Some(p) = this.player.as_mut().filter(|_| !this.controls.mini) {
                        this.controls.warning = format!("{e:#}");
                        p.panel_open = true;
                        this.menu_focus.origin = PlayerMenu::Settings;
                        this.menu_focus.entering.set(true);
                        this.controls.menu = PlayerMenu::Settings;
                        p.controls_visible = true;
                    } else {
                        this.message = format!("{e:#}");
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.server.is_some() || self.code.is_some() {
            return;
        }
        let address = self.address.read(cx).value().to_string();
        self.work(window, cx, move || {
            let server = Server::new(&address, uuid::Uuid::new_v4().to_string())?;
            let pair = server.pair()?;
            Ok((server, pair))
        }, |this, (server, pair), window, cx| {
            this.code = Some(pair.code);
            this.focus.focus(window);
            this.job = Some(cx.spawn_in(window, async move |view, cx| {
                let started = Instant::now();
                let signed_in = loop {
                    cx.background_executor().timer(Duration::from_secs(2)).await;
                    if started.elapsed() >= Duration::from_secs(300) { break Err(anyhow::anyhow!("The pairing code expired. Request a new code.")); }
                    let server = server.clone();
                    let secret = pair.secret.clone();
                    match cx.background_executor().spawn(async move { server.claim(&secret) }).await {
                        Ok(Some(account)) => break Ok(account),
                        Ok(None) => {}
                        Err(e) => break Err(e),
                    }
                };
                let _ = view.update_in(cx, |this, window, cx| {
                    this.code = None;
                    match signed_in.and_then(|a| {
                        let server = Server::restore(a.clone())?;
                        let saved = credentials::save(&a);
                        Ok((server, saved))
                    }) {
                        Ok((server, saved)) => {
                            this.server = Some(server);
                            this.load_preferences();
                            this.load(window, cx);
                            if let Err(e) = saved { this.message = format!("Signed in for this session. Windows could not save the credential. {e}"); }
                        }
                        Err(e) => this.message = format!("{e:#}"),
                    }
                    cx.notify();
                });
            }));
            Ok(())
        });
    }

    fn cancel_pairing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.job = None;
        self.code = None;
        self.busy = false;
        self.address.read(cx).focus_handle(cx).focus(window);
        cx.notify();
    }

    fn sign_out(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = credentials::clear() {
            self.message = format!("Could not clear the saved sign-in. {e}");
            cx.notify();
            return;
        }
        self.clear_session(window, cx);
    }

    fn clear_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.similar_job = None;
        self.return_fade = None;
        self.stop();
        self.failure = None;
        self.rebind = None;
        self.server = None;
        self.explore = Explore::new(window, cx);
        self.services = services::Services::new(window, cx);
        self.anime = anime::Anime::new(window, cx);
        self.controls = Default::default();
        self.load_preferences();
        self.flag_job = None;
        self.home_dirty = false;
        self.history.clear();
        self.shelves.clear();
        self.featured.clear();
        self.libraries.clear();
        self.library_list_job = None;
        self.artwork.clear(window, cx);
        self.playing = None;
        self.search_delay = None;
        self.state = ViewState::new(Screen::Discover);
        self.search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.message.clear();
        self.cancel_pairing(window, cx);
    }

    fn choose_account(
        &mut self,
        a: crate::jellyfin::Account,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match Server::restore(a.clone()).and_then(|server| {
            credentials::save(&a)?;
            Ok(server)
        }) {
            Ok(server) => {
                self.clear_session(w, cx);
                self.server = Some(server);
                self.load_preferences();
                self.navigate(Screen::Discover, w, cx);
            }
            Err(e) => {
                self.message = e.to_string();
                cx.notify();
            }
        }
    }

    fn leave_view(&mut self) {
        self.state.wheel_motion = None;
        self.state.reload_on_return |=
            self.busy || self.search_delay.is_some() || !self.message.is_empty();
        self.similar_job = None;
        self.return_fade = None;
        self.formats.expanded = None;
        self.search_delay = None;
        self.job = None;
        self.busy = false;
        self.message.clear();
        self.failure = None;
        self.rebind = None;
        self.filters_open = false;
        self.browse_menu = None;
        self.keyboard = false;
        self.state.wall.suspend();
        self.last_slide = Instant::now();
    }

    fn navigate(&mut self, screen: Screen, window: &mut Window, cx: &mut Context<Self>) {
        self.leave_view();
        self.history.visit(&mut self.state, screen);
        if matches!(self.state.screen, Screen::Search(_)) {
            self.search.read(cx).focus_handle(cx).focus(window);
        } else {
            self.focus.focus(window);
        }
        self.load(window, cx);
    }

    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.state.detail_panel.take() {
            self.detail_launcher(panel).focus(window);
            cx.notify();
            return;
        }
        if matches!(self.state.screen, Screen::Detail(..))
            && self
                .history
                .back
                .last()
                .is_some_and(|s| matches!(s.screen, Screen::Library(..)))
        {
            self.return_fade
                .get_or_insert_with(|| LibraryReturn::Leaving(Instant::now()));
            cx.notify();
            return;
        }
        self.return_fade = None;
        self.restore_view(NavigationDirection::Back, window, cx);
    }

    fn return_opacity(&mut self, window: &mut Window, cx: &mut Context<Self>) -> f32 {
        let Some(transition) = self.return_fade else {
            return 1.;
        };
        let eased = |start: Instant, seconds: f32| {
            let t = (start.elapsed().as_secs_f32() / seconds).min(1.);
            t * t * (3. - 2. * t)
        };
        window.request_animation_frame();
        match transition {
            LibraryReturn::Leaving(start) => {
                let opacity = 1. - eased(start, 0.085);
                if opacity <= 0. {
                    self.restore_view(NavigationDirection::Back, window, cx);
                    self.return_fade = Some(LibraryReturn::Arriving(Instant::now()));
                }
                opacity
            }
            LibraryReturn::Arriving(start) => {
                let opacity = eased(start, 0.12);
                if opacity >= 1. {
                    self.return_fade = None;
                }
                opacity
            }
        }
    }

    fn restore_view(
        &mut self,
        direction: NavigationDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.history.can_go(direction) {
            return;
        }
        self.leave_view();
        self.history.go(&mut self.state, direction);
        self.keyboard = matches!(self.state.screen, Screen::Library(..));
        self.state.ratings_pending = false;
        if matches!(self.state.screen, Screen::Discover)
            && let Some(row) = self.state.shelf
        {
            if let Some(shelf) = self.shelves.get(row) {
                self.state.selection = self
                    .state
                    .selection
                    .min(shelf.items.len().saturating_sub(1));
            } else {
                self.state.shelf = None;
                self.state.selection = 0;
            }
        }
        if let Screen::Search(term) = &self.state.screen
            && self.search.read(cx).value().trim() != term
        {
            self.search
                .update(cx, |input, cx| input.set_value(term.clone(), window, cx));
        }
        if std::mem::take(&mut self.state.reload_on_return)
            || matches!(
                self.state.screen,
                Screen::Explore | Screen::ExploreTitle(_) | Screen::Anime | Screen::AnimeTitle(_)
            )
            || matches!(&self.state.screen, Screen::Detail(i,_) if i.kind == "Series" && self.state.series_target.is_none())
        {
            self.load(window, cx);
        }
        if let Some(index) = self.state.return_to_similar.take() {
            self.focus_similar(index, window);
        } else {
            self.focus.focus(window);
        }
        cx.notify();
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(server) = self.server.clone() else {
            return;
        };
        // The top bar still needs library shortcuts when navigation cancels the page request.
        if self.library_list_job.is_none() {
            let source = server.clone();
            let worker = cx
                .background_executor()
                .spawn(async move { source.libraries() });
            self.library_list_job = Some(cx.spawn_in(window, async move |view, cx| {
                let result = worker.await;
                let _ = view.update_in(cx, |s, _, cx| {
                    match result {
                        Ok(libraries) => s.libraries = libraries,
                        Err(_) => s.library_list_job = None,
                    }
                    cx.notify();
                });
            }));
        }
        let offset = self.state.offset;
        match self.state.screen.clone() {
            Screen::Discover => {
                if !self.shelves.is_empty() && !self.home_dirty {
                    self.busy = false;
                    cx.notify();
                    return;
                }
                self.home_dirty = false;
                let others = if self.prefs.all_servers {
                    self.connected.clone()
                } else {
                    vec![]
                };
                self.work(
                    window,
                    cx,
                    move || crate::jellyfin::federation::discover(&server, &others),
                    |s, (shelves, unavailable), _, _| {
                        if !unavailable.is_empty() {
                            s.message = format!("Unavailable servers · {}", unavailable.join(", "));
                        }
                        s.featured = screens::featured_titles(&shelves);
                        s.state.shelf = None;
                        s.state.selection = 0;
                        s.hero = 0;
                        s.last_slide = Instant::now();
                        s.shelves = shelves
                            .into_iter()
                            .filter(|r| !r.items.is_empty())
                            .collect();
                        s.shelf_scroll = s.shelves.iter().map(|_| ScrollHandle::new()).collect();
                        Ok(())
                    },
                );
            }
            Screen::Settings(SettingsPage::Display) => self.read_display(window, cx),
            Screen::Settings(_) => cx.notify(),
            Screen::Explore => self.load_explore(window, cx),
            Screen::ExploreTitle(entry) => self.load_explore_title(*entry, window, cx),
            Screen::Anime => self.load_anime(window, cx),
            Screen::AnimeTitle(id) => self.load_anime_title(id, window, cx),
            Screen::Live(favorites) => {
                let servers = self.browse_servers();
                self.work(
                    window,
                    cx,
                    move || {
                        crate::jellyfin::federation::combined_page(&servers, offset, |s, start| {
                            s.channels(favorites, start)
                        })
                    },
                    Self::receive_combined,
                );
            }
            Screen::Guide(channel) => self.work(
                window,
                cx,
                move || server.schedule(&channel.id, offset),
                Self::receive_page,
            ),
            Screen::Timers(series) => self.work(
                window,
                cx,
                move || server.timers(series),
                |s, list, _, _| {
                    s.state.timers = list;
                    Ok(())
                },
            ),
            Screen::Library(..) => cx.notify(),
            Screen::Search(query) if !query.is_empty() => {
                let servers = self.browse_servers();
                let people = self.state.people_search;
                self.work(
                    window,
                    cx,
                    move || crate::jellyfin::federation::search(&servers, &query, people, offset),
                    Self::receive_combined,
                );
            }
            Screen::Detail(item, _) if item.playable() => {
                self.work(
                    window,
                    cx,
                    move || {
                        let full = server.item(&item.id)?;
                        let choice = preferences::load(&preferences::item_path(
                            &server.profile_path()?,
                            &item.id,
                        ))?;
                        let extras = server.extras(&full)?;
                        Ok((full, choice, extras))
                    },
                    |s, (item, choice, extras), _, _| {
                        s.state.screen = Screen::Detail(Box::new(item), None);
                        s.state.choice = choice;
                        s.state.extras = extras;
                        Ok(())
                    },
                );
            }
            Screen::Detail(item, season) if !item.playable() => {
                let known = self.state.seasons.clone();
                let hydrate = self.state.page.is_none() && offset == 0 && known.is_empty();
                let origin = item
                    .origin
                    .clone()
                    .or_else(|| server.identity().map(|a| a.key()));
                let cached_target = self.state.series_target.clone().or_else(|| {
                    if self.home_dirty {
                        return None;
                    }
                    self.shelves
                        .iter()
                        .find(|r| r.kind == ShelfKind::Continue)?
                        .items
                        .iter()
                        .find(|ep| ep.series_id.as_ref() == Some(&item.id) && ep.origin == origin)
                        .cloned()
                });
                self.work(
                    window,
                    cx,
                    move || {
                        let item = if hydrate {
                            Box::new(server.item(&item.id).unwrap_or(*item))
                        } else {
                            item
                        };
                        let seasons = if item.kind == "Series" && known.is_empty() {
                            server.seasons(&item.id)?
                        } else {
                            known
                        };
                        let series = item.kind == "Series";
                        let mut target = cached_target;
                        if series && target.is_none() {
                            target = server.series_target(&item.id)?;
                        }
                        let first_season = seasons
                            .iter()
                            .find(|s| s.index_number != Some(0))
                            .or_else(|| seasons.first());
                        let season = season
                            .or_else(|| {
                                target.as_ref().and_then(|ep| {
                                    ep.season_id.clone().or_else(|| {
                                        seasons
                                            .iter()
                                            .find(|s| {
                                                ep.parent_index_number.is_some()
                                                    && s.index_number == ep.parent_index_number
                                            })
                                            .map(|s| s.id.clone())
                                    })
                                })
                            })
                            .or_else(|| first_season.map(|s| s.id.clone()));
                        let (page, start) = server.children_page(
                            &item,
                            season.as_deref(),
                            offset,
                            target.as_ref().filter(|_| hydrate),
                        )?;
                        if series && target.is_none() {
                            let default_season = first_season.map(|s| s.id.as_str());
                            let fallback;
                            let episodes = if offset == 0 && season.as_deref() == default_season {
                                &page.items
                            } else {
                                fallback = server.children(&item, default_season, 0)?;
                                &fallback.items
                            };
                            target = episodes
                                .iter()
                                .find(|ep| {
                                    ep.playable()
                                        && ep.user_data.as_ref().and_then(|u| u.played)
                                            != Some(true)
                                })
                                .or_else(|| episodes.iter().find(|ep| ep.playable()))
                                .cloned();
                        }
                        let extras =
                            (series && hydrate).then(|| server.extras(&item).unwrap_or_default());
                        Ok((page, seasons, season, item, target, start, extras))
                    },
                    move |s, (page, seasons, season, item, target, start, extras), w, cx| {
                        if let Some(extras) = extras {
                            s.state.extras = extras;
                        }
                        s.state.screen = Screen::Detail(item, season);
                        s.state.seasons = seasons;
                        if let Screen::Detail(_, Some(id)) = &s.state.screen
                            && let Some(index) =
                                s.state.seasons.iter().position(|part| &part.id == id)
                        {
                            s.state.season_scroll.scroll_to_item(index);
                        }
                        if hydrate {
                            s.state.selection = target
                                .as_ref()
                                .and_then(|ep| page.items.iter().position(|i| i.id == ep.id))
                                .unwrap_or(0);
                        }
                        s.state.offset = start;
                        s.state.series_target = target;
                        s.state.reload_series_target = false;
                        Self::receive_page(s, page, w, cx)
                    },
                );
            }
            _ => {
                self.busy = false;
                cx.notify();
            }
        }
    }

    fn browse_servers(&self) -> Vec<Server> {
        let mut servers: Vec<_> = self.server.clone().into_iter().collect();
        if self.prefs.all_servers {
            servers.extend(self.connected.clone());
        }
        servers
    }
    fn source_server(&self, origin: Option<&str>) -> Option<Server> {
        if let Some(key) = origin {
            return self
                .server
                .iter()
                .chain(&self.connected)
                .find(|s| s.identity().is_some_and(|a| a.key() == key))
                .cloned();
        }
        self.server.clone()
    }
    fn receive_combined(
        &mut self,
        (page, missing): (Page, Vec<String>),
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if !missing.is_empty() {
            self.message = format!("Unavailable servers · {}", missing.join(", "));
        }
        Self::receive_page(self, page, w, cx)
    }
    fn receive_page(&mut self, page: Page, _: &mut Window, _: &mut Context<Self>) -> Result<()> {
        self.state.page = Some(page);
        Ok(())
    }

    fn refresh(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.similar_job = None;
        self.state.similar = None;
        if matches!(self.state.screen, Screen::Library(..)) {
            self.state.wall.invalidate();
        }
        self.library_list_job = None;
        self.formats.clear();
        self.artwork.retry();
        if matches!(self.state.screen, Screen::Discover) {
            self.shelves.clear();
            self.state.shelf = None;
        }
        if matches!(&self.state.screen, Screen::Detail(i, _) if i.kind == "Series") {
            self.state.series_target = None;
            self.home_dirty = true;
        }
        self.load(w, cx);
    }

    fn turn_page(&mut self, forward: bool, w: &mut Window, cx: &mut Context<Self>) {
        let Some(page) = &self.state.page else { return };
        let offset = if forward {
            self.state.offset + self.state.screen.page_size()
        } else {
            self.state
                .offset
                .saturating_sub(self.state.screen.page_size())
        };
        if offset == self.state.offset || offset >= page.total_record_count {
            return;
        }
        self.state.offset = offset;
        self.state.selection = 0;
        self.state.page = None;
        self.state.scroll = ScrollHandle::new();
        self.state.wheel_motion = None;
        self.load(w, cx);
    }

    fn set_hero(&mut self, index: usize, cx: &mut Context<Self>) {
        self.hero = index;
        self.last_slide = Instant::now();
        cx.notify();
    }

    fn adopt_server(&mut self, item: &Item, w: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(key) = &item.origin else { return true };
        if self
            .server
            .as_ref()
            .and_then(Server::identity)
            .is_some_and(|a| &a.key() == key)
        {
            return true;
        }
        let Some(a) = self.accounts.iter().find(|a| &a.key() == key).cloned() else {
            self.message = "Pair this Jellyfin server again to open this title.".into();
            cx.notify();
            return false;
        };
        match Server::restore(a.clone()).and_then(|server| {
            credentials::save(&a)?;
            Ok(server)
        }) {
            Ok(server) => {
                self.stop();
                self.flag_job = None;
                self.history.clear();
                self.state = ViewState::new(Screen::Discover);
                self.shelves.clear();
                self.featured.clear();
                self.services = services::Services::new(w, cx);
                self.anime = anime::Anime::new(w, cx);
                self.server = Some(server);
                self.libraries.clear();
                self.library_list_job = None;
                self.home_dirty = true;
                self.artwork.clear(w, cx);
                self.explore = Explore::new(w, cx);
                self.load_preferences();
                true
            }
            Err(e) => {
                self.message = e.to_string();
                cx.notify();
                false
            }
        }
    }
    fn open_item(&mut self, item: Item, w: &mut Window, cx: &mut Context<Self>) {
        self.state.return_to_similar = self
            .state
            .similar
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .and_then(|titles| {
                titles
                    .iter()
                    .position(|i| i.id == item.id && i.origin == item.origin)
            });
        if matches!(self.state.screen, Screen::Library(..))
            && let Some(index) = self.state.wall.index_of(&item.id)
        {
            self.state.selection = index;
        }
        if !item.copies.is_empty() {
            self.navigate(
                Screen::Copies {
                    item: Box::new(item),
                    play: false,
                },
                w,
                cx,
            );
        } else if self.adopt_server(&item, w, cx) {
            self.navigate(Screen::Detail(Box::new(item), None), w, cx);
        }
    }
    fn play_item(&mut self, item: Item, w: &mut Window, cx: &mut Context<Self>) {
        if !item.copies.is_empty() {
            self.navigate(
                Screen::Copies {
                    item: Box::new(item),
                    play: true,
                },
                w,
                cx,
            );
        } else if self.adopt_server(&item, w, cx) {
            self.play_item_at(item, None, w, cx);
        }
    }
    fn start_player(
        &mut self,
        request: PlayRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if self
            .controls
            .profile_notice
            .is_some_and(|(text, _)| Some(text) != request.route.startup_notice())
        {
            self.controls.profile_notice = None;
        }
        self.artwork.trickplay.clear(window, cx);
        self.message.clear();
        self.failure = None;
        let server = self.server.clone().context("Sign in before playback.")?;
        match Player::open(
            window,
            &request,
            server,
            self.playing.as_ref().context("Missing title")?,
            &self.prefs,
        ) {
            Ok(mut player) => {
                if !self.controls.captions.is_empty() {
                    player.caption_height = (self.prefs.subtitle_size as f32 * 1.7 + 12.).max(52.);
                }
                self.player = Some(player);
            }
            Err(error) => {
                self.playback_error(&request, error, window, cx);
                return Ok(());
            }
        }
        self.focus.focus(window);
        self.load_next_episode(window, cx);
        self.playback_poll = Some(cx.spawn_in(window, async move |view, cx| {
            let mut request = request;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let running = view.update_in(cx, |this, window, cx| {
                    let has_next = this.next_title().is_some();
                    let lookup_pending = this.downloads.playing.is_none()
                        && this.controls.queue.is_none()
                        && !this.controls.next_lookup_finished;
                    let Some(player) = this.player.as_mut() else {
                        return false;
                    };
                    let previous = player.status;
                    match player.poll(window) {
                        Err(error) => {
                            if player.status.ready && player.status.position.is_finite() {
                                request.resume = player.status.position;
                                request.start_paused = player.status.paused;
                            }
                            this.release_player();
                            this.playback_error(&request, error, window, cx);
                        }
                        Ok(()) if player.status.ended => {
                            this.anime_finished(window, cx);
                            if this.controls.watching_credits
                                || (this.controls.queue.is_none()
                                    && this.downloads.queue.is_empty()
                                    && !this.prefs.autoplay)
                            {
                                this.stop();
                            } else {
                                this.advance(window, cx);
                            }
                        }
                        Ok(()) => {
                            if this.controls.profile_notice.is_none()
                                && let Some(message) = request.route.startup_notice()
                                && previous.ready
                                && player.status.ready
                                && !player.status.paused
                                && player.status.position > previous.position
                            {
                                this.controls.profile_notice = Some((message, Instant::now()));
                            }
                            if this.prefs.auto_skip && player.status.ready && !player.status.paused
                            {
                                let offer_choice =
                                    this.playing.as_ref().is_some_and(|i| i.kind == "Episode")
                                        && (has_next || lookup_pending)
                                        && !this.controls.sleep_at_end;
                                if let Some(end) = this
                                    .controls
                                    .auto_skip_target(player.status.position, offer_choice)
                                    && let Err(e) = player.seek(end - player.status.position)
                                {
                                    this.controls.warning = e.to_string();
                                }
                            }
                            if this.controls.sleep.is_some_and(|t| Instant::now() >= t) {
                                this.controls.sleep = None;
                                if !player.status.paused
                                    && let Err(e) = player.toggle()
                                {
                                    this.controls.warning = e.to_string();
                                }
                            }
                            if player.pcm_volume.is_some_and(|v| v != this.volume)
                                && let Err(e) = player.set_volume(this.volume)
                            {
                                this.volume = player.pcm_volume.unwrap_or(1.);
                                this.message = format!("Could not change the playback volume. {e}");
                            }
                            if player.audio_only {
                                let now = (player.status.position * 10_000_000.) as u64;
                                let line = this
                                    .controls
                                    .lyrics
                                    .iter()
                                    .rposition(|l| l.start.is_some_and(|start| start <= now));
                                if line != this.controls.lyric_line {
                                    this.controls.lyric_line = line;
                                    if let Some(i) = line {
                                        this.controls.lyric_scroll.scroll_to_item(i);
                                    }
                                }
                            }
                            use crate::player::transport::MediaCommand;
                            let next = has_next;
                            let command = player
                                .transport
                                .as_mut()
                                .and_then(|t| t.poll(player.status.paused, next));
                            match command {
                                Some(MediaCommand::Next) if next => {
                                    this.advance(window, cx);
                                    cx.notify();
                                    return false;
                                }
                                Some(MediaCommand::Stop) => {
                                    this.stop();
                                    cx.notify();
                                    return false;
                                }
                                Some(MediaCommand::Restart) => {
                                    let _ = player.seek(-player.status.position);
                                }
                                Some(MediaCommand::Play) if player.status.paused => {
                                    let _ = player.toggle();
                                }
                                Some(MediaCommand::Pause) if !player.status.paused => {
                                    let _ = player.toggle();
                                }
                                _ => {}
                            }
                            let ticks = (player.status.position * 10_000_000.) as u64;
                            if player.status.ready && !player.status.paused {
                                this.checkpoint_download(ticks, false, false);
                            }
                            cx.notify();
                            return true;
                        }
                    }
                    cx.notify();
                    false
                });
                if !running.unwrap_or(false) {
                    break;
                }
            }
        }));
        Ok(())
    }

    fn playback_error(
        &mut self,
        request: &PlayRequest,
        error: anyhow::Error,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if request.route == Route::Mel81
            && error
                .downcast_ref::<crate::player::LayerError>()
                .is_some_and(|e| e.is_fel())
        {
            let mut retry = request.clone();
            retry.route = Route::FelHdr10;
            self.controls.captions = Default::default();
            self.controls.warning.clear();
            if let Err(e) = self.start_player(retry, window, cx) {
                self.message = format!("{e:#}");
            }
        } else if request.route.native_profile().is_some()
            && let Some(reason) = error.downcast_ref::<OutputLost>()
        {
            let mut retry = request.clone();
            retry.allow_passthrough =
                self.prefs.passthrough && matches!(reason, OutputLost::DeviceChanged);
            if let Err(e) = self.start_player(retry, window, cx) {
                self.message = format!("{e:#}");
            }
        } else {
            self.failure = self.playing.as_ref().map(|item| playback::Retry {
                item: item.clone(),
                choice: MediaChoice {
                    source: (item.kind != "TvChannel").then(|| request.source.clone()),
                    audio: request.audio_index,
                    subtitle: Some(request.subtitle_index.unwrap_or(-1)),
                },
                position: request.resume,
                paused: request.start_paused,
                offline: self.downloads.playing.clone(),
                dolby: request.route.native_profile().is_some(),
            });
            self.message = format!("{} · {error:#}", request.format);
        }
        cx.notify();
    }

    fn stop(&mut self) {
        self.release_player();
        self.downloads.queue.clear();
        self.downloads.playing = None;
        self.controls = Default::default();
    }

    fn release_player(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.end_preview();
        let Some(player) = self.player.take() else {
            return;
        };
        let Some(id) = self.playing.as_ref().map(|i| i.id.clone()) else {
            return;
        };
        if !player.status.ready {
            return;
        }
        let ticks = if player.status.ended {
            0
        } else {
            (player.status.position * 10_000_000.) as u64
        };
        self.checkpoint_download(ticks, player.status.ended, true);
        self.home_dirty = true;
        if let Some(episode) = self.playing.as_ref().filter(|i| i.kind == "Episode") {
            for state in self
                .history
                .iter_mut()
                .chain(std::iter::once(&mut self.state))
            {
                if matches!(&state.screen, Screen::Detail(show, _) if episode.series_id.as_ref() == Some(&show.id))
                {
                    state.reload_series_target =
                        player.status.ended && self.controls.next_episode.is_none();
                    state.series_target = if player.status.ended {
                        self.controls.next_episode.clone()
                    } else {
                        Some(episode.clone())
                    };
                }
            }
        }
        self.update_items(|item| {
            if item.id == id {
                let data = item.user_data.get_or_insert_with(Default::default);
                data.playback_position_ticks = Some(if player.status.ended {
                    0
                } else {
                    (player.status.position * 10_000_000.) as u64
                });
                if player.status.ended {
                    data.played = Some(true);
                }
            }
        });
    }

    fn toggle_flag(&mut self, favorite: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Screen::Detail(item, _) = &self.state.screen else {
            return;
        };
        let Some(server) = self.server.clone() else {
            return;
        };
        if self.flag_job.is_some() {
            return;
        }
        let id = item.id.clone();
        let old = item
            .user_data
            .as_ref()
            .and_then(|d| if favorite { d.is_favorite } else { d.played })
            .unwrap_or(false);
        let target = id.clone();
        let worker = cx
            .background_executor()
            .spawn(async move { server.set_user_flag(&target, favorite, !old) });
        self.flag_job = Some(cx.spawn_in(window, async move |view, cx| {
            let result = worker.await;
            let _ = view.update(cx, |s, cx| {
                s.flag_job = None;
                match result {
                    Ok(data) => {
                        s.update_items(|item| {
                            if item.id == id {
                                item.user_data = Some(data.clone());
                            }
                        });
                        if !favorite && let Screen::Detail(ep, _) = &s.state.screen && ep.kind == "Episode" {
                            for view in s.history.iter_mut() {
                                if matches!(&view.screen, Screen::Detail(series, _) if Some(&series.id) == ep.series_id.as_ref()) {
                                    view.series_target = None;
                                    view.reload_series_target = true;
                                }
                            }
                        }
                        s.home_dirty = true;
                    }
                    Err(e) => s.message = format!("Could not update this title. {e:#}"),
                }
                cx.notify();
            });
        }));
    }

    fn update_items(&mut self, update: impl Fn(&mut Item)) {
        for state in self
            .history
            .iter_mut()
            .chain(std::iter::once(&mut self.state))
        {
            if let Screen::Detail(item, _) = &mut state.screen {
                update(item);
            }
            if let Some(target) = &mut state.series_target {
                update(target);
            }
            if let Some(page) = &mut state.page {
                page.items.iter_mut().for_each(&update);
            }
            if let Screen::Library(_, q) = &state.screen {
                state.wall.update_items(q, &update);
            }
        }
        self.featured.iter_mut().for_each(&update);
        for shelf in &mut self.shelves {
            shelf.items.iter_mut().for_each(&update);
        }
    }

    fn volume(&mut self, value: Option<f64>, cx: &mut Context<Self>) {
        if self.player.as_ref().and_then(|p| p.pcm_volume).is_none() {
            return;
        }
        self.volume = match value {
            Some(n) => n.clamp(0., 1.),
            None if self.volume == 0. => self.unmuted_volume,
            None => {
                self.unmuted_volume = self.volume;
                0.
            }
        };
        cx.notify();
    }

    fn control(&mut self, seek: Option<f64>, cx: &mut Context<Self>) {
        if let Some(p) = &self.player
            && let Err(e) = match seek {
                Some(n) => p.seek(n),
                None => p.toggle(),
            }
        {
            self.stop();
            self.message = format!("{e:#}");
        }
        cx.notify();
    }

    fn change_shelf(&mut self, next: usize, cx: &mut Context<Self>) {
        self.state.shelf = Some(next);
        self.state.selection = 0;
        self.state.reveal_row(next + 1);
        self.shelf_scroll[next].set_offset(Point::default());
        cx.notify();
    }

    fn key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.state.wheel_motion = None;
        if self.capture_shortcut(&event.keystroke, cx) {
            return;
        }
        let key = event.keystroke.key.as_str();
        if self.player.as_ref().is_some_and(|p| p.panel_open) {
            match key {
                "up" | "left" | "down" | "right" | "tab" => {
                    let forward = matches!(key, "down" | "right")
                        || (key == "tab" && !event.keystroke.modifiers.shift);
                    self.move_menu_focus(forward, window, cx);
                }
                "escape" | "backspace" => self.back_player_menu(window, cx),
                "enter" | "space" => return, // The focused GPUI button handles key-up.
                _ => return,
            }
            cx.stop_propagation();
            return;
        }
        if self.controls.remote_scrub {
            match key {
                "left" | "right" => {
                    if let Some(p) = &self.player
                        && let Some(f) = &mut self.scrub
                    {
                        let direction = if key == "left" { -1. } else { 1. };
                        *f = (*f + direction * (10. / p.status.duration) as f32).clamp(0., 1.);
                    }
                }
                "enter" | "space" => {
                    if let Some(f) = self.scrub
                        && let Some(p) = &self.player
                    {
                        self.control(
                            Some(f64::from(f) * p.status.duration - p.status.position),
                            cx,
                        );
                    }
                    self.end_preview();
                }
                "escape" | "backspace" => self.end_preview(),
                _ => return,
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if self.credits_prompt().is_some() {
            match key {
                "right" | "down" => self.menu_focus.credits.focus(window),
                "left" | "up" => self.menu_focus.next.focus(window),
                "escape" | "backspace" => self.watch_credits(window, cx),
                "enter" if self.focus.is_focused(window) => self.play_from_credits(window, cx),
                _ => return self.regular_key(event, window, cx),
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        self.regular_key(event, window, cx);
    }

    fn regular_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        if self
            .browse_menu
            .as_ref()
            .is_some_and(|(_, menu)| menu.read(cx).focus_handle(cx).contains_focused(window, cx))
        {
            return;
        }
        if key == "tab" {
            if event.keystroke.modifiers.shift {
                window.focus_prev();
            } else {
                window.focus_next();
            }
            cx.stop_propagation();
            return;
        }
        if self.navigation_focus.contains_focused(window, cx) {
            match key {
                "left" => window.focus_prev(),
                "right" => window.focus_next(),
                "down" | "escape" => self.focus.focus(window),
                _ => return,
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        // Focused GPUI buttons dispatch their own keyboard click on key-up.
        if matches!(key, "enter" | "space") && !self.focus.is_focused(window) {
            return;
        }
        if self.player.is_some() && !self.controls.mini {
            let shortcut = event.keystroke.unparse();
            let action = customization::SHORTCUTS
                .iter()
                .find(|(id, _, key)| self.prefs.binding(id, key) == shortcut)
                .map(|(id, _, _)| *id)
                .unwrap_or(match key {
                    "enter" => "pause",
                    "f11" => "fullscreen",
                    "backspace" | "escape" => "exit",
                    _ => "",
                });
            match action {
                "exit" if self.player.as_ref().is_some_and(|p| p.panel_open) => {
                    self.back_player_menu(window, cx);
                }
                "exit" if key == "escape" && window.is_fullscreen() => window.toggle_fullscreen(),
                "exit" => self.stop(),
                "pause" => self.control(None, cx),
                "mute" => self.volume(None, cx),
                "louder" => self.volume(Some(self.volume + 0.05), cx),
                "quieter" => self.volume(Some(self.volume - 0.05), cx),
                "back" => self.control(Some(-10.), cx),
                "forward" => self.control(Some(10.), cx),
                "fullscreen" => window.toggle_fullscreen(),
                "preview" => self.start_preview(window, cx),
                "options" => self.toggle_player_menu(PlayerMenu::Settings, window, cx),
                "zoom-in" | "zoom-out" => {
                    if let Some(p) = &self.player
                        && p.mpv()
                        && !p.audio_only
                    {
                        let value = (self.controls.zoom
                            + if action == "zoom-in" { 0.25 } else { -0.25 })
                        .clamp(0.5, 2.);
                        match p.zoom(value) {
                            Ok(()) => self.controls.zoom = value,
                            Err(e) => self.controls.warning = e.to_string(),
                        }
                    }
                }
                _ => return,
            }
        } else if self.code.is_some() && key == "escape" {
            self.cancel_pairing(window, cx);
        } else if self.server.is_some() {
            let typing = [
                &self.search,
                &self.explore.address,
                &self.explore.key,
                &self.explore.search,
                &self.services.mdb_input,
                &self.services.discord_input,
                &self.anime.search,
                &self.anime.pin,
                &self.anime.app_id,
            ]
            .into_iter()
            .any(|i| i.read(cx).focus_handle(cx).is_focused(window));
            let editing = self.state.editor.as_ref().is_some_and(|e| {
                [&e.title, &e.overview]
                    .iter()
                    .any(|i| i.read(cx).focus_handle(cx).is_focused(window))
            }) || self
                .state
                .subtitle_search
                .as_ref()
                .is_some_and(|s| s.language.read(cx).focus_handle(cx).is_focused(window));
            if (typing || editing) && key != "escape" {
                return;
            }
            let count = self.state.page.as_ref().map_or(0, |p| p.items.len());
            let selected = self.state.selection;
            if self.detail_action_key(key, window, cx) {
                cx.stop_propagation();
                cx.notify();
                return;
            }
            if matches!(key, "up" | "down" | "left" | "right") {
                self.focus.focus(window);
            }
            self.keyboard = true;
            if self.series_key(key, window, cx) {
                cx.stop_propagation();
                cx.notify();
                return;
            }
            if matches!(self.state.screen, Screen::Library(..)) && self.catalog_key(key, window, cx)
            {
                cx.stop_propagation();
                cx.notify();
                return;
            }
            match key {
                "escape" | "backspace" => self.back(window, cx),
                "f11" => window.toggle_fullscreen(),
                "/" => self.navigate(
                    Screen::Search(self.search.read(cx).value().trim().to_owned()),
                    window,
                    cx,
                ),
                _ if matches!(self.state.screen, Screen::Discover) => {
                    self.shelf_light.hovered = None;
                    match (key, self.state.shelf) {
                        ("down" | "pagedown", None) if !self.shelves.is_empty() => {
                            self.change_shelf(0, cx)
                        }
                        ("down" | "pagedown", Some(r)) if r + 1 < self.shelves.len() => {
                            self.change_shelf(r + 1, cx)
                        }
                        ("up" | "pageup", Some(0)) => {
                            self.state.shelf = None;
                            self.state.scroll.scroll_to_item(0);
                        }
                        ("up" | "pageup", Some(r)) => self.change_shelf(r - 1, cx),
                        ("right" | "left", Some(r)) => {
                            let max = self.shelves[r].items.len() - 1;
                            self.state.selection = if key == "left" {
                                selected.saturating_sub(1)
                            } else {
                                (selected + 1).min(max)
                            };
                            let shelf = &self.shelf_scroll[r];
                            if let Some(card) = shelf.bounds_for_item(self.state.selection) {
                                let viewport = shelf.bounds();
                                let x = shelf
                                    .offset()
                                    .x
                                    .max(viewport.left() - card.left())
                                    .min(viewport.right() - card.right());
                                shelf.set_offset(point(x, px(0.)));
                            }
                        }
                        ("right" | "left", None) if !self.featured.is_empty() => {
                            let n = self.featured.len();
                            self.set_hero(
                                (self.hero + if key == "right" { 1 } else { n - 1 }) % n,
                                cx,
                            );
                        }
                        ("enter", row) => {
                            if let Some(shelf) = row.and_then(|r| self.shelves.get(r))
                                && let Some(item) = shelf.items.get(selected).cloned()
                            {
                                self.activate_home_item(shelf.kind, item, window, cx);
                            } else if let Some(item) = self.featured.get(self.hero).cloned() {
                                if item.playable() {
                                    self.play_item(item, window, cx);
                                } else {
                                    self.open_item(item, window, cx);
                                }
                            }
                        }
                        _ => return,
                    }
                }
                "pagedown" => self.turn_page(true, window, cx),
                "pageup" => self.turn_page(false, window, cx),
                "enter" => {
                    if let Screen::Detail(item, _) = &self.state.screen
                        && item.playable()
                    {
                        self.play_item(*item.clone(), window, cx);
                    } else if let Some(item) = self
                        .state
                        .page
                        .as_ref()
                        .and_then(|p| p.items.get(selected))
                        .cloned()
                    {
                        if matches!(&self.state.screen, Screen::Detail(show, _) if show.kind == "Series")
                            && item.playable()
                        {
                            self.play_item(item, window, cx);
                        } else {
                            self.open_item(item, window, cx);
                        }
                    }
                }
                "up" | "down" | "left" | "right" if count > 0 => {
                    let columns = if matches!(self.state.screen, Screen::Detail(..)) {
                        1
                    } else {
                        screens::CatalogGrid::for_window(window).columns
                    };
                    self.state.selection = match key {
                        "left" => selected.saturating_sub(1),
                        "right" => (selected + 1).min(count - 1),
                        "up" => selected.saturating_sub(columns),
                        _ => (selected + columns).min(count - 1),
                    };
                    let shift = usize::from(matches!(self.state.screen, Screen::Detail(..)));
                    self.state
                        .reveal_row(self.state.selection / columns + shift);
                }
                _ => return,
            }
        } else {
            return;
        }
        cx.stop_propagation();
        cx.notify();
    }
}
