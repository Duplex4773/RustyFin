use super::*;
use crate::jellyfin::edit::RemoteSubtitle;

pub(super) struct Editor {
    pub item: Item,
    pub title: Entity<InputState>,
    pub overview: Entity<InputState>,
}
pub(super) struct SubtitleSearch {
    pub language: Entity<InputState>,
    pub rows: Option<Vec<RemoteSubtitle>>,
}
impl SubtitleSearch {
    pub fn new(w: &mut Window, cx: &mut Context<Library>) -> Self {
        let language = cx.new(|cx| {
            InputState::new(w, cx)
                .placeholder("Language code")
                .default_value("eng")
        });
        Self {
            language,
            rows: None,
        }
    }
}
impl Library {
    pub(super) fn open_editor(&mut self, item: Item, w: &mut Window, cx: &mut Context<Self>) {
        let Some(server) = self.source_server(item.origin.as_deref()) else {
            return;
        };
        self.work(
            w,
            cx,
            move || {
                server.can_edit()?;
                server.item(&item.id)
            },
            |s, item, w, cx| {
                s.state.editor = Some(Editor {
                    title: cx.new(|cx| InputState::new(w, cx).default_value(item.name.clone())),
                    overview: cx.new(|cx| {
                        InputState::new(w, cx)
                            .multi_line(true)
                            .default_value(item.overview.clone().unwrap_or_default())
                    }),
                    item,
                });
                Ok(())
            },
        );
    }
    pub(super) fn save_editor(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let (Some(editor), Some(server)) = (&self.state.editor, self.detail_server()) else {
            return;
        };
        if self.busy {
            return;
        }
        let title = editor.title.read(cx).value().to_string();
        let overview = editor.overview.read(cx).value().to_string();
        let item = editor.item.clone();
        self.work(
            w,
            cx,
            move || server.edit_text(&item, &title, &overview),
            Self::refresh_edited,
        );
    }
    fn refresh_edited(&mut self, _: (), w: &mut Window, cx: &mut Context<Self>) -> Result<()> {
        self.state.editor = None;
        self.state.subtitle_search = None;
        self.home_dirty = true;
        self.artwork.clear(w, cx);
        self.load(w, cx);
        Ok(())
    }
    pub(super) fn pick_art(&mut self, kind: &'static str, w: &mut Window, cx: &mut Context<Self>) {
        let (Some(editor), Some(server)) = (&self.state.editor, self.detail_server()) else {
            return;
        };
        let item = editor.item.id.clone();
        let selected = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Replace Jellyfin artwork".into()),
        });
        cx.spawn_in(w, async move |view, cx| {
            let path = selected.await;
            let _ = view.update_in(cx, |s, w, cx| {
                if s.state.editor.as_ref().is_none_or(|e| e.item.id != item)
                    || s.detail_server()
                        .as_ref()
                        .and_then(Server::identity)
                        .map(|a| a.key())
                        != server.identity().map(|a| a.key())
                {
                    return;
                }
                match path {
                    Ok(Ok(Some(paths))) if !paths.is_empty() => {
                        let file = paths[0].clone();
                        s.work(
                            w,
                            cx,
                            move || server.upload_art(&item, kind, &file),
                            |s, _, w, cx| {
                                s.artwork.clear(w, cx);
                                s.home_dirty = true;
                                s.load(w, cx);
                                Ok(())
                            },
                        );
                    }
                    Ok(Err(e)) => {
                        s.message = e.to_string();
                        cx.notify();
                    }
                    _ => {}
                }
            });
        })
        .detach();
    }
    pub(super) fn search_subtitles(&mut self, id: String, w: &mut Window, cx: &mut Context<Self>) {
        let (Some(search), Some(server)) = (&self.state.subtitle_search, self.detail_server())
        else {
            return;
        };
        let language = search.language.read(cx).value().trim().to_owned();
        self.work(
            w,
            cx,
            move || server.subtitles_online(&id, &language),
            |s, rows, _, _| {
                if let Some(search) = &mut s.state.subtitle_search {
                    search.rows = Some(rows);
                }
                Ok(())
            },
        );
    }
    pub(super) fn install_subtitle(
        &mut self,
        item: String,
        id: String,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(server) = self.detail_server() else {
            return;
        };
        if !self.busy {
            self.work(
                w,
                cx,
                move || server.download_subtitle(&item, &id),
                Self::refresh_edited,
            );
        }
    }
}
