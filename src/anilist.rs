use anyhow::{Context, Result, bail};
use reqwest::blocking::Client;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::time::Duration;

#[derive(Clone)]
pub struct AniList {
    http: Client,
    token: Option<String>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub id: u64,
    pub id_mal: Option<u64>,
    pub title: Title,
    pub format: Option<String>,
    pub episodes: Option<u32>,
    pub average_score: Option<u32>,
    pub site_url: String,
    pub media_list_entry: Option<ListEntry>,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Title {
    pub user_preferred: String,
    pub english: Option<String>,
    pub romaji: Option<String>,
}
#[derive(Clone, Deserialize)]
pub struct ListEntry {
    pub id: u64,
    pub status: Option<String>,
    pub progress: Option<u32>,
    pub score: Option<f64>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
}
#[derive(Clone, Copy)]
pub enum Feed {
    Trending,
    Popular,
    Watchlist,
}
pub enum Update {
    Plan,
    Remove,
    Rate(u32),
    Watched(u32, bool),
}
const FIELDS: &str = "id idMal title {userPreferred english romaji} format episodes averageScore siteUrl mediaListEntry { id status progress score(format:POINT_10) }";
impl AniList {
    pub fn new(token: Option<String>) -> Result<Self> {
        let http = Client::builder()
            .timeout(Duration::from_secs(12))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { http, token })
    }
    fn query<T: DeserializeOwned>(&self, query: &str, variables: Value, field: &str) -> Result<T> {
        let mut request = self.http.post("https://graphql.anilist.co");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response: Value = request
            .json(&json!({"query":query,"variables":variables}))
            .send()?
            .error_for_status()?
            .json()?;
        if let Some(error) = response
            .pointer("/errors/0/message")
            .and_then(Value::as_str)
        {
            bail!("AniList: {error}");
        }
        Ok(serde_json::from_value(
            response
                .get("data")
                .and_then(|data| data.get(field))
                .context("Missing AniList response")?
                .clone(),
        )?)
    }
    pub fn viewer(&self) -> Result<u64> {
        #[derive(Deserialize)]
        struct Viewer {
            id: u64,
        }
        Ok(self
            .query::<Viewer>("query{Viewer{id}}", json!({}), "Viewer")?
            .id)
    }
    pub fn feed(
        &self,
        feed: &Feed,
        search: &str,
        page: u32,
        viewer: Option<u64>,
    ) -> Result<(Vec<Media>, bool)> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Results {
            page_info: PageInfo,
            #[serde(default)]
            media: Vec<Media>,
            #[serde(default)]
            media_list: Vec<Row>,
        }
        #[derive(Deserialize)]
        struct Row {
            media: Media,
        }
        let needle = search.trim();
        let mut vars = json!({"page":page});
        let query = if matches!(feed, Feed::Watchlist) && needle.is_empty() {
            vars["viewer"] =
                json!(viewer.context("Connect an AniList account for your watchlist.")?);
            format!(
                "query($viewer:Int!,$page:Int!){{Page(page:$page,perPage:25){{pageInfo{{hasNextPage}} mediaList(userId:$viewer,type:ANIME,status:PLANNING,sort:UPDATED_TIME_DESC){{media{{{FIELDS}}}}}}}}}"
            )
        } else {
            let sort = if !needle.is_empty() {
                "SEARCH_MATCH"
            } else if matches!(feed, Feed::Popular) {
                "POPULARITY_DESC"
            } else {
                "TRENDING_DESC"
            };
            let declaration = if needle.is_empty() {
                ""
            } else {
                vars["term"] = json!(needle);
                ",$term:String!"
            };
            let filter = if needle.is_empty() {
                ""
            } else {
                ",search:$term"
            };
            format!(
                "query($page:Int!{declaration}){{Page(page:$page,perPage:25){{pageInfo{{hasNextPage}} media(type:ANIME,isAdult:false,sort:{sort}{filter}){{{FIELDS}}}}}}}"
            )
        };
        let mut result: Results = self.query(&query, vars, "Page")?;
        result
            .media
            .extend(result.media_list.into_iter().map(|r| r.media));
        Ok((result.media, result.page_info.has_next_page))
    }
    pub fn media(&self, id: u64) -> Result<Media> {
        self.query(
            &format!("query($id:Int!){{Media(id:$id,type:ANIME){{{FIELDS}}}}}"),
            json!({"id":id}),
            "Media",
        )
    }
    pub fn sync_finished(
        &self,
        server: &crate::jellyfin::Server,
        item: &crate::jellyfin::Item,
    ) -> Result<()> {
        let episode = item.kind == "Episode";
        if !episode && item.kind != "Movie" {
            return Ok(());
        }
        let key = |i: &crate::jellyfin::Item| {
            let ids = i.provider_ids.as_ref()?;
            ["anilist", "myanimelist", "mal"]
                .into_iter()
                .find_map(|provider| {
                    ids.iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case(provider))
                        .and_then(|(_, s)| s.parse::<u64>().ok())
                        .map(|n| (if provider == "anilist" { "id" } else { "idMal" }, n))
                })
        };
        let entry = if episode {
            let season = item
                .season_id
                .as_deref()
                .map(|id| server.item(id))
                .transpose()?;
            let season_key = season.as_ref().and_then(key);
            if season_key.is_some() {
                season_key
            } else if item.parent_index_number == Some(1) {
                item.series_id
                    .as_deref()
                    .map(|id| server.item(id))
                    .transpose()?
                    .as_ref()
                    .and_then(key)
            } else {
                None
            }
        } else {
            key(item)
        };
        let Some((field, id)) = entry else {
            return Ok(());
        };
        let media: Media = self.query(
            &format!("query($id:Int!){{Media({field}:$id,type:ANIME){{{FIELDS}}}}}"),
            json!({"id":id}),
            "Media",
        )?;
        if !episode && media.format.as_deref() != Some("MOVIE") {
            return Ok(());
        }
        let progress = if episode {
            item.index_number.unwrap_or(0)
        } else {
            1
        };
        if progress == 0
            || media.episodes.is_some_and(|n| progress > n)
            || media
                .media_list_entry
                .as_ref()
                .and_then(|r| r.progress)
                .is_some_and(|n| n >= progress)
        {
            return Ok(());
        }
        self.update(
            &media,
            Update::Watched(progress, !episode || media.episodes == Some(progress)),
        )?;
        Ok(())
    }
    pub fn update(&self, m: &Media, change: Update) -> Result<Media> {
        let mut args = json!({"id":m.id});
        match change {
            Update::Remove => {
                let row = m
                    .media_list_entry
                    .as_ref()
                    .context("This title is not in your list.")?;
                self.query::<Value>(
                    "mutation($id:Int!){DeleteMediaListEntry(id:$id){deleted}}",
                    json!({"id":row.id}),
                    "DeleteMediaListEntry",
                )?;
            }
            change => {
                let (declarations, fields) = match change {
                    Update::Plan => {
                        args["status"] = json!("PLANNING");
                        (",$status:MediaListStatus!", ",status:$status")
                    }
                    Update::Rate(n) => {
                        anyhow::ensure!(n <= 10, "Choose a score out of 10.");
                        args["score"] = json!(n * 10);
                        (",$score:Int!", ",scoreRaw:$score")
                    }
                    Update::Watched(n, done) => {
                        args["progress"] = json!(n);
                        args["status"] = json!(if done { "COMPLETED" } else { "CURRENT" });
                        (
                            ",$progress:Int!,$status:MediaListStatus!",
                            ",progress:$progress,status:$status",
                        )
                    }
                    Update::Remove => unreachable!(),
                };
                self.query::<Value>(&format!("mutation($id:Int!{declarations}){{SaveMediaListEntry(mediaId:$id{fields}){{id}}}}"),args,"SaveMediaListEntry")?;
            }
        }
        self.media(m.id)
    }
}
