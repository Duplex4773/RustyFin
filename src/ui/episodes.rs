use super::*;
use crate::jellyfin::offline::Download;
use std::{collections::HashSet, path::Path};

pub(super) const EPISODE_ROW_HEIGHT: f32 = 112.;

#[derive(Clone)]
pub(super) struct EpisodeSeason {
    id: Option<String>,
    number: Option<u32>,
    pub name: String,
}

impl EpisodeSeason {
    fn named(number: Option<u32>, id: Option<String>) -> Self {
        let name = match number {
            Some(0) => "Specials".into(),
            Some(n) => format!("Season {n}"),
            None => "All episodes".into(),
        };
        Self { id, number, name }
    }
}

#[derive(Default)]
pub(super) struct EpisodeDrawer {
    pub seasons: Vec<EpisodeSeason>,
    pub selected: usize,
    pub result: Option<Result<Vec<Item>, String>>,
    job: Option<Task<()>>,
}

impl EpisodeDrawer {
    pub fn items(&self) -> &[Item] {
        match &self.result {
            Some(Ok(items)) => items,
            _ => &[],
        }
    }
}

fn current_season(seasons: &[EpisodeSeason], playing: &Item) -> Option<usize> {
    playing
        .season_id
        .as_ref()
        .and_then(|id| seasons.iter().position(|s| s.id.as_ref() == Some(id)))
        .or_else(|| {
            seasons
                .iter()
                .position(|s| s.number == playing.parent_index_number)
        })
}

fn server_episodes(
    server: &Server,
    series: &str,
    playing: &Item,
    mut seasons: Vec<EpisodeSeason>,
    selected: usize,
) -> Result<(Vec<EpisodeSeason>, usize, Vec<Item>)> {
    let selected = if seasons.is_empty() {
        seasons = server
            .seasons(series)?
            .into_iter()
            .map(|s| EpisodeSeason {
                id: Some(s.id),
                number: s.index_number,
                name: s.name,
            })
            .collect();
        current_season(&seasons, playing).unwrap_or_else(|| {
            seasons.push(EpisodeSeason::named(
                playing.parent_index_number,
                playing.season_id.clone(),
            ));
            seasons.len() - 1
        })
    } else {
        selected
    };
    let items = server.season_episodes(series, seasons[selected].id.as_deref())?;
    Ok((seasons, selected, items))
}

fn downloaded_episodes(
    profile: &Path,
    records: Vec<Download>,
    playing: &Item,
    season: Option<&EpisodeSeason>,
) -> (Vec<EpisodeSeason>, usize, Vec<Item>) {
    let mut seen = HashSet::new();
    let mut items: Vec<_> = records
        .into_iter()
        .filter(|d| d.ready(profile) && seen.insert(d.item.id.clone()))
        .map(|d| d.item_with_progress(profile).unwrap_or(d.item))
        .collect();
    if !items.iter().any(|i| i.id == playing.id) {
        items.push(playing.clone());
    }
    items.sort_by_key(|i| (i.parent_index_number, i.index_number));
    let mut numbers: Vec<_> = items.iter().map(|i| i.parent_index_number).collect();
    numbers.dedup();
    let number = season.map_or(playing.parent_index_number, |s| s.number);
    let selected = numbers.iter().position(|&n| n == number).unwrap_or(0);
    let number = numbers[selected];
    items.retain(|i| i.parent_index_number == number);
    let seasons = numbers
        .into_iter()
        .map(|n| EpisodeSeason::named(n, None))
        .collect();
    (seasons, selected, items)
}

impl Library {
    pub(super) fn has_episode_drawer(&self) -> bool {
        self.playing
            .as_ref()
            .is_some_and(|i| i.kind == "Episode" && i.series_id.is_some())
    }

    pub(super) fn load_episode_drawer(&mut self, w: &Window, cx: &mut Context<Self>) {
        let (Some(playing), Some(server)) = (self.playing.clone(), self.server.clone()) else {
            return;
        };
        let Some(series) = playing.series_id.clone() else {
            return;
        };
        let drawer = &mut self.controls.episodes;
        let seasons = drawer.seasons.clone();
        let selected = drawer.selected;
        let offline = self
            .downloads
            .playing
            .as_ref()
            .and(self.downloads.profile.clone());
        let records = offline.as_ref().map(|_| {
            self.downloads
                .records
                .iter()
                .filter(|d| d.item.kind == "Episode" && d.item.series_id.as_ref() == Some(&series))
                .cloned()
                .collect()
        });
        drawer.result = None;
        self.menu_focus.scroll.set_offset(Point::default());
        let work = cx.background_executor().spawn(async move {
            match offline.zip(records) {
                Some((profile, records)) => Ok(downloaded_episodes(
                    &profile,
                    records,
                    &playing,
                    seasons.get(selected),
                )),
                None => server_episodes(&server, &series, &playing, seasons, selected),
            }
        });
        drawer.job = Some(cx.spawn_in(w, async move |view, cx| {
            let result = work.await;
            let _ = view.update(cx, |s, cx| {
                let drawer = &mut s.controls.episodes;
                drawer.result = Some(match result {
                    Ok((seasons, selected, items)) => {
                        drawer.seasons = seasons;
                        drawer.selected = selected;
                        Ok(items)
                    }
                    Err(e) => Err(format!("Could not load episodes. {e:#}")),
                });
                if s.controls.menu == PlayerMenu::Episodes {
                    s.menu_focus.entering.set(true);
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn select_episode_season(
        &mut self,
        index: usize,
        w: &Window,
        cx: &mut Context<Self>,
    ) {
        let drawer = &mut self.controls.episodes;
        if index >= drawer.seasons.len() {
            return;
        }
        let changed = drawer.selected != index;
        drawer.selected = index;
        self.enter_player_menu(PlayerMenu::Episodes, cx);
        if changed {
            self.load_episode_drawer(w, cx);
        }
    }
}
