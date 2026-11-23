use super::*;
use crate::services::{ActivityInfo, Presence, Ratings};
use std::sync::Arc;

pub(super) struct Services {
    pub mdb_input: Entity<InputState>,
    pub discord_input: Entity<InputState>,
    pub ratings: Option<Arc<Ratings>>,
    pub presence: Option<Presence>,
    rating_job: Option<Task<()>>,
}
impl Services {
    pub fn new(w: &mut Window, cx: &mut Context<Library>) -> Self {
        Self {
            mdb_input: cx.new(|cx| {
                InputState::new(w, cx)
                    .masked(true)
                    .placeholder("MDBList API key")
            }),
            discord_input: cx
                .new(|cx| InputState::new(w, cx).placeholder("Discord application ID")),
            ratings: None,
            presence: None,
            rating_job: None,
        }
    }
}
impl Library {
    fn mdb_key(&self) -> Result<String> {
        Ok(format!(
            "RustFin/MDBList/{}",
            self.server
                .as_ref()
                .and_then(Server::identity)
                .context("Sign in to Jellyfin first.")?
                .key()
        ))
    }
    pub(super) fn restore_services(&mut self) {
        self.services.rating_job = None;
        self.services.ratings = self
            .mdb_key()
            .ok()
            .and_then(|k| credentials::read_secret::<String>(&k).ok().flatten())
            .and_then(|k| Ratings::new(k).ok())
            .map(Arc::new);
        self.services.presence = self
            .prefs
            .discord_app
            .as_deref()
            .and_then(|id| Presence::connect(id).ok());
    }
    pub(super) fn connect_ratings(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Ok(store) = self.mdb_key() else { return };
        if self.services.ratings.is_some() {
            match credentials::remove_secret(&store) {
                Ok(()) => self.services.ratings = None,
                Err(e) => self.message = e.to_string(),
            }
            cx.notify();
            return;
        }
        let key = self.services.mdb_input.read(cx).value().trim().to_owned();
        self.work(
            w,
            cx,
            move || {
                let client = Ratings::new(key.clone())?;
                client.verify()?;
                credentials::write_secret(&store, &key)?;
                Ok(client)
            },
            |s, client, w, cx| {
                s.services.ratings = Some(Arc::new(client));
                s.services
                    .mdb_input
                    .update(cx, |i, cx| i.set_value("", w, cx));
                Ok(())
            },
        );
    }
    pub(super) fn connect_discord(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.prefs.discord_app.is_some() {
            self.services.presence = None;
            self.prefs.discord_app = None;
            self.save_preferences(cx);
            return;
        }
        let id = self
            .services
            .discord_input
            .read(cx)
            .value()
            .trim()
            .to_owned();
        match Presence::connect(&id) {
            Ok(p) => {
                self.services.presence = Some(p);
                self.prefs.discord_app = Some(id);
                self.services
                    .discord_input
                    .update(cx, |i, cx| i.set_value("", w, cx));
                self.save_preferences(cx);
            }
            Err(e) => self.message = e.to_string(),
        }
        cx.notify();
    }
    pub(super) fn update_presence(&mut self) {
        let Some(discord) = &mut self.services.presence else {
            return;
        };
        let activity = self
            .player
            .as_ref()
            .filter(|p| p.status.ready)
            .zip(self.playing.as_ref())
            .map(|(p, item)| {
                let title = item.name.chars().take(100).collect();
                let state = if p.status.paused {
                    "Paused".into()
                } else if let Some(series) = &item.series_name {
                    series.clone()
                } else if p.audio_only {
                    item.artists
                        .as_ref()
                        .map(|v| v.join(", "))
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "Listening".into())
                } else {
                    "Watching".into()
                };
                ActivityInfo {
                    title,
                    state: state.chars().take(100).collect(),
                    music: p.audio_only,
                }
            });
        discord.update(activity);
    }
    pub(super) fn fetch_ratings(&mut self, w: &Window, cx: &mut Context<Self>) {
        let (Some(service), Screen::Detail(item, _)) = (&self.services.ratings, &self.state.screen)
        else {
            return;
        };
        if self.state.ratings.is_some() || self.busy || self.state.ratings_pending {
            return;
        }
        self.state.ratings_pending = true;
        let item = item.clone();
        let id = item.id.clone();
        let client = service.clone();
        let worker = cx
            .background_executor()
            .spawn(async move { client.lookup(&item) });
        self.services.rating_job = Some(cx.spawn_in(w, async move |view, cx| {
            let result = worker.await;
            let _ = view.update_in(cx, |s, _, cx| {
                if matches!(&s.state.screen, Screen::Detail(i,_) if i.id==id) {
                    s.state.ratings_pending = false;
                    s.state.ratings = Some(result.map_err(|e| e.to_string()));
                    cx.notify();
                }
            });
        }));
    }
}
