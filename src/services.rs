use crate::jellyfin::Item;
use anyhow::{Context, Result, ensure};
use discord_rich_presence::{
    DiscordIpc, DiscordIpcClient,
    activity::{Activity, ActivityType},
};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::{
    sync::mpsc::{self, RecvTimeoutError, Sender},
    time::Duration,
};

pub struct Ratings {
    http: Client,
    key: String,
}
#[derive(Clone, Deserialize)]
pub struct Rating {
    pub source: String,
    pub score: Option<f64>,
    pub value: Option<f64>,
}
impl Rating {
    pub fn imdb_score(&self) -> Option<f64> {
        if !self.source.eq_ignore_ascii_case("imdb") {
            return None;
        }
        self.value
            .filter(|&score| score > 0. && score <= 10.)
            .or_else(|| {
                self.score
                    .filter(|&score| score > 0. && score <= 100.)
                    .map(|score| score / 10.)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::Rating;

    #[test]
    fn imdb_score_requires_attribution_and_uses_the_ten_point_scale() {
        let mut rating = Rating {
            source: "tmdb".into(),
            value: Some(8.4),
            score: Some(84.),
        };
        assert_eq!(rating.imdb_score(), None);

        rating.source = "IMDb".into();
        rating.score = Some(83.);
        assert_eq!(rating.imdb_score(), Some(8.4));
        rating.value = None;
        assert_eq!(rating.imdb_score(), Some(8.3));
        rating.score = Some(101.);
        assert_eq!(rating.imdb_score(), None);
    }
}

impl Ratings {
    pub fn new(key: String) -> Result<Self> {
        ensure!(!key.trim().is_empty(), "Enter the MDBList API key.");
        Ok(Self {
            http: Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(8))
                .build()?,
            key,
        })
    }
    fn get<T: serde::de::DeserializeOwned>(&self, path: &[&str]) -> Result<T> {
        let mut url = url::Url::parse("https://api.mdblist.com/")?;
        url.path_segments_mut()
            .unwrap()
            .pop_if_empty()
            .extend(path.iter().copied());
        self.http
            .get(url)
            .query(&[("apikey", &self.key)])
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json())
            .map_err(|e| e.without_url().into())
    }
    pub fn verify(&self) -> Result<()> {
        let _: serde_json::Value = self.get(&["user"])?;
        Ok(())
    }
    pub fn lookup(&self, item: &Item) -> Result<Vec<Rating>> {
        let kind = match item.kind.as_str() {
            "Movie" => "movie",
            "Series" => "show",
            _ => return Ok(vec![]),
        };
        let Some(ids) = &item.provider_ids else {
            return Ok(vec![]);
        };
        let Some((provider, id)) = ["imdb", "tmdb", "tvdb"].into_iter().find_map(|p| {
            ids.iter()
                .find(|(k, v)| k.eq_ignore_ascii_case(p) && !v.is_empty())
                .map(|(_, v)| (p, v))
        }) else {
            return Ok(vec![]);
        };
        #[derive(Deserialize)]
        struct Detail {
            #[serde(default)]
            ratings: Vec<Rating>,
        }
        let result: Detail = self.get(&[provider, kind, id, ""])?;
        Ok(result
            .ratings
            .into_iter()
            .filter(|r| r.score.is_some() || r.value.is_some())
            .collect())
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ActivityInfo {
    pub title: String,
    pub state: String,
    pub music: bool,
}
pub struct Presence {
    sender: Sender<Option<ActivityInfo>>,
    last: Option<ActivityInfo>,
}
impl Presence {
    pub fn connect(id: &str) -> Result<Self> {
        ensure!(
            id.len() >= 16 && id.len() <= 20 && id.bytes().all(|c| c.is_ascii_digit()),
            "Enter the numeric application ID from the Discord Developer Portal."
        );
        let id = id.to_owned();
        let (sender, receiver) = mpsc::channel::<Option<ActivityInfo>>();
        std::thread::Builder::new()
            .name("Discord presence".into())
            .spawn(move || {
                let mut ipc: Option<DiscordIpcClient> = None;
                let mut activity: Option<ActivityInfo> = None;
                loop {
                    match receiver.recv_timeout(Duration::from_secs(20)) {
                        Ok(value) => {
                            activity = value;
                            while let Ok(latest) = receiver.try_recv() {
                                activity = latest;
                            }
                        }
                        Err(RecvTimeoutError::Disconnected) => break,
                        Err(RecvTimeoutError::Timeout) if activity.is_none() => continue,
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                    let Some(a) = &activity else {
                        if let Some(mut c) = ipc.take() {
                            let _ = c.clear_activity();
                            let _ = c.close();
                        }
                        continue;
                    };
                    if ipc.is_none() {
                        let mut c = DiscordIpcClient::new(&id);
                        if c.connect().is_err() {
                            continue;
                        }
                        ipc = Some(c);
                    }
                    let note = Activity::new()
                        .details(&*a.title)
                        .state(&*a.state)
                        .activity_type(if a.music {
                            ActivityType::Listening
                        } else {
                            ActivityType::Watching
                        });
                    if ipc.as_mut().unwrap().set_activity(note).is_err()
                        && let Some(mut c) = ipc.take()
                    {
                        let _ = c.close();
                    }
                }
                if let Some(mut c) = ipc {
                    let _ = c.clear_activity();
                    let _ = c.close();
                }
            })
            .context("Starting Discord Rich Presence")?;
        Ok(Self { sender, last: None })
    }
    pub fn update(&mut self, activity: Option<ActivityInfo>) {
        if self.last != activity {
            let _ = self.sender.send(activity.clone());
            self.last = activity;
        }
    }
}
