use super::*;
use std::collections::HashSet;

impl Account {
    pub fn key(&self) -> String {
        crate::preferences::item_key(&format!("{}\0{}", self.server, self.user))
    }
}

impl Server {
    pub fn identify_items(&self, items: &mut [Item]) {
        let origin = self.identity().map(Account::key);
        for item in items {
            item.origin.clone_from(&origin);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HomeFeed {
    Resume,
    NextUp,
    RecentTv,
    RecentMovies,
}

impl HomeFeed {
    pub const ALL: [Self; 4] = [
        Self::Resume,
        Self::NextUp,
        Self::RecentTv,
        Self::RecentMovies,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Resume => "In progress",
            Self::NextUp => "Next Up",
            Self::RecentTv => ShelfKind::RecentTv.title(),
            Self::RecentMovies => ShelfKind::RecentMovies.title(),
        }
    }
}

pub fn home_shelves<'a>(
    sources: impl Iterator<Item = [&'a [Item]; 4]>,
    federated: bool,
    include_watching: impl Fn(&Item) -> bool,
) -> Vec<Shelf> {
    let mut watching = Vec::new();
    let mut shows = Vec::new();
    let mut films = Vec::new();
    for [resume, next, tv, movies] in sources {
        // Remove hidden episodes before choosing one card per series, so a
        // different eligible episode can still appear.
        watching.extend(catalog::continue_watching(
            resume
                .iter()
                .filter(|i| include_watching(i))
                .cloned()
                .collect(),
            next.iter()
                .filter(|i| include_watching(i))
                .cloned()
                .collect(),
        ));
        shows.extend_from_slice(tv);
        films.extend_from_slice(movies);
    }
    let mut shelves = vec![
        Shelf {
            kind: ShelfKind::Continue,
            items: copies(watching),
        },
        Shelf {
            kind: ShelfKind::RecentTv,
            items: shows,
        },
        Shelf {
            kind: ShelfKind::RecentMovies,
            items: films,
        },
    ];
    for row in &mut shelves {
        if federated && row.kind != ShelfKind::Continue {
            row.items.sort_by_cached_key(|item| {
                let added = match row.kind {
                    ShelfKind::RecentTv => item
                        .date_last_media_added
                        .as_ref()
                        .or(item.date_created.as_ref()),
                    _ => item.date_created.as_ref(),
                };
                std::cmp::Reverse(added.and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()))
            });
            if row.kind == ShelfKind::RecentTv {
                row.items = copies(std::mem::take(&mut row.items));
            }
        }
        row.items.truncate(24);
    }
    shelves.retain(|row| !row.items.is_empty());
    shelves
}

// Jellyfin installations can have different subsets of provider IDs for the same title.
fn copies(items: Vec<Item>) -> Vec<Item> {
    let mut groups: Vec<(HashSet<String>, Item)> = vec![];
    for item in items {
        let mut ids: HashSet<_> = item
            .provider_ids
            .iter()
            .flatten()
            .filter(|(provider, id)| {
                !id.is_empty()
                    && matches!(
                        provider.to_ascii_lowercase().as_str(),
                        "tmdb" | "imdb" | "tvdb"
                    )
            })
            .map(|(provider, id)| format!("{}:{}:{id}", item.kind, provider.to_ascii_lowercase()))
            .collect();
        let matches: Vec<_> = groups
            .iter()
            .enumerate()
            .filter(|(_, (keys, _))| !keys.is_disjoint(&ids))
            .map(|(i, _)| i)
            .collect();
        let Some((&first, rest)) = matches.split_first() else {
            groups.push((ids, item));
            continue;
        };
        for &i in rest.iter().rev() {
            let (keys, mut group) = groups.remove(i);
            ids.extend(keys);
            groups[first].1.copies.append(&mut group.copies);
            groups[first].1.copies.push(group);
        }
        groups[first].0.extend(ids);
        groups[first].1.copies.push(item);
    }
    groups.into_iter().map(|(_, item)| item).collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SearchGroup {
    Movies,
    Shows,
    Episodes,
    People,
    Music,
    Videos,
}

impl SearchGroup {
    pub const ALL: [Self; 6] = [
        Self::Movies,
        Self::Shows,
        Self::Episodes,
        Self::People,
        Self::Music,
        Self::Videos,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Movies => "Movies",
            Self::Shows => "Shows",
            Self::Episodes => "Episodes",
            Self::People => "People",
            Self::Music => "Music",
            Self::Videos => "Videos",
        }
    }

    pub fn on_server(
        self,
        server: &Server,
        term: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page> {
        let kinds = match self {
            Self::Movies => "Movie",
            Self::Shows => "Series",
            Self::Episodes => "Episode",
            Self::People => "Person",
            Self::Music => "MusicAlbum,MusicArtist,Audio",
            Self::Videos => "Video,MusicVideo",
        };
        let mut page = server.search_group(kinds, term, offset, limit)?;
        server.identify_items(&mut page.items);
        Ok(page)
    }
}

pub fn combined_page(
    servers: &[Server],
    offset: usize,
    get: impl Fn(&Server, usize) -> Result<Page>,
) -> Result<(Page, Vec<String>)> {
    combined_page_limit(servers, offset, PAGE_SIZE, get)
}

fn combined_page_limit(
    servers: &[Server],
    offset: usize,
    limit: usize,
    get: impl Fn(&Server, usize) -> Result<Page>,
) -> Result<(Page, Vec<String>)> {
    let mut items = Vec::new();
    let mut total = 0usize;
    let mut failures = Vec::new();
    for server in servers {
        // The first request supplies both the server's total and the requested slice.
        let start = offset.saturating_sub(total);
        let page = match get(server, start) {
            Ok(p) => p,
            Err(_) => {
                failures.push(server.host().to_owned());
                continue;
            }
        };
        total += page.total_record_count;
        if items.len() < limit {
            let key = server.identity().map(Account::key);
            items.extend(
                page.items
                    .into_iter()
                    .take(limit - items.len())
                    .map(|mut i| {
                        i.origin = key.clone();
                        i
                    }),
            );
        }
    }
    ensure!(
        failures.len() < servers.len(),
        "No connected Jellyfin server answered the request."
    );
    Ok((
        Page {
            items,
            total_record_count: total,
        },
        failures,
    ))
}

pub fn connections(primary: &Server, accounts: &[Account]) -> Vec<Server> {
    let mut active = HashSet::new();
    if let Some(a) = primary.identity() {
        active.insert(a.server.clone());
    }
    accounts
        .iter()
        .filter(|a| active.insert(a.server.clone()))
        .filter_map(|a| Server::restore(a.clone()).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_ids_merge_copies_but_distinct_kinds_stay_separate() {
        let fixture = |id, kind, providers| {
            serde_json::from_value(
                json!({"Id":id,"Name":"Fixture","Type":kind,"ProviderIds":providers}),
            )
            .unwrap()
        };
        let items = copies(vec![
            fixture("a", "Movie", json!({"Imdb":"tt1"})),
            fixture("b", "Movie", json!({"Tmdb":"42"})),
            fixture("c", "Series", json!({"Tmdb":"42"})),
            fixture("d", "Movie", json!({"imdb":"tt1","tmdb":"42"})),
            fixture("e", "Movie", json!({})),
        ]);
        assert_eq!(
            items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["a", "c", "e"]
        );
        assert_eq!(items[0].copies.len(), 2);
    }
}
