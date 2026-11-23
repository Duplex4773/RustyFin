use super::*;
use crate::seerr::*;
pub(super) struct Explore {
    pub address: Entity<InputState>,
    pub key: Entity<InputState>,
    pub search: Entity<InputState>,
    pub client: Option<Seerr>,
    pub section: &'static str,
    pub term: String,
    pub page: u32,
    pub results: Results,
    pub selected: Option<Entry>,
    pub options: RequestOptions,
    pub servers: Vec<Destination>,
    pub destinations: ServiceOptions,
    pub matching: Vec<Item>,
    pub sent: bool,
}
impl Explore {
    pub fn new(w: &mut Window, cx: &mut Context<Library>) -> Self {
        let search = cx.new(|cx| InputState::new(w, cx).placeholder("Search Seerr"));
        cx.subscribe_in(&search, w, |s, _, event, w, cx| {
            if matches!(s.state.screen, Screen::Explore)
                && matches!(event, InputEvent::PressEnter { .. })
            {
                s.explore.term = s.explore.search.read(cx).value().trim().to_owned();
                s.explore.page = 1;
                s.load_explore(w, cx);
            }
        })
        .detach();
        Self {
            address: cx.new(|cx| InputState::new(w, cx).placeholder("Seerr server address")),
            key: cx.new(|cx| {
                InputState::new(w, cx)
                    .masked(true)
                    .placeholder("Seerr API key")
            }),
            search,
            client: None,
            section: "trending",
            term: String::new(),
            page: 1,
            results: Default::default(),
            selected: None,
            options: Default::default(),
            servers: vec![],
            destinations: Default::default(),
            matching: vec![],
            sent: false,
        }
    }
}
impl Library {
    fn seerr_key(&self) -> Result<String> {
        let account = self
            .server
            .as_ref()
            .and_then(Server::identity)
            .context("Sign in to Jellyfin first.")?;
        Ok(format!(
            "RustFin/Seerr/{}",
            preferences::item_key(&format!("{}\0{}", account.server, account.user))
        ))
    }
    pub(super) fn restore_seerr(&mut self) {
        self.explore.client = None;
        if self.server.is_some() {
            match self
                .seerr_key()
                .and_then(|k| credentials::read_secret::<Connection>(&k))
                .and_then(|c| c.map(|c| Seerr::new(&c)).transpose())
            {
                Ok(client) => self.explore.client = client,
                Err(e) => self.message = format!("Could not restore Seerr. {e}"),
            }
        }
    }
    pub(super) fn connect_seerr(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Ok(key) = self.seerr_key() else { return };
        let config = Connection {
            address: self.explore.address.read(cx).value().trim().to_owned(),
            key: self.explore.key.read(cx).value().trim().to_owned(),
        };
        self.work(
            w,
            cx,
            move || {
                let client = Seerr::new(&config)?;
                client.check()?;
                credentials::write_secret(&key, &config)?;
                Ok(client)
            },
            |s, client, w, cx| {
                s.explore.client = Some(client);
                s.explore.key.update(cx, |i, cx| i.set_value("", w, cx));
                s.navigate(Screen::Explore, w, cx);
                Ok(())
            },
        );
    }
    pub(super) fn disconnect_seerr(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if let Err(e) = self
            .seerr_key()
            .and_then(|k| credentials::remove_secret(&k))
        {
            self.message = e.to_string();
        } else {
            self.explore = Explore::new(w, cx);
        }
        cx.notify();
    }
    pub(super) fn load_explore(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = self.explore.client.clone() else {
            self.busy = false;
            cx.notify();
            return;
        };
        let (section, term, page) = (
            self.explore.section,
            self.explore.term.clone(),
            self.explore.page,
        );
        self.explore.selected = None;
        self.work(
            w,
            cx,
            move || client.catalog(section, &term, page),
            |s, v, _, _| {
                s.explore.results = v;
                Ok(())
            },
        );
    }
    pub(super) fn load_explore_title(
        &mut self,
        item: Entry,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.explore.selected = None;
        let Some(client) = self.explore.client.clone() else {
            return;
        };
        let servers = self.browse_servers();
        self.work(
            w,
            cx,
            move || {
                let title = client.detail(&item)?;
                let mut ids = Vec::new();
                for server in servers {
                    if let Ok(mut rows) =
                        server.match_tmdb(title.name(), title.id, title.media_type == "movie")
                    {
                        for item in &mut rows {
                            item.origin = server.identity().map(crate::jellyfin::Account::key);
                        }
                        ids.extend(rows);
                    }
                }
                let destinations = client.destinations(&title.media_type).unwrap_or_default();
                Ok((title, ids, destinations))
            },
            |s, (title, ids, list), _, _| {
                s.explore.selected = Some(title);
                s.explore.matching = ids;
                s.explore.servers = list;
                s.explore.options = Default::default();
                s.explore.destinations = Default::default();
                s.explore.sent = false;
                Ok(())
            },
        );
    }
}
