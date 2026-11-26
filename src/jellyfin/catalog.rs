use super::{Item, Page, Server, read};
use anyhow::{Context, Result};
use std::{collections::HashSet, io::Read};

pub const PAGE_SIZE: usize = 36;
pub const LIBRARY_PAGE_SIZE: usize = 90;
const FIELDS: &str = "Overview,Genres,MediaSources,ProviderIds";
const MOVING_PICTURES: &str = "Movie,Series,Episode,Video,Audio,MusicAlbum,MusicArtist,MusicVideo";

pub fn catalog_initial(sort_name: Option<&str>, name: &str) -> char {
    sort_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(name)
        .trim_start()
        .chars()
        .next()
        .filter(char::is_ascii_alphabetic)
        .map_or('#', |c| c.to_ascii_uppercase())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Art {
    pub origin: Option<String>,
    pub owner: String,
    pub kind: ArtKind,
    pub max_width: u16,
    tag: String,
}

impl super::Person {
    pub fn portrait(&self, origin: Option<&str>) -> Option<Art> {
        Some(Art {
            origin: origin.map(str::to_owned),
            owner: self.id.clone(),
            kind: ArtKind::Poster,
            max_width: 240,
            tag: self.primary_image_tag.clone()?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArtKind {
    Poster,
    LargePoster,
    Thumb,
    Backdrop,
    SmallBackdrop,
    Chapter(usize),
    Logo,
}

impl Item {
    pub fn art(&self, kind: ArtKind) -> Option<Art> {
        let max_width = match kind {
            ArtKind::Poster | ArtKind::Chapter(_) => 320,
            ArtKind::LargePoster => 640,
            ArtKind::Thumb | ArtKind::SmallBackdrop => 480,
            ArtKind::Logo => 960,
            ArtKind::Backdrop => 1440,
        };
        let local_tag = |name| self.image_tags.as_ref()?.get(name);
        let own = |tag: &String| {
            Some(Art {
                origin: self.origin.clone(),
                owner: self.id.clone(),
                kind,
                max_width,
                tag: tag.clone(),
            })
        };
        let inherited = |id: &Option<String>, tag: Option<&String>| {
            Some(Art {
                origin: self.origin.clone(),
                owner: id.clone()?,
                kind,
                max_width,
                tag: tag?.clone(),
            })
        };
        match kind {
            ArtKind::Chapter(i) => own(self.chapters.as_ref()?.get(i)?.image_tag.as_ref()?),
            ArtKind::Poster | ArtKind::LargePoster => {
                if self.kind == "Audio"
                    && let Some(art) =
                        inherited(&self.album_id, self.album_primary_image_tag.as_ref())
                {
                    return Some(art);
                }
                if self.kind == "Episode"
                    && let Some(art) =
                        inherited(&self.series_id, self.series_primary_image_tag.as_ref())
                {
                    return Some(art);
                }
                own(local_tag("Primary")?)
            }
            ArtKind::Thumb => own(local_tag("Primary")?),
            ArtKind::Backdrop | ArtKind::SmallBackdrop => self
                .backdrop_image_tags
                .as_ref()
                .and_then(|t| t.first())
                .and_then(own)
                .or_else(|| {
                    inherited(
                        &self.parent_backdrop_item_id,
                        self.parent_backdrop_image_tags.as_ref()?.first(),
                    )
                }),
            ArtKind::Logo => local_tag("Logo").and_then(own).or_else(|| {
                inherited(
                    &self.parent_logo_item_id,
                    self.parent_logo_image_tag.as_ref(),
                )
            }),
        }
    }

    pub fn landscape(&self) -> Option<Art> {
        if self.kind == "Episode" {
            self.art(ArtKind::Thumb)
                .or_else(|| self.art(ArtKind::SmallBackdrop))
        } else {
            self.art(ArtKind::SmallBackdrop)
                .or_else(|| self.art(ArtKind::Thumb))
        }
    }

    pub fn position(&self) -> u64 {
        self.user_data
            .as_ref()
            .and_then(|u| u.playback_position_ticks)
            .unwrap_or(0)
    }

    pub fn playable(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "Movie" | "Episode" | "Video" | "Trailer" | "MusicVideo" | "Audio"
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ShelfKind {
    Continue,
    RecentTv,
    RecentMovies,
}

impl ShelfKind {
    pub const ALL: [Self; 3] = [Self::Continue, Self::RecentTv, Self::RecentMovies];

    pub fn title(self) -> &'static str {
        match self {
            Self::Continue => "Continue Watching",
            Self::RecentTv => "Recently Added TV",
            Self::RecentMovies => "Recently Added Movies",
        }
    }

    pub fn art(self, item: &Item) -> Option<Art> {
        match self {
            Self::Continue => item.landscape(),
            Self::RecentTv | Self::RecentMovies => item.art(ArtKind::Poster),
        }
    }
}

pub struct Shelf {
    pub kind: ShelfKind,
    pub items: Vec<Item>,
}

// Jellyfin can return an in-progress episode in both Resume and NextUp.
// Keep the in-progress episode even when NextUp points to another episode of the same show.
pub(super) fn continue_watching(resume: Vec<Item>, next: Vec<Item>) -> Vec<Item> {
    let mut seen = HashSet::new();
    let mut unique = |i: &Item| seen.insert(i.series_id.as_ref().unwrap_or(&i.id).clone());
    let mut resume = resume
        .into_iter()
        .filter(|i| i.position() > 0 && unique(i))
        .peekable();
    let first_is_episode = resume.peek().is_none_or(|i| i.kind == "Episode");
    let (episodes, films): (Vec<_>, _) = resume.partition(|i| i.kind == "Episode");
    let (first, third) = if first_is_episode {
        (episodes, films)
    } else {
        (films, episodes)
    };
    let mut next = next.into_iter().filter(|i| {
        i.kind == "Episode"
            && i.user_data.as_ref().and_then(|u| u.played) != Some(true)
            && unique(i)
    });
    let (mut first, mut third) = (first.into_iter(), third.into_iter());
    let mut queue = Vec::new();
    loop {
        let size = queue.len();
        queue.extend(first.next());
        queue.extend(next.next());
        queue.extend(third.next());
        if queue.len() == size {
            return queue;
        }
    }
}

#[cfg(test)]
mod discover_tests {
    use super::*;
    use serde_json::json;

    fn entry(id: &str, series: Option<&str>, position: u64, played: bool) -> Item {
        serde_json::from_value(json!({
            "Id": id, "Name": id, "Type": if series.is_some() {"Episode"} else {"Movie"},
            "SeriesId": series, "UserData": {"PlaybackPositionTicks":position, "Played":played}
        }))
        .unwrap()
    }

    #[test]
    fn resumes_win_over_next_up_for_the_same_series() {
        let queue = continue_watching(
            vec![
                entry("paused", Some("series"), 98, false),
                entry("movie", None, 35, false),
            ],
            vec![
                entry("paused", Some("series"), 98, false),
                entry("later", Some("series"), 0, false),
                entry("seen", Some("other"), 0, true),
                entry("next", Some("other"), 0, false),
            ],
        );
        assert_eq!(
            queue
                .iter()
                .map(|i| (i.id.as_str(), i.position()))
                .collect::<Vec<_>>(),
            [("paused", 98), ("next", 0), ("movie", 35)]
        );
    }

    #[test]
    fn next_up_also_works_without_an_unfinished_title() {
        let queue = continue_watching(vec![], vec![entry("next", Some("show"), 0, false)]);
        assert_eq!(queue[0].id, "next");
        assert!(continue_watching(vec![], vec![]).is_empty());
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct CatalogQuery {
    pub kind: &'static str,
    pub sort: &'static str,
    pub descending: bool,
    pub favorites: bool,
    pub unwatched: bool,
    pub initial: Option<char>,
    pub library: Option<String>,
}

impl Item {
    pub fn catalog_kind(&self) -> Option<&'static str> {
        match self.collection_type.as_deref()? {
            "movies" => Some("Movie"),
            "tvshows" => Some("Series"),
            "music" => Some("MusicAlbum"),
            "musicvideos" => Some("MusicVideo"),
            "boxsets" => Some("BoxSet"),
            "playlists" => Some("Playlist"),
            "homevideos" => Some("Folder"),
            _ => None,
        }
    }
}

impl Default for CatalogQuery {
    fn default() -> Self {
        Self {
            kind: "Movie",
            sort: "SortName",
            descending: false,
            favorites: false,
            unwatched: false,
            initial: None,
            library: None,
        }
    }
}

impl CatalogQuery {
    pub fn favourites() -> Self {
        Self {
            kind: "Movie,Series,Episode,Video,BoxSet,Audio,MusicAlbum,MusicArtist,MusicVideo",
            favorites: true,
            ..Default::default()
        }
    }
}

impl Server {
    pub fn similar_titles(&self, id: &str) -> Result<Vec<Item>> {
        let page: Page = read(self.get(&["Items", id, "Similar"]).query(&[
            ("userId", self.user_id()?),
            ("limit", "24"),
            ("fields", "MediaSources,DateLastMediaAdded,ProviderIds"),
        ]))?;
        let origin = self.identity().map(|account| account.key());
        let mut seen = HashSet::from([id.to_owned()]);
        Ok(page
            .items
            .into_iter()
            .filter(|item| {
                matches!(item.kind.as_str(), "Movie" | "Series") && seen.insert(item.id.clone())
            })
            .map(|mut item| {
                item.origin = origin.clone();
                item
            })
            .collect())
    }
    pub(super) fn user_id(&self) -> Result<&str> {
        Ok(&self
            .account
            .as_ref()
            .context("Sign in to browse Jellyfin.")?
            .user)
    }

    pub fn host(&self) -> &str {
        self.base.host_str().unwrap_or("Jellyfin")
    }

    pub fn libraries(&self) -> Result<Vec<Item>> {
        let page: Page = read(
            self.get(&["UserViews"])
                .query(&[("userId", self.user_id()?)]),
        )?;
        Ok(page
            .items
            .into_iter()
            .filter(|i| {
                matches!(
                    i.collection_type.as_deref(),
                    None | Some(
                        "movies"
                            | "tvshows"
                            | "mixed"
                            | "homevideos"
                            | "boxsets"
                            | "playlists"
                            | "music"
                            | "musicvideos"
                    )
                )
            })
            .collect())
    }

    pub fn catalog(
        &self,
        q: &CatalogQuery,
        search: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<Page> {
        read(self.catalog_request(q, search, offset, limit)?.query(&[(
            "fields",
            "Overview,Genres,MediaSources,ProviderIds,SortName",
        )]))
    }

    /// Read only sort labels for the alphabet rail, in exactly the order of the poster grid.
    pub fn catalog_letters(&self, q: &CatalogQuery, offset: usize) -> Result<(Vec<char>, usize)> {
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Entry {
            name: String,
            sort_name: Option<String>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Entries {
            items: Vec<Entry>,
            total_record_count: usize,
        }
        let entries: Entries = read(self.catalog_request(q, None, offset, 512)?.query(&[
            ("fields", "SortName"),
            ("enableImages", "false"),
            ("enableUserData", "false"),
        ]))?;
        Ok((
            entries
                .items
                .iter()
                .map(|e| catalog_initial(e.sort_name.as_deref(), &e.name))
                .collect(),
            entries.total_record_count,
        ))
    }

    fn catalog_request(
        &self,
        q: &CatalogQuery,
        search: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<reqwest::blocking::RequestBuilder> {
        let folders = search.is_none() && q.kind == "Folder";
        let order = if q.descending {
            "Descending"
        } else {
            "Ascending"
        };
        let mut request = self.get(&["Items"]).query(&[
            ("userId", self.user_id()?),
            ("recursive", if folders { "false" } else { "true" }),
            ("limit", &limit.to_string()),
            ("startIndex", &offset.to_string()),
        ]);
        request = if folders {
            request.query(&[
                ("sortBy", format!("IsFolder,{}", q.sort)),
                ("sortOrder", format!("Ascending,{order}")),
            ])
        } else {
            request.query(&[
                (
                    "includeItemTypes",
                    if search.is_some() {
                        MOVING_PICTURES
                    } else {
                        q.kind
                    },
                ),
                ("sortBy", q.sort),
                ("sortOrder", order),
            ])
        };
        if let Some(term) = search {
            request = request.query(&[("searchTerm", term)]);
        }
        if let Some(id) = &q.library {
            request = request.query(&[("parentId", id)]);
        }
        if let Some(c) = q.initial {
            request = request.query(&[("nameStartsWith", c.to_string())]);
        }
        if q.unwatched {
            request = request.query(&[("isPlayed", "false")]);
        }
        if q.favorites {
            request = request.query(&[("filters", "IsFavorite")]);
        }
        Ok(request)
    }

    pub fn home_items(&self, feed: super::federation::HomeFeed) -> Result<Vec<Item>> {
        use super::federation::HomeFeed;
        let user = self.user_id()?;
        let query = [("userId", user), ("fields", FIELDS), ("limit", "12")];
        let request = match feed {
            HomeFeed::Resume => self.get(&["UserItems", "Resume"]).query(&query).query(&[
                ("mediaTypes", "Video"),
                ("includeItemTypes", "Movie,Episode,Video"),
            ]),
            HomeFeed::NextUp => self.get(&["Shows", "NextUp"]).query(&query),
            HomeFeed::RecentTv => self.get(&["Items"]).query(&[
                ("userId", user),
                ("recursive", "true"),
                ("includeItemTypes", "Series"),
                ("sortBy", "DateLastContentAdded,DateCreated"),
                ("sortOrder", "Descending"),
                (
                    "fields",
                    "DateLastMediaAdded,DateCreated,Genres,Overview,ProviderIds",
                ),
                ("limit", "24"),
            ]),
            HomeFeed::RecentMovies => {
                return read(self.get(&["Items", "Latest"]).query(&[
                    ("userId", user),
                    ("includeItemTypes", "Movie"),
                    ("groupItems", "false"),
                    ("fields", "DateCreated,Genres,People,Overview,MediaSources"),
                    ("limit", "24"),
                ]));
            }
        };
        Ok(read::<Page>(request)?.items)
    }

    pub fn set_user_flag(&self, id: &str, favorite: bool, value: bool) -> Result<super::UserData> {
        let path = if favorite {
            "UserFavoriteItems"
        } else {
            "UserPlayedItems"
        };
        let method = if value {
            reqwest::Method::POST
        } else {
            reqwest::Method::DELETE
        };
        read(
            self.request(method, &[path, id])
                .query(&[("userId", self.user_id()?)]),
        )
    }

    pub fn series_target(&self, series: &str) -> Result<Option<Item>> {
        let user = self.user_id()?;
        let mut resume: Page = read(self.get(&["UserItems", "Resume"]).query(&[
            ("userId", user),
            ("parentId", series),
            ("includeItemTypes", "Episode"),
            ("mediaTypes", "Video"),
            ("fields", FIELDS),
            ("limit", "12"),
        ]))?;
        self.identify_items(&mut resume.items);
        let from_series = |item: &Item| item.series_id.as_deref() == Some(series);
        let resumed = continue_watching(
            resume.items.into_iter().filter(from_series).collect(),
            vec![],
        );
        if let Some(episode) = resumed.into_iter().next() {
            return Ok(Some(episode));
        }
        let mut next: Page = read(self.get(&["Shows", "NextUp"]).query(&[
            ("userId", user),
            ("seriesId", series),
            ("fields", FIELDS),
            ("limit", "1"),
        ]))?;
        self.identify_items(&mut next.items);
        Ok(
            continue_watching(vec![], next.items.into_iter().filter(from_series).collect())
                .into_iter()
                .next(),
        )
    }

    pub fn seasons(&self, series: &str) -> Result<Vec<Item>> {
        let page: Page = read(
            self.get(&["Shows", series, "Seasons"])
                .query(&[("userId", self.user_id()?)]),
        )?;
        Ok(page.items)
    }

    fn episode_request(
        &self,
        series: &str,
        season: Option<&str>,
    ) -> reqwest::blocking::RequestBuilder {
        let request = self.get(&["Shows", series, "Episodes"]);
        match season {
            Some(id) => request.query(&[("seasonId", id)]),
            None => request,
        }
    }

    pub fn season_episodes(&self, series: &str, season: Option<&str>) -> Result<Vec<Item>> {
        let user = self.user_id()?;
        let mut items = Vec::new();
        loop {
            let page: Page = read(self.episode_request(series, season).query(&[
                ("userId", user),
                ("isMissing", "false"),
                ("fields", "Overview"),
                ("startIndex", &items.len().to_string()),
                ("limit", &PAGE_SIZE.to_string()),
            ]))?;
            let last = page.items.is_empty();
            items.extend(page.items);
            if last || items.len() >= page.total_record_count {
                self.identify_items(&mut items);
                return Ok(items);
            }
        }
    }

    pub fn children_page(
        &self,
        parent: &Item,
        season: Option<&str>,
        offset: usize,
        target: Option<&Item>,
    ) -> Result<(Page, usize)> {
        let first = self.children(parent, season, offset)?;
        let Some(target) = target else {
            return Ok((first, offset));
        };
        let contains = |p: &Page| p.items.iter().any(|i| i.id == target.id);
        if !contains(&first) && !first.items.is_empty() {
            let mut start = offset + first.items.len();
            while start < first.total_record_count {
                let page = self.children(parent, season, start)?;
                if contains(&page) {
                    return Ok((page, start));
                }
                if page.items.is_empty() {
                    break;
                }
                start += page.items.len();
            }
        }
        Ok((first, offset))
    }

    pub fn children(&self, item: &Item, season: Option<&str>, offset: usize) -> Result<Page> {
        let mut request = match item.kind.as_str() {
            "Series" => self.episode_request(&item.id, season),
            "Person" => self.get(&["Items"]).query(&[
                ("personIds", item.id.as_str()),
                ("recursive", "true"),
                ("includeItemTypes", "Movie,Series,Episode,MusicVideo"),
            ]),
            "MusicArtist" => self.get(&["Items"]).query(&[
                ("artistIds", item.id.as_str()),
                ("recursive", "true"),
                ("includeItemTypes", "MusicAlbum"),
                ("sortBy", "ProductionYear,SortName"),
            ]),
            "MusicAlbum" => self.get(&["Items"]).query(&[
                ("parentId", item.id.as_str()),
                ("includeItemTypes", "Audio"),
                ("sortBy", "ParentIndexNumber,IndexNumber"),
            ]),
            "Playlist" => self.get(&["Playlists", &item.id, "Items"]),
            _ => self
                .get(&["Items"])
                .query(&[("parentId", item.id.as_str()), ("sortBy", "SortName")]),
        };
        request = request.query(&[
            ("userId", self.user_id()?),
            ("fields", FIELDS),
            ("startIndex", &offset.to_string()),
            ("limit", &PAGE_SIZE.to_string()),
        ]);
        let mut page: Page = read(request)?;
        self.identify_items(&mut page.items);
        Ok(page)
    }

    pub fn match_tmdb(&self, name: &str, id: u64, movie: bool) -> Result<Vec<Item>> {
        let page: Page = read(self.get(&["Items"]).query(&[
            ("userId", self.user_id()?),
            ("searchTerm", name),
            ("recursive", "true"),
            ("includeItemTypes", if movie { "Movie" } else { "Series" }),
            ("fields", "ProviderIds,Overview"),
            ("collapseBoxSetItems", "false"),
        ]))?;
        let id = id.to_string();
        Ok(page
            .items
            .into_iter()
            .filter(|i| {
                i.provider_ids
                    .iter()
                    .flatten()
                    .any(|(k, v)| k.eq_ignore_ascii_case("Tmdb") && v == &id)
            })
            .collect())
    }
    pub(super) fn search_group(
        &self,
        kinds: &str,
        term: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page> {
        let request = if kinds == "Person" {
            self.get(&["Persons"])
        } else {
            self.get(&["Items"]).query(&[
                ("recursive", "true"),
                ("includeItemTypes", kinds),
                ("fields", FIELDS),
            ])
        };
        read(request.query(&[
            ("userId", self.user_id()?),
            ("searchTerm", term),
            ("startIndex", &offset.to_string()),
            ("limit", &limit.to_string()),
        ]))
    }

    pub fn queue(
        &self,
        query: Option<&CatalogQuery>,
        parent: Option<&Item>,
        shuffle: bool,
    ) -> Result<Vec<Item>> {
        use rand::seq::SliceRandom;
        let mut items = Vec::new();
        let mut offset = 0;
        loop {
            let mut request = match parent {
                Some(i) if i.kind == "Playlist" => self.get(&["Playlists", &i.id, "Items"]),
                _ => {
                    let kind = match query.map(|q| q.kind) {
                        Some("Movie") => "Movie",
                        Some("Series" | "Episode") => "Episode",
                        Some("MusicAlbum" | "Audio" | "MusicArtist") => "Audio",
                        _ => "Movie,Episode,Video,MusicVideo,Audio",
                    };
                    let mut req = self.get(&["Items"]).query(&[
                        ("recursive", "true"),
                        ("includeItemTypes", kind),
                        ("collapseBoxSetItems", "false"),
                        ("sortBy", "SortName"),
                        ("sortOrder", "Ascending"),
                    ]);
                    if let Some(q) = query {
                        if let Some(id) = &q.library {
                            req = req.query(&[("parentId", id)]);
                        }
                        if q.favorites {
                            req = req.query(&[("filters", "IsFavorite")]);
                        }
                        if q.unwatched {
                            req = req.query(&[("isPlayed", "false")]);
                        }
                        if let Some(c) = q.initial {
                            req = req.query(&[("nameStartsWith", c.to_string())]);
                        }
                    }
                    if let Some(i) = parent {
                        if i.kind == "MusicArtist" {
                            req = req.query(&[("artistIds", &i.id)]);
                        } else if i.kind == "Series" {
                            req = self
                                .get(&["Shows", &i.id, "Episodes"])
                                .query(&[("isMissing", "false")]);
                        } else {
                            req = req.query(&[("parentId", &i.id)]);
                        }
                    }
                    req
                }
            };
            request = request.query(&[
                ("userId", self.user_id()?),
                ("startIndex", &offset.to_string()),
                ("limit", "200"),
            ]);
            let page: Page = read(request)?;
            let n = page.items.len();
            items.extend(page.items.into_iter().filter(Item::playable));
            offset += n;
            if n == 0 || offset >= page.total_record_count {
                break;
            }
        }
        if shuffle {
            items.shuffle(&mut rand::rng());
        }
        self.identify_items(&mut items);
        Ok(items)
    }

    pub fn instant_mix(&self, id: &str) -> Result<Vec<Item>> {
        let mut page: Page = read(
            self.get(&["Items", id, "InstantMix"])
                .query(&[("userId", self.user_id()?), ("limit", "100")]),
        )?;
        self.identify_items(&mut page.items);
        Ok(page.items.into_iter().filter(Item::playable).collect())
    }
    pub fn artwork(&self, art: &Art) -> Result<Vec<u8>> {
        let (kind, format) = match art.kind {
            ArtKind::Poster | ArtKind::LargePoster | ArtKind::Thumb => ("Primary", "jpg"),
            ArtKind::SmallBackdrop | ArtKind::Backdrop => ("Backdrop", "jpg"),
            ArtKind::Chapter(_) => ("Chapter", "jpg"),
            ArtKind::Logo => ("Logo", "png"),
        };
        let index = match art.kind {
            ArtKind::Chapter(i) => Some(i.to_string()),
            _ => None,
        };
        let mut path = vec!["Items", art.owner.as_str(), "Images", kind];
        if let Some(i) = index.as_deref() {
            path.push(i);
        }
        let response = self
            .get(&path)
            .query(&[
                ("maxWidth", art.max_width.to_string().as_str()),
                ("tag", &art.tag),
                ("format", format),
            ])
            .send()?
            .error_for_status()?;
        let mut bytes = Vec::new();
        response.take(12 * 1024 * 1024).read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}
