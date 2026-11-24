use super::*;
use std::{collections::HashSet, path::PathBuf};

pub(super) const SHORTCUTS: &[(&str, &str, &str)] = &[
    ("pause", "Play or pause", "space"),
    ("back", "Seek back 10 seconds", "left"),
    ("forward", "Seek forward 10 seconds", "right"),
    ("mute", "Mute or unmute", "m"),
    ("louder", "Volume up", "up"),
    ("quieter", "Volume down", "down"),
    ("fullscreen", "Toggle fullscreen", "f"),
    ("zoom-in", "Zoom in", "]"),
    ("zoom-out", "Zoom out", "["),
    ("options", "Playback options", "o"),
    ("episodes", "Browse episodes", "e"),
    ("preview", "Preview seek position", "t"),
];

impl Preferences {
    pub(super) fn binding(&self, id: &str, default: &str) -> String {
        self.shortcuts
            .get(id)
            .cloned()
            .unwrap_or_else(|| default.to_owned())
    }
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            (16..=72).contains(&self.subtitle_size),
            "Subtitle size must be between 16 and 72."
        );
        let mut used = HashSet::new();
        for &(id, _, default) in SHORTCUTS {
            let key = self.binding(id, default);
            anyhow::ensure!(
                gpui::Keystroke::parse(&key).is_ok()
                    && !matches!(
                        key.as_str(),
                        "escape" | "backspace" | "tab" | "enter" | "f11"
                    )
                    && used.insert(key),
                "The settings contain an invalid, reserved or duplicate keyboard shortcut."
            );
        }
        Ok(())
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Backup {
    version: u8,
    preferences: Preferences,
}

impl Library {
    pub(super) fn capture_shortcut(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        let Some(action) = self.rebind else {
            return false;
        };
        if key.key == "escape" {
            self.rebind = None;
        } else {
            let mut prefs = self.prefs.clone();
            prefs.shortcuts.insert(action.to_owned(), key.unparse());
            match prefs.validate() {
                Ok(()) => {
                    self.prefs = prefs;
                    self.rebind = None;
                    self.message.clear();
                    self.save_preferences(cx);
                }
                Err(e) => self.message = e.to_string(),
            }
        }
        cx.stop_propagation();
        cx.notify();
        true
    }

    pub(super) fn read_display(&mut self, w: &Window, cx: &mut Context<Self>) {
        self.hdr = crate::player::parent(w)
            .and_then(crate::player::HdrSettings::read)
            .ok();
        self.busy = false;
        cx.notify();
    }
    pub(super) fn change_hdr(&mut self, w: &Window, cx: &mut Context<Self>) {
        let Some(settings) = &self.hdr else { return };
        let result = settings.set(!settings.enabled);
        if let Err(e) = result {
            self.message = e.to_string();
        } else {
            self.message.clear();
            self.read_display(w, cx);
        }
        cx.notify();
    }
    pub(super) fn settings_file(&mut self, export: bool, w: &mut Window, cx: &mut Context<Self>) {
        let server = self
            .server
            .as_ref()
            .and_then(Server::identity)
            .map(|a| a.key());
        let settings = Preferences {
            discord_app: None,
            anime_sync: false,
            ..self.prefs.clone()
        };
        let save = export.then(|| {
            cx.prompt_for_new_path(
                &PathBuf::from(std::env::var_os("USERPROFILE").unwrap_or_default()),
                Some("rustyfin-settings.json"),
            )
        });
        let open = (!export).then(|| {
            cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some("Import RustyFin settings".into()),
            })
        });
        cx.spawn_in(w, async move |view, cx| {
            let result: Result<Option<Preferences>> = async {
                if let Some(save) = save {
                    if let Some(path) = save.await?? {
                        preferences::save(
                            &path,
                            &Backup {
                                version: 1,
                                preferences: settings,
                            },
                        )?;
                    }
                    return Ok(None);
                }
                let Some(path) = open.unwrap().await??.and_then(|v| v.into_iter().next()) else {
                    return Ok(None);
                };
                let file = std::fs::File::open(path)?;
                anyhow::ensure!(
                    file.metadata()?.len() <= 1024 * 1024,
                    "The settings file is too large."
                );
                let data: Backup = serde_json::from_reader(file)?;
                anyhow::ensure!(data.version == 1, "Unsupported settings backup version.");
                data.preferences.validate()?;
                Ok(Some(data.preferences))
            }
            .await;
            let _ = view.update_in(cx, |s, _, cx| {
                if s.server
                    .as_ref()
                    .and_then(Server::identity)
                    .map(|a| a.key())
                    != server
                {
                    return;
                }
                match result {
                    Ok(Some(mut prefs)) => {
                        prefs.discord_app.clone_from(&s.prefs.discord_app);
                        prefs.anime_sync = s.prefs.anime_sync;
                        s.rotate = prefs.rotate;
                        s.prefs = prefs;
                        s.home_dirty = true;
                        s.save_preferences(cx);
                    }
                    Ok(None) => {}
                    Err(e) => s.message = format!("Settings file error. {e:#}"),
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::Preferences;
    #[test]
    fn older_settings_fill_defaults_and_ambiguous_shortcuts_are_rejected() {
        let mut settings: Preferences = serde_json::from_str(r#"{"rotate":false}"#).unwrap();
        assert!(!settings.rotate && settings.autoplay && settings.passthrough);
        assert!(settings.mel_to_81 && settings.windows_dolby);
        assert_eq!(settings.poster_size, crate::preferences::PosterSize::Medium);
        settings.poster_size = crate::preferences::PosterSize::Large;
        settings.mel_to_81 = false;
        settings.windows_dolby = false;
        let saved: Preferences =
            serde_json::from_value(serde_json::to_value(&settings).unwrap()).unwrap();
        assert!(!saved.mel_to_81 && !saved.windows_dolby);
        assert_eq!(saved.poster_size, crate::preferences::PosterSize::Large);
        assert!(settings.validate().is_ok());
        settings.shortcuts.insert("pause".into(), "left".into());
        assert!(settings.validate().is_err());
        settings.shortcuts.insert("pause".into(), "ctrl-p".into());
        assert!(settings.validate().is_ok());
    }
}
