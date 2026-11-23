use super::*;
use anyhow::{Context, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;
use std::path::Path;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteSubtitle {
    pub id: String,
    pub name: Option<String>,
    pub provider_name: Option<String>,
    pub format: Option<String>,
}

impl Server {
    pub fn subtitles_online(&self, item: &str, language: &str) -> Result<Vec<RemoteSubtitle>> {
        ensure!(
            language.len() == 3 && language.bytes().all(|c| c.is_ascii_alphabetic()),
            "Enter a three-letter language code such as eng."
        );
        read(self.get(&[
            "Items",
            item,
            "RemoteSearch",
            "Subtitles",
            &language.to_ascii_lowercase(),
        ]))
    }
    pub fn download_subtitle(&self, item: &str, subtitle: &str) -> Result<()> {
        self.post(&["Items", item, "RemoteSearch", "Subtitles", subtitle])
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.without_url())?;
        Ok(())
    }
    pub fn can_edit(&self) -> Result<()> {
        let user: Value = read(self.get(&["Users", "Me"]))?;
        ensure!(
            user.pointer("/Policy/IsAdministrator")
                .and_then(Value::as_bool)
                == Some(true),
            "Editing shared library metadata requires a Jellyfin administrator account."
        );
        Ok(())
    }
    pub fn edit_text(&self, before: &Item, title: &str, overview: &str) -> Result<()> {
        ensure!(!title.trim().is_empty(), "Enter a title.");
        // This endpoint replaces the full DTO. Preserve the fresh server-provided fields
        // and check the two editable values against the version visible when editing began.
        let mut dto: Value = read(
            self.get(&["Items", &before.id])
                .query(&[("userId", self.user_id()?)]),
        )?;
        let current = dto.as_object_mut().context("Invalid Jellyfin item")?;
        ensure!(
            current.get("Name").and_then(Value::as_str) == Some(&before.name)
                && current
                    .get("Overview")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    == before.overview.as_deref().unwrap_or_default(),
            "The title or overview changed on the server. Reopen the editor to load the latest version."
        );
        current.insert("Name".into(), json!(title.trim()));
        current.insert("Overview".into(), json!(overview));
        self.post(&["Items", &before.id])
            .json(&dto)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.without_url())?;
        Ok(())
    }
    pub fn upload_art(&self, item: &str, kind: &str, file: &Path) -> Result<()> {
        use std::io::Read;
        let mut image = std::fs::File::open(file)?;
        ensure!(
            image.metadata()?.len() <= 32 * 1024 * 1024,
            "Choose an image under 32 MiB."
        );
        let mut bytes = Vec::new();
        image.read_to_end(&mut bytes)?;
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            "image/jpeg"
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            anyhow::bail!("Choose a JPEG, PNG or WebP image.")
        };
        self.post(&["Items", item, "Images", kind])
            .header("Content-Type", mime)
            .body(STANDARD.encode(bytes))
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.without_url())?;
        Ok(())
    }
}
