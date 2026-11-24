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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Shader {
    #[default]
    Off,
    Sharpen,
    Soften,
    ArtCnn16,
    ArtCnn32,
    ArtCnn16Ds,
    Fsrcnnx8,
    Fsrcnnx16,
    RavuLite,
    RavuZoom,
    Nnedi3,
    SsimSuperRes,
    RtxVsr,
    Anime4k,
    Custom(PathBuf),
}

impl Shader {
    pub const PRESETS: &[Self] = &[
        Self::Off,
        Self::Fsrcnnx8,
        Self::Fsrcnnx16,
        Self::RavuLite,
        Self::RavuZoom,
        Self::Nnedi3,
        Self::SsimSuperRes,
        Self::ArtCnn16,
        Self::ArtCnn32,
        Self::ArtCnn16Ds,
        Self::Anime4k,
        Self::Sharpen,
        Self::Soften,
    ];

    pub fn label(&self) -> &str {
        match self {
            Self::Off => "Off",
            Self::Sharpen => "Sharpen",
            Self::Soften => "Soften",
            Self::ArtCnn16 => "ArtCNN C4F16",
            Self::ArtCnn32 => "ArtCNN C4F32",
            Self::ArtCnn16Ds => "ArtCNN C4F16 DS",
            Self::Fsrcnnx8 => "FSRCNNX 8",
            Self::Fsrcnnx16 => "FSRCNNX 16",
            Self::RavuLite => "RAVU Lite AR",
            Self::RavuZoom => "RAVU Zoom AR r3",
            Self::Nnedi3 => "NNEDI3",
            Self::SsimSuperRes => "SSimSuperRes",
            Self::RtxVsr => "RTX Video Super Resolution",
            Self::Anime4k => "Anime4K ×2",
            Self::Custom(path) => path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Custom shader"),
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Off => "Standard mpv scaling",
            Self::Sharpen | Self::Soften => "All output sizes",
            Self::Fsrcnnx8 => "Live action · Balanced",
            Self::Fsrcnnx16 => "Live action · Higher GPU cost",
            Self::RavuLite => "Live action · Anti-ringing",
            Self::RavuZoom => "Live action · Fractional scaling",
            Self::Nnedi3 => "Edge-directed · 64 neurons",
            Self::SsimSuperRes => "Sharpen and reduce ringing",
            Self::RtxVsr => "NVIDIA video processing",
            Self::ArtCnn16 => "Animation · Balanced",
            Self::ArtCnn32 => "Animation · Higher GPU cost",
            Self::ArtCnn16Ds => "Denoise and sharpen",
            Self::Anime4k => "Animation · CNN upscaling",
            Self::Custom(_) => "Custom file",
        }
    }

    pub fn disabled_reason(&self) -> Option<&'static str> {
        match self {
            Self::ArtCnn32 => {
                Some("Temporarily disabled because shader compilation can stall playback.")
            }
            _ => None,
        }
    }

    pub fn asset(&self) -> Option<&'static str> {
        Some(match self {
            Self::Off | Self::RtxVsr | Self::Custom(_) => return None,
            Self::Sharpen => "sharpen.glsl",
            Self::Soften => "soften.glsl",
            Self::ArtCnn16 => "ArtCNN_C4F16.glsl",
            Self::ArtCnn32 => "ArtCNN_C4F32.glsl",
            Self::ArtCnn16Ds => "ArtCNN_C4F16_DS.glsl",
            Self::Fsrcnnx8 => "FSRCNNX_x2_8-0-4-1.glsl",
            Self::Fsrcnnx16 => "FSRCNNX_x2_16-0-4-1.glsl",
            Self::RavuLite => "ravu-lite-ar-r3.hook",
            Self::RavuZoom => "ravu-zoom-ar-r3.hook",
            Self::Nnedi3 => "nnedi3-nns64-win8x4.hook",
            Self::SsimSuperRes => "SSimSuperRes.glsl",
            Self::Anime4k => "Anime4K_Upscale_CNN_x2_M.glsl",
        })
    }
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
    pub custom_shaders: Vec<PathBuf>,
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
    pub performance_overlay: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub anime_sync: bool,
    pub subtitle_size: u32,
    pub passthrough: bool,
    pub windows_dolby: bool,
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
            performance_overlay: false,
            anime_sync: false,
            subtitle_size: 36,
            passthrough: true,
            windows_dolby: true,
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
pub fn shader_cache_path() -> Result<PathBuf> {
    let path = directory()?.join("mpv-shader-cache");
    fs::create_dir_all(&path)?;
    Ok(path)
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
