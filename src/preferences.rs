use anyhow::{Context, Result};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum Channels {
    #[default]
    System,
    Stereo,
    Surround,
}
#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum Shader {
    #[default]
    Off,
    Sharpen,
    Soften,
}
#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum SubtitleStyle {
    #[default]
    Authored,
    WhiteOutline,
    YellowOutline,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MpvSettings {
    pub channels: Channels,
    pub normalize: bool,
    pub center: bool,
    pub shader: Shader,
    pub subtitle: SubtitleStyle,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub poster_size: PosterSize,
    pub autoplay: bool,
    pub mpv: MpvSettings,
    pub auto_skip: bool,
    pub all_servers: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord_app: Option<String>,
    pub rotate: bool,
    pub match_refresh: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub anime_sync: bool,
    pub subtitle_size: u32,
    pub passthrough: bool,
    pub mel_to_81: bool,
    pub shortcuts: std::collections::BTreeMap<String, String>,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            poster_size: PosterSize::default(),
            mpv: Default::default(),
            auto_skip: false,
            all_servers: false,
            discord_app: None,
            autoplay: true,
            rotate: true,
            match_refresh: false,
            anime_sync: false,
            subtitle_size: 36,
            passthrough: true,
            mel_to_81: true,
            shortcuts: Default::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PosterSize {
    Small,
    #[default]
    Medium,
    Large,
}
fn directory() -> Result<PathBuf> {
    let base = PathBuf::from(
        std::env::var_os("LOCALAPPDATA")
            .context("Windows local application data is unavailable.")?,
    );
    // Existing download records can contain absolute paths under the original directory.
    let legacy = base.join("RustFin");
    Ok(if legacy.is_dir() {
        legacy
    } else {
        base.join("RustyFin")
    })
}
pub fn account_path(server: &str, user: &str) -> Result<PathBuf> {
    let key = Sha256::digest(format!("{server}\0{user}").as_bytes());
    Ok(directory()?.join(format!("{key:x}")))
}
pub fn item_key(id: &str) -> String {
    format!("{:x}", Sha256::digest(id.as_bytes()))
}
pub fn item_path(folder: &Path, id: &str) -> PathBuf {
    folder
        .join("tracks")
        .join(format!("{:x}.json", Sha256::digest(id.as_bytes())))
}
pub fn load<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    match fs::read(path) {
        Ok(data) => Ok(serde_json::from_slice(&data)?),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(e.into()),
    }
}
pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path.parent().context("Invalid preferences location.")?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut temp, value)?;
    temp.flush()?;
    temp.persist(path)?;
    Ok(())
}
