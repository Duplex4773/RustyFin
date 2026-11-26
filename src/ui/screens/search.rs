use super::*;
use crate::jellyfin::federation::SearchGroup;
use std::ops::Range;

const SEARCH_BATCH: usize = 24;

pub(in crate::ui) struct ResultGroup {
    pub kind: SearchGroup,
    pub page: Page,
    server: Server,
    start: usize,
    first: usize,
    handles: Vec<FocusHandle>,
}

impl ResultGroup {
    fn new(kind: SearchGroup, server: Server, page: Page, start: usize, cx: &App) -> Self {
        let handles = page.items.iter().map(|_| cx.focus_handle()).collect();
        Self {
            kind,
            server,
            page,
            start,
            first: 0,
            handles,
        }
    }

    pub(in crate::ui) fn visible(&self, columns: usize) -> Range<usize> {
        self.first..(self.first + columns).min(self.page.items.len())
    }

    fn heading(&self, federated: bool) -> SharedString {
        if !federated {
            return self.kind.label().into();
        }
        let source = self.server.identity().map_or_else(
            || self.server.host().to_owned(),
            |account| {
                let address = account
                    .server
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_end_matches('/');
                match &account.username {
                    Some(user) => format!("{address} ({user})"),
                    None => address.to_owned(),
                }
            },
        );
        format!("{} · {source}", self.kind.label()).into()
    }
}

#[derive(Default)]
pub(in crate::ui) struct SearchState {
    pub groups: Vec<ResultGroup>,
    pub best: Option<Item>,
    pub target: Option<Item>,
    hero: Vec<FocusHandle>,
    pub needs_target: bool,
    return_focus: Option<FocusHandle>,
    pub restore_after_playback: bool,
    request: u64,
    federated: bool,
    pending_servers: usize,
    failures: Vec<String>,
    jobs: Vec<Task<()>>,
}

impl SearchState {
    fn receive(
        &mut self,
        server: Server,
        responses: Vec<(SearchGroup, Result<Page>)>,
        term: &str,
        w: &Window,
        cx: &App,
    ) -> bool {
        self.pending_servers = self.pending_servers.saturating_sub(1);
        let failed: Vec<_> = responses
            .iter()
            .filter_map(|(kind, result)| result.is_err().then_some(kind.label()))
            .collect();
        if !failed.is_empty() {
            let host = server
                .identity()
                .map_or_else(|| server.host(), |a| a.server.as_str());
            self.failures
                .push(if failed.len() == SearchGroup::ALL.len() {
                    format!("Couldn’t search {host}.")
                } else {
                    format!("Couldn’t search {} on {host}.", failed.join(", "))
                });
        }
        let best_from_server = best_title(
            term,
            responses
                .iter()
                .filter_map(|(_, result)| result.as_ref().ok())
                .flat_map(|page| page.items.iter()),
        )
        .cloned();
        for (kind, result) in responses {
            if let Ok(page) = result
                && !page.items.is_empty()
            {
                self.groups
                    .push(ResultGroup::new(kind, server.clone(), page, 0, cx));
            }
        }
        if self
            .hero
            .iter()
            .any(|handle| handle.is_focused(w) || self.return_focus.as_ref() == Some(handle))
        {
            return false;
        }
        let best = best_title(term, self.best.iter().chain(&best_from_server));
        if self.best.as_ref().map(|i| (&i.id, &i.origin)) == best.map(|i| (&i.id, &i.origin)) {
            return false;
        }
        self.best = best.cloned();
        self.target = self.best.as_ref().filter(|item| item.playable()).cloned();
        self.needs_target = self.best.as_ref().is_some_and(|item| item.kind == "Series");
        if self.best.is_some() && self.hero.is_empty() {
            self.hero = vec![cx.focus_handle(), cx.focus_handle()];
        }
        true
    }

    pub(in crate::ui) fn restore_focus(&mut self, window: &mut Window) -> bool {
        let Some(mut focus) = self.return_focus.clone() else {
            return false;
        };
        if self.hero.contains(&focus) {
            if self.target.is_none() {
                focus = self.hero[0].clone();
            }
            focus.focus(window);
            return true;
        }
        for group in &mut self.groups {
            if let Some(index) = group.handles.iter().position(|h| h == &focus) {
                let columns = search_grid(group.kind, window).columns;
                if !group.visible(columns).contains(&index) {
                    group.first = index.saturating_sub(columns - 1);
                }
                focus.focus(window);
                return true;
            }
        }
        false
    }
}

pub(in crate::ui) fn search_grid(kind: SearchGroup, w: &Window) -> CatalogGrid {
    let width = f32::from(w.viewport_size().width) - 2. * PAD;
    match kind {
        SearchGroup::Episodes | SearchGroup::Videos => {
            CatalogGrid::fit(width, w.scale_factor(), 235., 12)
        }
        SearchGroup::People => CatalogGrid::fit(width, w.scale_factor(), 132., 20),
        _ => CatalogGrid::for_window(w),
    }
}

fn best_title<'a>(term: &str, items: impl Iterator<Item = &'a Item>) -> Option<&'a Item> {
    let term = term.trim().to_lowercase();
    items
        .filter(|i| {
            matches!(
                i.kind.as_str(),
                "Movie" | "Series" | "Episode" | "Video" | "MusicVideo"
            )
        })
        .min_by_key(|i| {
            let name = i.name.trim().to_lowercase();
            let rank = if name == term {
                0
            } else if name.starts_with(&term) {
                1
            } else if name.contains(&term) {
                2
            } else {
                3
            };
            (
                rank,
                !matches!(i.kind.as_str(), "Movie" | "Series"),
                i.position() == 0,
                name.len(),
            )
        })
}

fn remember(terms: &mut Vec<String>, term: &str) -> bool {
    let term = term.trim();
    if term.is_empty() || terms.first().is_some_and(|s| s == term) {
        return false;
    }
    let folded = term.to_lowercase();
    terms.retain(|s| s.to_lowercase() != folded);
    terms.insert(0, term.to_owned());
    terms.truncate(10);
    true
}

impl Library {
    fn save_recent_searches(&mut self) {
        if let Some(server) = &self.server
            && let Err(e) = server.profile_path().and_then(|p| {
                preferences::save(&p.join("recent-searches.json"), &self.recent_searches)
            })
        {
            self.message = format!("Could not save recent searches. {e}");
        }
    }

    pub(in crate::ui) fn remember_search(&mut self) {
        if let Screen::Search(term) = &self.state.screen
            && remember(&mut self.recent_searches, term)
        {
            self.save_recent_searches();
        }
    }

    pub(in crate::ui) fn load_search(&mut self, term: String, w: &Window, cx: &mut Context<Self>) {
        let servers = self.browse_servers();
        self.next_search_request += 1;
        let request = self.next_search_request;
        self.search_target_job = None;
        self.state.search = SearchState {
            request,
            federated: servers.len() > 1,
            pending_servers: servers.len(),
            ..Default::default()
        };
        self.message.clear();
        for server in servers {
            let query = term.clone();
            let worker = cx.background_executor().spawn(async move {
                let responses = std::thread::scope(|scope| {
                    let requests: Vec<_> = SearchGroup::ALL
                        .into_iter()
                        .map(|kind| {
                            let server = &server;
                            let term = &query;
                            (
                                kind,
                                scope.spawn(move || kind.on_server(server, term, 0, SEARCH_BATCH)),
                            )
                        })
                        .collect();
                    requests
                        .into_iter()
                        .map(|(kind, job)| {
                            (
                                kind,
                                job.join().unwrap_or_else(|_| {
                                    Err(anyhow::anyhow!("Search interrupted."))
                                }),
                            )
                        })
                        .collect::<Vec<_>>()
                });
                (server, query, responses)
            });
            self.state
                .search
                .jobs
                .push(cx.spawn_in(w, async move |view, cx| {
                    let (server, query, responses) = worker.await;
                    let _ = view.update_in(cx, |s, w, cx| {
                        if s.state.search.request == request {
                            if s.state.search.receive(server, responses, &query, w, cx) {
                                s.search_target_job = None;
                            }
                            cx.notify();
                        } else if let Some(state) = s
                            .history
                            .iter_mut()
                            .find(|state| state.search.request == request)
                        {
                            state.search.receive(server, responses, &query, w, cx);
                        }
                    });
                }));
        }
        cx.notify();
    }

    pub(in crate::ui) fn load_search_target(&mut self, w: &Window, cx: &mut Context<Self>) {
        if !self.state.search.needs_target || self.search_target_job.is_some() {
            return;
        }
        let Some(show) = self.state.search.best.as_ref() else {
            return;
        };
        if !self.home_dirty
            && self.home.continue_ready(show.origin.as_deref())
            && let Some(ep) = self
                .shelves
                .iter()
                .filter(|s| s.kind == ShelfKind::Continue)
                .flat_map(|s| &s.items)
                .flat_map(|i| std::iter::once(i).chain(&i.copies))
                .find(|ep| ep.origin == show.origin && ep.series_id.as_ref() == Some(&show.id))
        {
            self.state.search.target = Some(ep.clone());
            self.state.search.needs_target = false;
            return;
        }
        let Some(server) = self.source_server(show.origin.as_deref()) else {
            return;
        };
        let show = show.clone();
        let worker = cx.background_executor().spawn(async move {
            let episode = match server.series_target(&show.id)? {
                Some(ep) => Some(ep),
                None => {
                    let seasons = server.seasons(&show.id)?;
                    match seasons
                        .iter()
                        .find(|s| s.index_number != Some(0))
                        .or(seasons.first())
                    {
                        Some(season) => server
                            .children(&show, Some(&season.id), 0)?
                            .items
                            .into_iter()
                            .find(Item::playable),
                        None => None,
                    }
                }
            };
            anyhow::Ok(episode.map(|mut ep| {
                ep.origin = show.origin;
                ep
            }))
        });
        self.search_target_job = Some(cx.spawn_in(w, async move |view, cx| {
            let result = worker.await;
            let _ = view.update(cx, |s, cx| {
                s.search_target_job = None;
                s.state.search.needs_target = false;
                match result {
                    Ok(ep) => s.state.search.target = ep,
                    Err(e) => s.message = format!("Could not load the episode to play. {e}"),
                }
                cx.notify();
            });
        }));
    }
    fn search_page(&mut self, index: usize, start: usize, w: &mut Window, cx: &mut Context<Self>) {
        let Screen::Search(term) = self.state.screen.clone() else {
            return;
        };
        let Some(group) = self.state.search.groups.get_mut(index) else {
            return;
        };
        let kind = group.kind;
        let needed = (start + search_grid(kind, w).columns).min(group.page.total_record_count);
        let loaded = group.start + group.page.items.len();
        if start >= group.start && start < loaded && needed <= loaded {
            group.first = start - group.start;
            group.handles[group.first].focus(w);
            cx.notify();
            return;
        }
        let server = group.server.clone();
        self.work(
            w,
            cx,
            move || {
                let page = kind.on_server(&server, &term, start, SEARCH_BATCH)?;
                if start > 0 && page.items.is_empty() && page.total_record_count > 0 {
                    let last = page.total_record_count.saturating_sub(SEARCH_BATCH);
                    let page = kind.on_server(&server, &term, last, SEARCH_BATCH)?;
                    return Ok((last, page));
                }
                Ok((start, page))
            },
            move |s, (start, page), w, cx| {
                if page.items.is_empty() {
                    s.state.search.groups.remove(index);
                    s.search.read(cx).focus_handle(cx).focus(w);
                    return Ok(());
                }
                let server = s.state.search.groups[index].server.clone();
                let group = ResultGroup::new(kind, server, page, start, cx);
                if let Some(first) = group.handles.first() {
                    first.focus(w);
                }
                s.state.search.groups[index] = group;
                Ok(())
            },
        );
    }

    fn search_action(
        &mut self,
        item: Item,
        play: bool,
        handle: &FocusHandle,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.search.return_focus = Some(handle.clone());
        self.remember_search();
        if play {
            self.play_item(item, w, cx);
        } else {
            self.open_item(item, w, cx);
        }
    }

    pub(in crate::ui) fn search_key(
        &mut self,
        key: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !matches!(key, "left" | "right" | "up" | "down" | "enter") {
            return false;
        }
        let typing = self.search.read(cx).focus_handle(cx).is_focused(w);
        if typing && key != "down" {
            return false;
        }
        if matches!(key, "left" | "right") {
            for (index, group) in self.state.search.groups.iter().enumerate() {
                let columns = search_grid(group.kind, w).columns;
                let range = group.visible(columns);
                let Some(cards) = group.handles.get(range.clone()) else {
                    continue;
                };
                let start = group.start + range.start;
                let end = group.start + range.end;
                let page = match key {
                    "right"
                        if end < group.page.total_record_count
                            && cards.last().is_some_and(|f| f.is_focused(w)) =>
                    {
                        end
                    }
                    "left" if start > 0 && cards.first().is_some_and(|f| f.is_focused(w)) => {
                        start.saturating_sub(columns)
                    }
                    _ => continue,
                };
                self.search_page(index, page, w, cx);
                return true;
            }
        }
        let state = &self.state.search;
        // Row numbers correspond to the direct children of the scroll container (including headers).
        let mut rows = Vec::new();
        let mut y = usize::from(self.search_pending()) + usize::from(!state.failures.is_empty());
        if !state.hero.is_empty() {
            rows.push((y, &state.hero[..1 + usize::from(state.target.is_some())]));
            y += 1;
        }
        for group in &state.groups {
            y += 1;
            let range = group.visible(search_grid(group.kind, w).columns);
            if !range.is_empty() {
                rows.push((y, &group.handles[range]));
                y += 1;
            }
        }
        let Some((_, first)) = rows.first() else {
            return false;
        };
        let place = rows
            .iter()
            .enumerate()
            .find_map(|(r, (_, h))| h.iter().position(|f| f.is_focused(w)).map(|c| (r, c)));
        let (r, c) = match place {
            None if typing || self.focus.is_focused(w) => {
                if key == "enter"
                    && self.focus.is_focused(w)
                    && let Some(item) = state.best.clone()
                {
                    let handle = first[0].clone();
                    self.search_action(item, false, &handle, w, cx);
                    return true;
                }
                (0, 0)
            }
            None => return false,
            Some(_) if key == "enter" => return false,
            Some((0, _)) if key == "up" => {
                self.search.read(cx).focus_handle(cx).focus(w);
                return true;
            }
            Some((r, c)) => match key {
                "left" => (r, c.saturating_sub(1)),
                "right" => (r, c + 1),
                "up" => (r - 1, c),
                _ => ((r + 1).min(rows.len() - 1), c),
            },
        };
        let (row, handles) = rows[r];
        handles[c.min(handles.len() - 1)].focus(w);
        self.state.scroll.scroll_to_item(row);
        true
    }
}

impl Library {
    fn search_pending(&self) -> bool {
        self.state.search.pending_servers > 0
            || self.search_delay.is_some()
            || self.state.search.request == 0
    }

    fn best_match(&self, item: &Item, w: &Window, cx: &Context<Self>) -> Div {
        let height = (f32::from(w.viewport_size().height) * 0.22).clamp(122., 178.);
        let width = (height * 16. / 9.).round();
        let state = &self.state.search;
        let target = item.clone();
        let details = state.hero[0].clone();
        let mut art = div()
            .id("best-art")
            .track_focus(&details)
            .tab_stop(true)
            .w(px(width))
            .h(px(height))
            .flex_none()
            .overflow_hidden()
            .relative()
            .rounded_lg()
            .cursor_pointer()
            .bg(rgb(SURFACE))
            .flex()
            .items_center()
            .justify_center();
        let image = self.artwork.get(item.landscape());
        art = if let Some(image) = image {
            art.child(img(image).size_full().object_fit(ObjectFit::Cover))
        } else {
            art.child(icon(VIDEO, 30.).text_color(rgb(MUTED)))
        };
        let border = if details.is_focused(w) {
            rgb(LIME)
        } else {
            rgba(0)
        };
        let art = art
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_lg()
                    .border_2()
                    .border_color(border),
            )
            .hover(|s| s.border_1().border_color(rgb(TEXT)))
            .tooltip(|w, cx| Tooltip::new("Open details").build(w, cx))
            .on_click(cx.listener(move |s, _, w, cx| {
                s.search_action(target.clone(), false, &details, w, cx)
            }));
        let kind = match item.kind.as_str() {
            "Series" => "TV show",
            "MusicVideo" => "Music video",
            other => other,
        };
        let mut info = vec![kind.to_owned()];
        if let Some(year) = item.production_year {
            info.push(year.to_string());
        }
        if let Some(genre) = item.genres.iter().flatten().next() {
            info.push(genre.clone());
        }
        let mut copy = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .items_start()
            .gap_1()
            .child(label(info.join("  ·  "), 12., MUTED))
            .child(title(item.name.clone(), if height < 140. { 23. } else { 29. }).line_clamp(1));
        if let Some(target) = &state.target {
            let mut context = Vec::new();
            if target.kind == "Episode" && item.kind == "Series" {
                context.push(target.name.clone());
            }
            if let Some(timing) = detail::episode_timing(target).0 {
                context.push(timing);
            }
            if !context.is_empty() {
                copy = copy.child(label(context.join("  ·  "), 14., TEXT).line_clamp(1));
            }
            let label = if target.kind == "Episode" {
                episode_play_label(target)
            } else if target.position() > 0 {
                "Resume".into()
            } else {
                "Play".into()
            };
            let target = target.clone();
            let handle = state.hero[1].clone();
            copy = copy.child(
                accessibility::focus(pill("best-play", label, Some(PLAY), true), &handle)
                    .mt_1()
                    .bg(rgb(LIME))
                    .hover(|s| s.bg(rgb(0xe7ff9a)))
                    .on_click(cx.listener(move |s, _, w, cx| {
                        s.search_action(target.clone(), true, &handle, w, cx)
                    })),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .pb_4()
            .child(section_heading("Best match", 18.))
            .child(div().flex().items_center().gap_4().child(art).child(copy))
    }

    fn search_card(
        &self,
        g: usize,
        index: usize,
        layout: CatalogGrid,
        w: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let group = &self.state.search.groups[g];
        let item = &group.page.items[index];
        let handle = group.handles[index].clone();
        let id: ElementId =
            SharedString::from(format!("search-result-{g}-{}", group.start + index)).into();
        let selected = handle.is_focused(w);
        let width = layout.card_width;
        let person = group.kind == SearchGroup::People;
        let face = if person || matches!(group.kind, SearchGroup::Episodes | SearchGroup::Videos) {
            let key = if person {
                item.art(ArtKind::Poster)
            } else {
                item.landscape()
            };
            let mut pic = div()
                .relative()
                .w_full()
                .h(px(if person { width } else { width * 9. / 16. }))
                .overflow_hidden()
                .map(|d| {
                    if person {
                        d.rounded_full()
                    } else {
                        d.rounded_lg()
                    }
                })
                .bg(rgb(SURFACE))
                .flex()
                .items_center()
                .justify_center();
            if let Some(image) = self.artwork.get(key) {
                pic = pic.child(
                    img(image)
                        .size_full()
                        .object_fit(ObjectFit::Cover)
                        .map(|d| {
                            if person {
                                d.rounded_full()
                            } else {
                                d.rounded_lg()
                            }
                        }),
                );
            } else {
                pic = pic
                    .child(icon(if person { ACCOUNT } else { VIDEO }, 22.).text_color(rgb(MUTED)));
            }
            pic = pic.child(
                div()
                    .absolute()
                    .inset_0()
                    .map(|d| {
                        if person {
                            d.rounded_full()
                        } else {
                            d.rounded_lg()
                        }
                    })
                    .border_2()
                    .border_color(if selected { rgb(LIME) } else { rgba(0) })
                    .group_hover("media-card", |s| s.border_color(rgb(TEXT))),
            );
            if !person {
                pic = pic.children(self.card_format_strip(item, &id, width, cx));
            }
            div()
                .relative()
                .child(pic)
                .child(
                    title(item.name.clone(), 14.)
                        .mt_1()
                        .line_clamp(1)
                        .when(person, |d| d.text_center()),
                )
                .when(item.kind == "Episode", |d| {
                    d.child(label(item.caption(), 12., MUTED).line_clamp(1))
                })
        } else {
            self.catalog_face(item, width, layout.poster, selected, &id, cx)
        };
        let item = item.clone();
        accessibility::title_button(
            card_frame(face, width, selected, id),
            &item,
            Some(handle.clone()),
            selected,
        )
        .track_focus(&handle)
        .tab_stop(true)
        .on_click(
            cx.listener(move |s, _, w, cx| s.search_action(item.clone(), false, &handle, w, cx)),
        )
    }

    pub(super) fn search_view(&self, term: &str, w: &Window, cx: &mut Context<Self>) -> Div {
        let input = accessibility::field(
            Input::new(&self.search).prefix(icon(SEARCH, 16.)),
            &self.search,
            "Search titles and people",
            false,
            cx,
        )
        .max_w(px(520.));
        let header = div()
            .px(px(PAD))
            .py_3()
            .flex()
            .items_center()
            .gap_4()
            .child(page_heading("Search", 24.).flex_none())
            .child(input)
            .child(
                glyph_button("On-screen keyboard", "\u{e765}").on_click(cx.listener(
                    |s, _, w, cx| {
                        if s.search_keyboard.is_some() {
                            s.back(w, cx);
                        } else {
                            s.open_search_keyboard(w, cx);
                        }
                    },
                )),
            );
        let mut body = div()
            .id("search-results")
            .track_scroll(&self.state.scroll)
            .overflow_y_scroll()
            .min_h_0()
            .flex_1()
            .px(px(PAD))
            .pb_4();
        if term.is_empty() {
            if !self.recent_searches.is_empty() {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .py_2()
                        .child(section_heading("Recent searches", 18.))
                        .child(
                            pill("clear-searches", "Clear history", None, false)
                                .h(px(26.))
                                .on_click(cx.listener(|s, _, w, cx| {
                                    s.recent_searches.clear();
                                    s.save_recent_searches();
                                    s.search.read(cx).focus_handle(cx).focus(w);
                                    cx.notify();
                                })),
                        ),
                );
                let mut chips = div().flex().flex_wrap().gap_2();
                for (i, term) in self.recent_searches.iter().enumerate() {
                    let term = term.clone();
                    chips = chips.child(
                        pill(("recent-search", i), term.clone(), Some(SEARCH), false).on_click(
                            cx.listener(move |s, _, w, cx| {
                                s.search
                                    .update(cx, |input, cx| input.set_value(term.clone(), w, cx));
                                s.search.read(cx).focus_handle(cx).focus(w);
                            }),
                        ),
                    );
                }
                body = body.child(chips);
            }
        } else {
            let state = &self.state.search;
            if self.search_pending() {
                let status = if state.groups.is_empty() {
                    "Searching Jellyfin…"
                } else {
                    "Searching more servers…"
                };
                body = body.child(
                    label(status, 14., MUTED)
                        .relative()
                        .py_2()
                        .children(accessibility::status(status)),
                );
            } else if state.groups.is_empty() && state.failures.is_empty() {
                body = body.child(self.empty("No matching titles or people."));
            }
            if !state.failures.is_empty() {
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .py_2()
                        .child(label(state.failures.join(" "), 14., MUTED).flex_1())
                        .child(
                            pill("retry-search", "Retry search", Some(REFRESH), false)
                                .on_click(cx.listener(|s, _, w, cx| s.refresh(w, cx))),
                        ),
                );
            }
            if let Some(best) = &self.state.search.best {
                body = body.child(self.best_match(best, w, cx));
            }
            for (g, group) in self.state.search.groups.iter().enumerate() {
                let layout = search_grid(group.kind, w);
                let mut heading = div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .pt_2()
                    .pb_1()
                    .child(section_heading(group.heading(state.federated), 19.))
                    .child(label(group.page.total_record_count.to_string(), 12., MUTED))
                    .child(div().flex_1());
                let visible = group.visible(layout.columns);
                let start = group.start + visible.start;
                let end = group.start + visible.end;
                for (show, start, glyph, tip_text) in [
                    (
                        start > 0,
                        start.saturating_sub(layout.columns),
                        PREV,
                        "Previous results",
                    ),
                    (
                        end < group.page.total_record_count,
                        end,
                        NEXT,
                        "Next results",
                    ),
                ] {
                    if show {
                        heading = heading.child(
                            tip(
                                div()
                                    .id((tip_text, g))
                                    .size(px(28.))
                                    .rounded_full()
                                    .bg(rgb(SURFACE))
                                    .flex()
                                    .justify_center()
                                    .items_center()
                                    .cursor_pointer()
                                    .child(icon(glyph, 13.)),
                                tip_text,
                            )
                            .on_click(
                                cx.listener(move |s, _, w, cx| s.search_page(g, start, w, cx)),
                            ),
                        );
                    }
                }
                body = body.child(heading);
                let mut line = div().flex().gap(px(CATALOG_GAP)).pb_2();
                for i in visible {
                    line = line.child(self.search_card(g, i, layout, w, cx));
                }
                body = body.child(line);
            }
        }
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .capture_key_down(cx.listener(|s, event: &KeyDownEvent, w, cx| {
                if s.keypad_navigation(&event.keystroke.key, w, cx) {
                    w.prevent_default();
                    cx.stop_propagation();
                }
            }))
            .child(header)
            .children(
                self.search_keyboard
                    .map(|keys| self.search_keypad(keys, cx)),
            )
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::{Item, best_title, remember};

    fn item(id: &str, kind: &str, name: &str) -> Item {
        serde_json::from_value(serde_json::json!({ "Id": id, "Name": name, "Type": kind })).unwrap()
    }

    #[test]
    fn best_match_prefers_an_exact_show_over_an_episode_or_partial_title() {
        let hits = [
            item("episode", "Episode", "Fallout"),
            item("movie", "Movie", "The Fallout"),
            item("show", "Series", "Fallout"),
        ];
        assert_eq!(best_title("  FALLOUT ", hits.iter()).unwrap().id, "show");
        let people = [item("person", "Person", "Fallout")];
        assert!(best_title("fallout", people.iter()).is_none());
    }

    #[test]
    fn recent_searches_keep_the_ten_latest_distinct_queries() {
        let mut terms = Vec::new();
        for number in 0..12 {
            remember(&mut terms, &format!("Title {number}"));
        }
        assert!(remember(&mut terms, "  TITLE 2  "));
        assert_eq!(terms[0], "TITLE 2");
        assert_eq!(terms[1], "Title 11");
        assert_eq!(terms.len(), 10);
        assert!(!remember(&mut terms, "TITLE 2"));
        assert!(!remember(&mut terms, "   "));
    }
}
