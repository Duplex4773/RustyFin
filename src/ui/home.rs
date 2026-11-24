use super::*;
use crate::jellyfin::federation::{HomeFeed, home_shelves};

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

    fn shelves(&self) -> Vec<Shelf> {
        home_shelves(
            self.sources
                .iter()
                .map(|s| s.feeds.each_ref().map(|f| f.items.as_slice())),
            self.sources.len() > 1,
        )
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
        let shelves = self.home.shelves();
        let featured = screens::featured_titles(&shelves);
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
    use super::{Home, HomeFeed, Item, Server};

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
    fn home_keeps_ready_feeds_and_ignores_superseded_responses() {
        let mut home = Home::default();
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
        assert_eq!(home.shelves()[0].items[0].id, "unfinished");
        assert_eq!(
            home.shelves()[1].items[0].origin.as_deref(),
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
        assert_eq!(home.shelves()[1].items[0].id, "film");
        assert_eq!(home.failures().count(), 1);
        assert!(!home.unavailable());

        let retry = home.begin(&primary, HomeFeed::RecentMovies).unwrap();
        home.finish(&primary, HomeFeed::RecentMovies, retry, Ok(vec![]));
        assert_eq!(home.shelves().len(), 1);
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
        assert!(home.shelves().is_empty());

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
        assert!(home.shelves().is_empty());
    }
}
