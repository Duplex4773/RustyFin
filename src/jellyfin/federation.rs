use super::*;
use std::collections::HashSet;

impl Account {
    pub fn key(&self) -> String {
        crate::preferences::item_key(&format!("{}\0{}", self.server, self.user))
    }
}

pub fn discover(primary: &Server, others: &[Server]) -> Result<(Vec<Shelf>, Vec<String>)> {
    let mut shelves: Vec<Shelf> = vec![];
    let mut warnings = Vec::new();
    let mut answered = false;
    for source in std::iter::once(primary).chain(others) {
        let batch = match source.discover() {
            Ok(v) => {
                answered = true;
                v
            }
            Err(e) if others.is_empty() => return Err(e),
            Err(_) => {
                warnings.push(source.host().to_owned());
                continue;
            }
        };
        let origin = source.identity().map(Account::key);
        for mut row in batch {
            for item in &mut row.items {
                item.origin.clone_from(&origin);
            }
            if let Some(target) = shelves.iter_mut().find(|s| s.kind == row.kind) {
                target.items.append(&mut row.items);
            } else {
                shelves.push(row);
            }
        }
    }
    ensure!(answered, "No connected Jellyfin server answered Discover.");
    for row in &mut shelves {
        if row.kind == ShelfKind::Continue {
            row.items = copies(std::mem::take(&mut row.items));
        } else if !others.is_empty() {
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
    Ok((shelves, warnings))
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

pub fn search(
    servers: &[Server],
    term: &str,
    people: bool,
    offset: usize,
) -> Result<(Page, Vec<String>)> {
    combined_page(servers, offset, |server, start| {
        if people {
            server.people(term, start)
        } else {
            server.catalog(&CatalogQuery::default(), Some(term), start, PAGE_SIZE)
        }
    })
}

pub fn combined_page(
    servers: &[Server],
    offset: usize,
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
        if items.len() < PAGE_SIZE {
            let key = server.identity().map(Account::key);
            items.extend(
                page.items
                    .into_iter()
                    .take(PAGE_SIZE - items.len())
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
