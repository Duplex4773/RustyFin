use super::*;
use chrono::{DateTime, Local, Utc};
use std::sync::Arc;

pub struct LiveStream {
    client: Server,
    id: String,
}
impl Drop for LiveStream {
    fn drop(&mut self) {
        let server = self.client.clone();
        let id = self.id.clone();
        std::thread::spawn(move || {
            let _ = server
                .post(&["LiveStreams", "Close"])
                .query(&[("liveStreamId", id)])
                .send();
        });
    }
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Timer {
    pub id: String,
    pub name: Option<String>,
    pub channel_name: Option<String>,
    pub start_date: Option<String>,
}
pub fn local_time(value: Option<&str>) -> String {
    value
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| {
            d.with_timezone(&Local)
                .format("%a %d %b  %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}
impl Server {
    pub fn channels(&self, favorites: bool, offset: usize) -> Result<Page> {
        let mut req = self.get(&["LiveTv", "Channels"]).query(&[
            ("userId", self.user_id()?),
            ("addCurrentProgram", "true"),
            ("startIndex", &offset.to_string()),
            ("limit", &PAGE_SIZE.to_string()),
        ]);
        if favorites {
            req = req.query(&[("isFavorite", "true")]);
        }
        read(req)
    }
    pub fn schedule(&self, id: &str, offset: usize) -> Result<Page> {
        read(self.get(&["LiveTv", "Programs"]).query(&[
            ("userId", self.user_id()?),
            ("channelIds", id),
            ("minEndDate", &Utc::now().to_rfc3339()),
            ("sortBy", "StartDate"),
            ("sortOrder", "Ascending"),
            ("startIndex", &offset.to_string()),
            ("limit", &PAGE_SIZE.to_string()),
        ]))
    }
    pub fn timers(&self, series: bool) -> Result<Vec<Timer>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Timers {
            items: Vec<Timer>,
        }
        Ok(
            read::<Timers>(self.get(&["LiveTv", if series { "SeriesTimers" } else { "Timers" }]))?
                .items,
        )
    }
    pub fn record(&self, program: &str, series: bool) -> Result<()> {
        let defaults: serde_json::Value = read(
            self.get(&["LiveTv", "Timers", "Defaults"])
                .query(&[("programId", program)]),
        )?;
        self.post(&["LiveTv", if series { "SeriesTimers" } else { "Timers" }])
            .json(&defaults)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.without_url())?;
        Ok(())
    }
    pub fn cancel_timer(&self, id: &str, series: bool) -> Result<()> {
        self.request(
            reqwest::Method::DELETE,
            &["LiveTv", if series { "SeriesTimers" } else { "Timers" }, id],
        )
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.without_url())?;
        Ok(())
    }
    pub(super) fn open_tuner(
        &self,
        item: &Item,
        source: &Source,
        session: &str,
    ) -> Result<(Source, Option<Arc<LiveStream>>)> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Open {
            media_source: Source,
        }
        let token = source
            .open_token
            .as_deref()
            .context("Jellyfin did not supply a tuner token.")?;
        let request = json!({
            "ItemId": item.id,
            "UserId": self.user_id()?,
            "PlaySessionId": session,
            "OpenToken": token,
            "EnableDirectPlay": true,
            "EnableDirectStream": false,
            "DeviceProfile": direct_play_profile()
        });
        let opened: Open = read(self.post(&["LiveStreams", "Open"]).json(&request))?;
        let id = opened
            .media_source
            .live_stream_id
            .clone()
            .context("Jellyfin did not return a live stream ID.")?;
        let lease = Arc::new(LiveStream {
            client: self.clone(),
            id,
        });
        Ok((opened.media_source, Some(lease)))
    }
}
