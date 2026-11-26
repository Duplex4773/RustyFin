use super::*;
use crate::jellyfin::federation::{HomeFeed, home_shelves};
use crate::preferences::{DismissedTitle, HomePreferences, ResumeKey};

fn resume_title(item: &Item) -> &str {
    if item.kind == "Episode" {
        item.series_id.as_deref().unwrap_or(&item.id)
    } else {
        &item.id
    }
}

impl DismissedTitle {
    fn from_item(item: &Item) -> Self {
        let mut copies = Vec::new();
        for copy in std::iter::once(item).chain(&item.copies) {
            if let Some(account) = &copy.origin {
                let key = ResumeKey {
                    account: account.clone(),
                    title: resume_title(copy).to_owned(),
                };
                if !copies.contains(&key) {
                    copies.push(key);
                }
            }
        }
        Self {
            name: item.series_name.as_ref().unwrap_or(&item.name).clone(),
            copies,
        }
    }

    fn contains(&self, item: &Item) -> bool {
        self.copies.iter().any(|key| {
            item.origin.as_ref() == Some(&key.account) && resume_title(item) == key.title
        })
    }
}

#[derive(Default)]
enum FeedStatus {
    #[default]
    Idle,
    Ready,
    Failed(String),
}

#[derive(Default)]
struct Feed {
    status: FeedStatus,
    pending: Option<u64>,
    items: Vec<Item>,
}

struct Source {
    origin: String,
    host: String,
    feeds: [Feed; 4],
}

#[derive(Default)]
pub(super) struct Home {
    sources: Vec<Source>,
    next_request: u64,
}

impl Home {
    pub(super) fn clear(&mut self) {
        // Keep the request sequence across account changes so late responses cannot
        // write into a new session, even when the same account signs in again.
        self.sources.clear();
    }

    fn sync(&mut self, servers: &[Server]) {
        let mut previous = std::mem::take(&mut self.sources);
        for server in servers {
            let Some(account) = server.identity() else {
                continue;
            };
            let origin = account.key();
            let source = previous
                .iter()
                .position(|s| s.origin == origin)
                .map(|i| previous.swap_remove(i))
                .unwrap_or_else(|| Source {
                    origin,
                    host: account.server.trim_end_matches('/').to_string(),
                    feeds: Default::default(),
                });
            self.sources.push(source);
        }
    }

    fn feed_mut(&mut self, origin: &str, feed: HomeFeed) -> Option<&mut Feed> {
        Some(&mut self.sources.iter_mut().find(|s| s.origin == origin)?.feeds[feed as usize])
    }

    fn begin(&mut self, origin: &str, feed: HomeFeed) -> Option<u64> {
        self.next_request += 1;
        let request = self.next_request;
        self.feed_mut(origin, feed)?.pending = Some(request);
        Some(request)
    }

    fn finish(
        &mut self,
        origin: &str,
        feed: HomeFeed,
        request: u64,
        result: Result<Vec<Item>>,
    ) -> bool {
        let Some(slot) = self.feed_mut(origin, feed) else {
            return false;
        };
        if slot.pending != Some(request) {
            return false;
        }
        slot.pending = None;
        match result {
            Ok(mut items) => {
                for item in &mut items {
                    item.origin = Some(origin.to_string());
                }
                slot.items = items;
                slot.status = FeedStatus::Ready;
            }
            Err(e) => slot.status = FeedStatus::Failed(e.to_string()),
        }
        true
    }

    fn shelves(&self, prefs: &HomePreferences) -> Vec<Shelf> {
        let mut shelves = home_shelves(
            self.sources
                .iter()
                .map(|s| s.feeds.each_ref().map(|f| f.items.as_slice())),
            self.sources.len() > 1,
            |item| !prefs.dismissed.iter().any(|title| title.contains(item)),
        );
        let order = prefs.layout.shelf_order();
        shelves.sort_by_key(|shelf| order.iter().position(|&kind| kind == shelf.kind));
        shelves
    }

    pub(super) fn loading(&self) -> bool {
        self.sources
            .iter()
            .flat_map(|s| &s.feeds)
            .any(|f| f.pending.is_some())
    }

    pub(super) fn continue_ready(&self, origin: Option<&str>) -> bool {
        self.sources
            .iter()
            .find(|s| Some(s.origin.as_str()) == origin)
            .is_some_and(|s| {
                [HomeFeed::Resume, HomeFeed::NextUp].iter().all(|feed| {
                    let slot = &s.feeds[*feed as usize];
                    slot.pending.is_none() && matches!(slot.status, FeedStatus::Ready)
                })
            })
    }

    pub(super) fn unavailable(&self) -> bool {
        !self.sources.is_empty()
            && self
                .sources
                .iter()
                .flat_map(|s| &s.feeds)
                .all(|f| matches!(f.status, FeedStatus::Failed(_)))
    }

    pub(super) fn failures(&self) -> impl Iterator<Item = (&str, &str, Vec<(HomeFeed, &str)>)> {
        self.sources.iter().filter_map(|source| {
            let failures: Vec<_> = HomeFeed::ALL
                .into_iter()
                .filter_map(|feed| {
                    let slot = &source.feeds[feed as usize];
                    if slot.pending.is_none()
                        && let FeedStatus::Failed(error) = &slot.status
                    {
                        Some((feed, error.as_str()))
                    } else {
                        None
                    }
                })
                .collect();
            (!failures.is_empty()).then_some((
                source.origin.as_str(),
                source.host.as_str(),
                failures,
            ))
        })
    }

    pub(super) fn update_items(&mut self, update: impl Fn(&mut Item)) {
        for item in self
            .sources
            .iter_mut()
            .flat_map(|s| &mut s.feeds)
            .flat_map(|f| &mut f.items)
        {
            update(item);
        }
    }
}

impl Library {
    pub(super) fn save_home(&mut self, cx: &mut Context<Self>) {
        self.publish_home();
        self.save_preferences(cx);
    }

    pub(super) fn dismiss_continue(&mut self, item: &Item, w: &mut Window, cx: &mut Context<Self>) {
        let dismissal = DismissedTitle::from_item(item);
        if dismissal.copies.is_empty() || self.prefs.home.dismissed.contains(&dismissal) {
            return;
        }
        self.prefs.home.dismissed.push(dismissal.clone());
        self.home_undo = Some(dismissal);
        self.shelf_light.hovered = None;
        self.save_home(cx);
        if self.keyboard {
            let view = cx.weak_entity();
            w.defer(cx, move |w, cx| {
                let _ = view.update(cx, |s, _| {
                    if s.home_undo.is_some() && matches!(s.state.screen, Screen::Discover) {
                        s.home_undo_focus.focus(w);
                    }
                });
            });
        }
    }

    pub(super) fn restore_continue(&mut self, title: &DismissedTitle, cx: &mut Context<Self>) {
        self.prefs
            .home
            .dismissed
            .retain(|dismissed| dismissed != title);
        if self.home_undo.as_ref() == Some(title) {
            self.home_undo = None;
        }
        self.save_home(cx);
    }

    pub(super) fn undo_continue(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(title) = self.home_undo.take() {
            self.focus.focus(w);
            self.restore_continue(&title, cx);
        }
    }

    pub(super) fn load_home(&mut self, w: &Window, cx: &mut Context<Self>) {
        if !self.home.sources.is_empty() && !self.home_dirty {
            cx.notify();
            return;
        }
        self.home_dirty = false;
        let servers = self.browse_servers();
        self.home.sync(&servers);
        self.publish_home();
        for server in servers {
            for feed in HomeFeed::ALL {
                self.load_home_feed(&server, feed, w, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn load_home_feed(
        &mut self,
        server: &Server,
        feed: HomeFeed,
        w: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(origin) = server.identity().map(|a| a.key()) else {
            return;
        };
        let Some(request) = self.home.begin(&origin, feed) else {
            return;
        };
        let server = server.clone();
        let worker = cx
            .background_executor()
            .spawn(async move { server.home_items(feed) });
        cx.spawn_in(w, async move |view, cx| {
            let result = worker.await;
            let _ = view.update(cx, |s, cx| {
                if s.home.finish(&origin, feed, request, result) {
                    s.publish_home();
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn publish_home(&mut self) {
        let mut shelves = self.home.shelves(&self.prefs.home);
        let featured = screens::featured_titles(&shelves);
        shelves.retain(|shelf| !self.prefs.home.layout.hidden.contains(&shelf.kind));
        let old_hero = self.featured.get(self.hero);
        self.hero = old_hero
            .and_then(|old| {
                featured
                    .iter()
                    .position(|i| i.id == old.id && i.origin == old.origin)
            })
            .unwrap_or(0);
        if old_hero.map(|i| (&i.id, &i.origin))
            != featured.get(self.hero).map(|i| (&i.id, &i.origin))
        {
            self.last_slide = Instant::now();
        }
        self.featured = featured;
        let active = self
            .server
            .as_ref()
            .and_then(Server::identity)
            .map(|a| a.key());
        for state in self
            .history
            .iter_mut()
            .chain(std::iter::once(&mut self.state))
        {
            if !matches!(state.screen, Screen::Discover)
                || (state.profile.is_some() && state.profile != active)
            {
                continue;
            }
            let Some(row) = state.shelf else {
                continue;
            };
            let Some(old) = self.shelves.get(row) else {
                state.shelf = None;
                state.selection = 0;
                continue;
            };
            state.shelf = shelves.iter().position(|s| s.kind == old.kind);
            state.selection = state.shelf.map_or(0, |row| {
                let items = &shelves[row].items;
                old.items
                    .get(state.selection)
                    .and_then(|old| {
                        items
                            .iter()
                            .position(|i| i.id == old.id && i.origin == old.origin)
                    })
                    .unwrap_or_else(|| state.selection.min(items.len().saturating_sub(1)))
            });
        }
        self.shelf_scroll = shelves
            .iter()
            .map(|shelf| {
                self.shelves
                    .iter()
                    .position(|s| s.kind == shelf.kind)
                    .and_then(|i| self.shelf_scroll.get(i))
                    .cloned()
                    .unwrap_or_default()
            })
            .collect();
        self.shelves = shelves;
    }
}

#[cfg(test)]
mod tests {
    use super::{DismissedTitle, Home, HomeFeed, HomePreferences, Item, Server, ShelfKind};
    use crate::preferences::Preferences;

    fn server(name: &str) -> Server {
        Server::restore(crate::jellyfin::Account {
            server: format!("http://{name}.example:8096"),
            device: "fixture".into(),
            user: name.into(),
            username: None,
            token: "test".into(),
        })
        .unwrap()
    }

    fn item(id: &str) -> Item {
        serde_json::from_value(serde_json::json!({"Id":id, "Name":id, "Type":"Movie", "UserData":{"PlaybackPositionTicks":200}})).unwrap()
    }

    #[test]
    fn dismissing_a_series_survives_refresh_and_keeps_resume_progress() {
        let mut home = Home::default();
        home.sync(&[server("primary"), server("secondary")]);
        let primary = home.sources[0].origin.clone();
        let secondary = home.sources[1].origin.clone();
        let episode = |id: &str, series: &str, position: u64| {
            serde_json::from_value(serde_json::json!({
                "Id": id,
                "Name": "Episode",
                "Type": "Episode",
                "SeriesId": series,
                "SeriesName": "Test show",
                "ProviderIds": { "Imdb": "tt1234" },
                "UserData": { "PlaybackPositionTicks": position }
            }))
            .unwrap()
        };
        let resume = home.begin(&primary, HomeFeed::Resume).unwrap();
        home.finish(
            &primary,
            HomeFeed::Resume,
            resume,
            Ok(vec![episode("first", "series", 200)]),
        );
        let resume = home.begin(&secondary, HomeFeed::Resume).unwrap();
        home.finish(
            &secondary,
            HomeFeed::Resume,
            resume,
            Ok(vec![episode("copy", "linked-series", 100)]),
        );
        let mut prefs = HomePreferences::default();
        let shelf = home.shelves(&prefs);
        let dismissal = DismissedTitle::from_item(&shelf[0].items[0]);
        assert_eq!(dismissal.copies.len(), 2);
        prefs.dismissed.push(dismissal);
        assert!(home.shelves(&prefs).is_empty());

        let next = home.begin(&primary, HomeFeed::NextUp).unwrap();
        home.finish(
            &primary,
            HomeFeed::NextUp,
            next,
            Ok(vec![episode("next", "series", 0)]),
        );
        assert!(home.shelves(&prefs).is_empty());
        assert_eq!(
            home.sources[0].feeds[HomeFeed::Resume as usize].items[0].position(),
            200
        );
        assert_eq!(
            home.shelves(&HomePreferences::default())[0].items[0].position(),
            200
        );
        prefs.dismissed.clear();
        assert_eq!(home.shelves(&prefs)[0].items[0].position(), 200);
    }

    #[test]
    fn older_profiles_get_standard_home_and_new_layout_round_trips() {
        let mut prefs: Preferences = serde_json::from_str(r#"{"rotate":false}"#).unwrap();
        assert_eq!(prefs.home.layout.shelf_order(), ShelfKind::ALL);
        assert!(!prefs.rotate);
        prefs.home.layout.move_shelf(ShelfKind::RecentMovies, -1);
        prefs.home.layout.hidden.push(ShelfKind::RecentTv);
        prefs.home.layout.pin_favourites = true;
        prefs
            .home
            .layout
            .pinned_libraries
            .push("library".to_owned());
        let loaded: Preferences =
            serde_json::from_slice(&serde_json::to_vec(&prefs).unwrap()).unwrap();
        assert_eq!(
            loaded.home.layout.shelf_order(),
            [
                ShelfKind::Continue,
                ShelfKind::RecentMovies,
                ShelfKind::RecentTv
            ]
        );
        assert_eq!(loaded.home.layout.hidden, [ShelfKind::RecentTv]);
        assert!(loaded.home.layout.pin_favourites);
        assert_eq!(loaded.home.layout.pinned_libraries, ["library"]);
    }

    #[test]
    fn home_keeps_ready_feeds_and_ignores_superseded_responses() {
        let mut home = Home::default();
        let prefs = HomePreferences::default();
        home.sync(&[server("primary"), server("secondary")]);
        let primary = home.sources[0].origin.clone();
        let secondary = home.sources[1].origin.clone();
        let slow = home.begin(&secondary, HomeFeed::RecentMovies).unwrap();
        let resume = home.begin(&primary, HomeFeed::Resume).unwrap();
        home.finish(
            &primary,
            HomeFeed::Resume,
            resume,
            Ok(vec![item("unfinished")]),
        );
        let movies = home.begin(&primary, HomeFeed::RecentMovies).unwrap();
        home.finish(
            &primary,
            HomeFeed::RecentMovies,
            movies,
            Ok(vec![item("film")]),
        );
        assert_eq!(home.shelves(&prefs)[0].items[0].id, "unfinished");
        assert_eq!(
            home.shelves(&prefs)[1].items[0].origin.as_deref(),
            Some(primary.as_str())
        );
        assert!(home.loading());

        let old = home.begin(&primary, HomeFeed::RecentMovies).unwrap();
        let new = home.begin(&primary, HomeFeed::RecentMovies).unwrap();
        home.finish(
            &primary,
            HomeFeed::RecentMovies,
            new,
            Err(anyhow::anyhow!("unavailable")),
        );
        assert!(!home.finish(&primary, HomeFeed::RecentMovies, old, Ok(vec![])));
        assert_eq!(home.shelves(&prefs)[1].items[0].id, "film");
        assert_eq!(home.failures().count(), 1);
        assert!(!home.unavailable());

        let retry = home.begin(&primary, HomeFeed::RecentMovies).unwrap();
        home.finish(&primary, HomeFeed::RecentMovies, retry, Ok(vec![]));
        assert_eq!(home.shelves(&prefs).len(), 1);
        assert_eq!(home.failures().count(), 0);
        home.sync(&[server("primary")]);
        assert!(!home.finish(
            &secondary,
            HomeFeed::RecentMovies,
            slow,
            Ok(vec![item("private")])
        ));
    }

    #[test]
    fn home_distinguishes_an_empty_success_from_all_requests_failing() {
        let mut home = Home::default();
        let prefs = HomePreferences::default();
        let server = server("primary");
        home.sync(std::slice::from_ref(&server));
        let key = home.sources[0].origin.clone();
        for feed in HomeFeed::ALL {
            let request = home.begin(&key, feed).unwrap();
            home.finish(&key, feed, request, Err(anyhow::anyhow!("unreachable")));
        }
        assert!(home.unavailable());
        assert!(!home.loading());
        let retry = home.begin(&key, HomeFeed::RecentTv).unwrap();
        // Offline downloads stay available while an explicit retry is pending.
        assert!(home.unavailable());
        assert!(home.loading());
        home.finish(&key, HomeFeed::RecentTv, retry, Ok(vec![]));
        assert!(!home.unavailable());
        assert!(home.shelves(&prefs).is_empty());

        let old = home.begin(&key, HomeFeed::Resume).unwrap();
        home.clear();
        home.sync(&[server]);
        home.begin(&key, HomeFeed::Resume);
        assert!(!home.finish(
            &key,
            HomeFeed::Resume,
            old,
            Ok(vec![item("previous-session")])
        ));
        assert!(home.shelves(&prefs).is_empty());
    }
}
