use super::*;
use crate::anilist::{AniList, Feed, Media, Update};

pub(super) struct Anime {
    pub search: Entity<InputState>,
    pub app_id: Entity<InputState>,
    pub pin: Entity<InputState>,
    pub client: Option<AniList>,
    pub viewer: Option<u64>,
    pub feed: Feed,
    pub term: String,
    pub page: u32,
    pub rows: Vec<Media>,
    pub more: bool,
    pub selected: Option<Media>,
    pub matching: Vec<Item>,
}
impl Anime {
    pub fn new(w: &mut Window, cx: &mut Context<Library>) -> Self {
        let search = cx.new(|cx| InputState::new(w, cx).placeholder("Search AniList"));
        cx.subscribe_in(&search, w, |s, _, event, w, cx| {
            if matches!(s.state.screen, Screen::Anime)
                && matches!(event, InputEvent::PressEnter { .. })
            {
                s.anime.term = s.anime.search.read(cx).value().trim().to_owned();
                s.anime.page = 1;
                s.load_anime(w, cx);
            }
        })
        .detach();
        Self {
            search,
            app_id: cx.new(|cx| InputState::new(w, cx).placeholder("AniList application ID")),
            pin: cx.new(|cx| {
                InputState::new(w, cx)
                    .masked(true)
                    .placeholder("AniList access token")
            }),
            client: None,
            viewer: None,
            feed: Feed::Trending,
            term: String::new(),
            page: 1,
            rows: vec![],
            more: false,
            selected: None,
            matching: vec![],
        }
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
struct Account {
    token: String,
    viewer: u64,
}
impl Library {
    fn anime_key(&self) -> Result<String> {
        Ok(format!(
            "RustFin/AniList/{}",
            self.server
                .as_ref()
                .and_then(Server::identity)
                .context("Sign in first.")?
                .key()
        ))
    }
    pub(super) fn restore_anime(&mut self) {
        let saved = self
            .anime_key()
            .ok()
            .and_then(|key| credentials::read_secret::<Account>(&key).ok().flatten());
        self.anime.viewer = saved.as_ref().map(|a| a.viewer);
        self.anime.client = AniList::new(saved.map(|a| a.token)).ok();
    }
    pub(super) fn authorize_anime(&mut self, cx: &mut Context<Self>) {
        let id = self.anime.app_id.read(cx).value().trim().parse::<u64>();
        match id {
            Ok(id) => cx.open_url(&format!(
                "https://anilist.co/api/v2/oauth/authorize?client_id={id}&response_type=token"
            )),
            Err(_) => {
                self.message = "Enter the numeric ID of your AniList PIN application.".into();
                cx.notify();
            }
        }
    }
    pub(super) fn pair_anime(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Ok(key) = self.anime_key() else { return };
        if self.anime.viewer.is_some() {
            match credentials::remove_secret(&key) {
                Ok(()) => {
                    self.prefs.anime_sync = false;
                    self.save_preferences(cx);
                    self.anime = Anime::new(w, cx);
                    self.restore_anime();
                }
                Err(e) => self.message = e.to_string(),
            }
            cx.notify();
            return;
        }
        let token = self.anime.pin.read(cx).value().trim().to_owned();
        self.work(
            w,
            cx,
            move || {
                anyhow::ensure!(!token.is_empty(), "Paste the AniList access token.");
                let client = AniList::new(Some(token.clone()))?;
                let viewer = client.viewer()?;
                credentials::write_secret(&key, &Account { token, viewer })?;
                Ok((client, viewer))
            },
            |s, (client, viewer), w, cx| {
                s.anime.client = Some(client);
                s.anime.viewer = Some(viewer);
                s.anime.pin.update(cx, |i, cx| i.set_value("", w, cx));
                s.navigate(Screen::Anime, w, cx);
                Ok(())
            },
        );
    }
    pub(super) fn load_anime(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.anime.client.clone() else {
            return;
        };
        let feed = self.anime.feed;
        let term = self.anime.term.clone();
        let page = self.anime.page;
        let viewer = self.anime.viewer;
        self.anime.selected = None;
        self.work(
            w,
            cx,
            move || client.feed(&feed, &term, page, viewer),
            |s, (rows, more), _, _| {
                s.anime.rows = rows;
                s.anime.more = more;
                Ok(())
            },
        );
    }
    pub(super) fn open_anime(&mut self, entry: Media, w: &mut Window, cx: &mut Context<Self>) {
        let servers = self.browse_servers();
        let Some(client) = self.anime.client.clone() else {
            return;
        };
        self.work(
            w,
            cx,
            move || {
                let entry = client.media(entry.id)?;
                let mut matches = vec![];
                for server in servers {
                    let mut names = vec![entry.title.user_preferred.as_str()];
                    names.extend(entry.title.english.as_deref());
                    names.extend(entry.title.romaji.as_deref());
                    names.sort();
                    names.dedup();
                    let mut seen = std::collections::HashSet::new();
                    for name in names {
                        let Ok(page) =
                            server.catalog(&CatalogQuery::default(), Some(name), 0, PAGE_SIZE)
                        else {
                            continue;
                        };
                        for mut item in page.items {
                            if !matches!(item.kind.as_str(), "Movie" | "Series")
                                || !seen.insert(item.id.clone())
                            {
                                continue;
                            }
                            if item.provider_ids.iter().flatten().any(|(k, v)| {
                                let n = v.parse::<u64>().ok();
                                (k.eq_ignore_ascii_case("anilist") && n == Some(entry.id))
                                    || ((k.eq_ignore_ascii_case("mal")
                                        || k.eq_ignore_ascii_case("myanimelist"))
                                        && n.is_some()
                                        && n == entry.id_mal)
                            }) {
                                item.origin = server.identity().map(crate::jellyfin::Account::key);
                                matches.push(item);
                            }
                        }
                    }
                }
                Ok((entry, matches))
            },
            |s, (entry, matches), _, _| {
                s.anime.selected = Some(entry);
                s.anime.matching = matches;
                Ok(())
            },
        );
    }
    pub(super) fn anime_finished(&mut self, w: &Window, cx: &mut Context<Self>) {
        if !self.prefs.anime_sync || self.anime.viewer.is_none() {
            return;
        }
        let (Some(service), Some(item), Some(server)) = (
            self.anime.client.clone(),
            self.playing.clone(),
            self.server.clone(),
        ) else {
            return;
        };
        let account = server.identity().map(crate::jellyfin::Account::key);
        let worker = cx
            .background_executor()
            .spawn(async move { service.sync_finished(&server, &item) });
        cx.spawn_in(w, async move |view, cx| {
            if let Err(e) = worker.await {
                let _ = view.update(cx, |s, cx| {
                    if s.server
                        .as_ref()
                        .and_then(Server::identity)
                        .map(crate::jellyfin::Account::key)
                        == account
                    {
                        s.message = format!("AniList could not update watch progress. {e}");
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
    pub(super) fn update_anime(&mut self, update: Update, w: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.anime.viewer.is_none() {
            return;
        }
        let (Some(client), Some(entry)) = (self.anime.client.clone(), self.anime.selected.clone())
        else {
            return;
        };
        self.work(
            w,
            cx,
            move || client.update(&entry, update),
            |s, m, _, _| {
                if let Some(row) = s.anime.rows.iter_mut().find(|r| r.id == m.id) {
                    *row = m.clone();
                }
                s.anime.selected = Some(m);
                Ok(())
            },
        );
    }
}
