use anyhow::{Context, Result, ensure};
use reqwest::blocking::{Client, RequestBuilder};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::time::Duration;
use url::Url;

#[derive(Clone, Deserialize, Serialize)]
pub struct Connection {
    pub address: String,
    pub key: String,
}
#[derive(Clone)]
pub struct Seerr {
    base: Url,
    http: Client,
    key: String,
}
#[derive(Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    pub media_type: String,
    pub title: Option<String>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub release_date: Option<String>,
    pub first_air_date: Option<String>,
    pub media_info: Option<Availability>,
    pub seasons: Vec<Season>,
}
#[derive(Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Season {
    pub season_number: u32,
    pub name: String,
}
#[derive(Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Availability {
    pub status: u8,
    #[serde(rename = "status4k")]
    pub uhd: u8,
}
impl Entry {
    pub fn name(&self) -> &str {
        self.title
            .as_deref()
            .or(self.name.as_deref())
            .unwrap_or("Untitled")
    }
    pub fn status(&self, uhd: bool) -> u8 {
        self.media_info
            .as_ref()
            .map_or(1, |i| if uhd { i.uhd } else { i.status })
    }
    pub fn status_name(&self, uhd: bool) -> &'static str {
        match self.status(uhd) {
            2 => "Requested",
            3 => "Processing",
            4 => "Partially available",
            5 => "Available",
            6 => "Blocked",
            7 => "Deleted",
            _ => "Not requested",
        }
    }
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Results {
    pub results: Vec<Entry>,
    pub total_pages: u32,
}
#[derive(Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Destination {
    pub id: u32,
    pub name: String,
    pub is4k: bool,
}
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ServiceOptions {
    pub profiles: Vec<Named>,
    pub root_folders: Vec<Folder>,
}
#[derive(Deserialize)]
pub struct Named {
    pub id: u32,
    pub name: String,
}
#[derive(Deserialize)]
pub struct Folder {
    pub path: String,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestOptions {
    #[serde(rename = "is4k")]
    pub uhd: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_folder: Option<String>,
    #[serde(skip)]
    pub seasons: Vec<u32>,
}
impl Seerr {
    pub fn new(connection: &Connection) -> Result<Self> {
        let mut base = Url::parse(connection.address.trim())?;
        ensure!(
            matches!(base.scheme(), "http" | "https")
                && base.host_str().is_some()
                && base.username().is_empty()
                && base.password().is_none()
                && base.query().is_none()
                && base.fragment().is_none(),
            "Enter an HTTP or HTTPS Seerr server address."
        );
        ensure!(
            !connection.key.trim().is_empty(),
            "Enter the Seerr API key."
        );
        base.path_segments_mut()
            .unwrap()
            .pop_if_empty()
            .extend(["api", "v1", ""]);
        Ok(Self {
            base,
            http: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(15))
                .build()?,
            key: connection.key.trim().to_owned(),
        })
    }
    fn request(&self, method: reqwest::Method, path: &[&str]) -> RequestBuilder {
        let mut url = self.base.clone();
        url.path_segments_mut()
            .unwrap()
            .pop_if_empty()
            .extend(path.iter().copied());
        self.http
            .request(method, url)
            .header("X-Api-Key", &self.key)
    }
    fn get<T: DeserializeOwned>(&self, path: &[&str], query: &[(&str, String)]) -> Result<T> {
        Ok(self
            .request(reqwest::Method::GET, path)
            .query(query)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.without_url())?
            .json()?)
    }
    pub fn check(&self) -> Result<()> {
        let _: Value = self.get(&["auth", "me"], &[])?;
        Ok(())
    }
    pub fn catalog(&self, section: &str, term: &str, page: u32) -> Result<Results> {
        let mut query = vec![("page", page.to_string())];
        let mut data: Results = if !term.trim().is_empty() {
            query.push(("query", term.trim().to_owned()));
            self.get(&["search"], &query)?
        } else {
            self.get(&["discover", section], &query)?
        };
        for e in &mut data.results {
            if e.media_type.is_empty() {
                e.media_type = if e.title.is_some() { "movie" } else { "tv" }.into();
            }
        }
        data.results
            .retain(|e| matches!(e.media_type.as_str(), "movie" | "tv"));
        Ok(data)
    }
    pub fn detail(&self, e: &Entry) -> Result<Entry> {
        let mut item: Entry = self.get(&[&e.media_type, &e.id.to_string()], &[])?;
        item.media_type = e.media_type.clone();
        Ok(item)
    }
    fn service(kind: &str) -> &str {
        if kind == "movie" { "radarr" } else { "sonarr" }
    }
    pub fn destinations(&self, kind: &str) -> Result<Vec<Destination>> {
        self.get(&["service", Self::service(kind)], &[])
    }
    pub fn options(&self, kind: &str, id: u32) -> Result<ServiceOptions> {
        self.get(&["service", Self::service(kind), &id.to_string()], &[])
    }
    pub fn submit(&self, e: &Entry, options: &RequestOptions) -> Result<()> {
        ensure!(
            matches!(e.media_type.as_str(), "tv" | "movie"),
            "Choose a movie or show."
        );
        let mut payload = serde_json::to_value(options)?;
        let body = payload
            .as_object_mut()
            .context("Invalid request options.")?;
        body.insert("mediaType".into(), json!(e.media_type));
        body.insert("mediaId".into(), json!(e.id));
        if e.media_type == "tv" {
            body.insert(
                "seasons".into(),
                if options.seasons.is_empty() {
                    json!("all")
                } else {
                    json!(options.seasons)
                },
            );
        }
        self.request(reqwest::Method::POST, &["request"])
            .json(&payload)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.without_url())?;
        Ok(())
    }
}
