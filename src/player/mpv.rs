use std::{
    env,
    ffi::{CStr, CString, c_char, c_int, c_void},
    io::Read,
    path::Path,
    ptr::{self, NonNull},
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use libloading::Library;
use windows::Win32::Foundation::HWND;

use super::Status;
use crate::preferences::{Channels, MpvSettings, Shader, SubtitleStyle};

mod rtx;
use rtx::RtxVsr;

type Handle = *mut c_void;
type SetOption = unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int;
type GetProperty = unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int;
type GetPropertyAsync = unsafe extern "C" fn(Handle, u64, *const c_char, c_int) -> c_int;
type Command = unsafe extern "C" fn(Handle, *const *const c_char) -> c_int;
type SetProperty = unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int;
type RequestLog = unsafe extern "C" fn(Handle, *const c_char) -> c_int;

// Layouts and identifiers come from libmpv's public client API v2 (mpv/client.h).
#[repr(C)]
struct Event {
    id: c_int,
    error: c_int,
    reply: u64,
    data: *mut c_void,
}
#[repr(C)]
struct EndFile {
    reason: c_int,
    error: c_int,
}

#[repr(C)]
union NodeValue {
    string: *const c_char,
    list: *mut NodeList,
}

#[repr(C)]
struct Node {
    value: NodeValue,
    format: c_int,
}

#[repr(C)]
struct NodeList {
    count: c_int,
    values: *mut Node,
    keys: *mut *mut c_char,
}

#[repr(C)]
struct LogMessage {
    prefix: *const c_char,
    level: *const c_char,
    text: *const c_char,
    log_level: c_int,
}

#[repr(C)]
struct EventProperty {
    name: *const c_char,
    format: c_int,
    data: *const c_void,
}

const FORMAT_STRING: c_int = 1;
const FORMAT_NODE: c_int = 6;
const FORMAT_NODE_ARRAY: c_int = 7;
const SHADER_PROBE_REPLY: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ShaderStatus {
    Off,
    Checking,
    Running {
        passes: usize,
        milliseconds: Option<f64>,
    },
    BelowScale {
        scale: f64,
        required: f64,
    },
    NoPasses,
    Unavailable,
    Failed,
    RtxRunning {
        scale: f64,
        driver_accepted: bool,
    },
    RtxAtUhd,
    RtxUnavailable(&'static str),
}

#[derive(serde::Deserialize)]
struct RenderPasses<'a> {
    #[serde(borrow)]
    fresh: Vec<RenderPass<'a>>,
    #[serde(borrow)]
    redraw: Vec<RenderPass<'a>>,
}

#[derive(serde::Deserialize)]
struct RenderPass<'a> {
    desc: std::borrow::Cow<'a, str>,
    last: u64,
}

impl RenderPasses<'_> {
    fn shader_status(&self, shader: &Shader, names: &[String]) -> Option<ShaderStatus> {
        for passes in [&self.fresh, &self.redraw] {
            // The 16-feature FSRCNNX shader shares many descriptions with the
            // 8-feature variant. Ignore the previous preset's cached frame.
            if matches!(shader, Shader::Fsrcnnx8 | Shader::Fsrcnnx16)
                && passes.iter().any(|p| p.desc == "feature map 3")
                    != (*shader == Shader::Fsrcnnx16)
            {
                continue;
            }
            let mut count = 0;
            let mut nanoseconds = 0f64;
            let mut has_timings = true;
            for pass in passes {
                if names.iter().any(|name| pass.desc == name.as_str()) {
                    count += 1;
                    nanoseconds += pass.last as f64;
                    has_timings &= pass.last > 0;
                }
            }
            if count > 0 {
                return Some(ShaderStatus::Running {
                    passes: count,
                    milliseconds: has_timings.then_some(nanoseconds / 1_000_000.),
                });
            }
        }
        None
    }
}

fn shader_pass_names(path: &Path) -> Result<Vec<String>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(32 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 32 * 1024 * 1024,
        "The shader file exceeds 32 MB."
    );
    Ok(bytes
        .split(|&byte| byte == b'\n')
        .filter_map(|line| line.strip_prefix(b"//!DESC"))
        .map(|name| String::from_utf8_lossy(name).trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect())
}

fn minimum_scale(shader: &Shader) -> Option<f64> {
    Some(match shader {
        Shader::Fsrcnnx8
        | Shader::Fsrcnnx16
        | Shader::ArtCnn16
        | Shader::ArtCnn32
        | Shader::ArtCnn16Ds => 1.3,
        Shader::Anime4k => 1.2,
        Shader::RavuLite | Shader::Nnedi3 => 1. / 0.707106,
        Shader::RavuZoom | Shader::SsimSuperRes => 1.,
        _ => return None,
    })
}

pub struct Mpv {
    handle: NonNull<c_void>,
    destroy: unsafe extern "C" fn(Handle),
    command: Command,
    property: GetProperty,
    property_async: GetPropertyAsync,
    set_property: SetProperty,
    request_log: RequestLog,
    free: unsafe extern "C" fn(*mut c_void),
    event: unsafe extern "C" fn(Handle, f64) -> *const Event,
    loaded: bool,
    passthrough_enabled: bool,
    pcm_output: bool,
    pcm_filters: Option<(bool, bool)>,
    pub shader: Shader,
    pub shader_error: Option<String>,
    pub shader_status: ShaderStatus,
    shader_pass_names: Vec<String>,
    shader_selected_at: Instant,
    shader_probe_at: Instant,
    shader_probe_pending: bool,
    rtx: RtxVsr,
    selection: Option<(Option<i32>, Option<i32>, Option<url::Url>)>,
    _library: Library,
}

impl Mpv {
    pub fn open(
        hwnd: HWND,
        request: &crate::jellyfin::PlayRequest,
        settings: &MpvSettings,
    ) -> Result<Self> {
        let dir = env::current_exe()?
            .parent()
            .context("Missing executable directory")?
            .to_owned();
        let path = ["libmpv-2.dll", "mpv-2.dll"].iter().map(|name| dir.join(name)).find(|p| p.is_file())
            .context("Place the x64 libmpv DLL beside rustyfin.exe. The project’s scripts/Get-Mpv.ps1 installs it for a debug build.")?;
        unsafe {
            let lib = Library::new(path).context("Opening libmpv")?;
            let version = lib
                .get::<unsafe extern "C" fn() -> std::ffi::c_ulong>(b"mpv_client_api_version\0")?(
            );
            ensure!(
                version >> 16 == 2,
                "RustyFin needs libmpv client API version 2."
            );
            let initialize =
                *lib.get::<unsafe extern "C" fn(Handle) -> c_int>(b"mpv_initialize\0")?;
            let set = *lib.get::<SetOption>(b"mpv_set_option_string\0")?;
            let destroy = *lib.get(b"mpv_terminate_destroy\0")?;
            let command = *lib.get(b"mpv_command\0")?;
            let free = *lib.get(b"mpv_free\0")?;
            let property = *lib.get(b"mpv_get_property\0")?;
            let property_async = *lib.get(b"mpv_get_property_async\0")?;
            let set_property = *lib.get(b"mpv_set_property\0")?;
            let request_log = *lib.get::<RequestLog>(b"mpv_request_log_messages\0")?;
            let event = *lib.get(b"mpv_wait_event\0")?;
            let create = *lib.get::<unsafe extern "C" fn() -> Handle>(b"mpv_create\0")?;
            let handle = NonNull::new(create()).context("Creating an mpv player")?;
            let mut player = Self {
                handle,
                destroy,
                command,
                property,
                property_async,
                set_property,
                request_log,
                free,
                event,
                loaded: false,
                passthrough_enabled: request.allow_passthrough,
                pcm_output: false,
                pcm_filters: Some((
                    settings.normalize,
                    settings.center && settings.channels == Channels::Stereo,
                ))
                .filter(|(normalize, center)| *normalize || *center),
                shader: Shader::Off,
                shader_error: None,
                shader_status: ShaderStatus::Off,
                shader_pass_names: Vec::new(),
                shader_selected_at: Instant::now(),
                shader_probe_at: Instant::now(),
                shader_probe_pending: false,
                rtx: RtxVsr::default(),
                selection: Some((
                    request.audio_index,
                    request.subtitle_index,
                    request.external_subtitle.clone(),
                )),
                _library: lib,
            };
            for (key, value) in [
                ("wid", (hwnd.0 as usize).to_string()),
                ("hwdec", "auto-safe".into()),
                ("vo", "gpu-next".into()),
                ("vid", if request.audio_only { "no" } else { "auto" }.into()),
                ("ao", "wasapi".into()),
                (
                    "audio-channels",
                    match settings.channels {
                        Channels::System => "auto-safe",
                        Channels::Stereo => "stereo",
                        Channels::Surround => "5.1,5.1(side),stereo",
                    }
                    .into(),
                ),
                (
                    "audio-spdif",
                    if request.allow_passthrough {
                        "ac3,eac3,dts,dts-hd,truehd"
                    } else {
                        ""
                    }
                    .into(),
                ),
                ("start", request.resume.to_string()),
                (
                    "pause",
                    if request.start_paused { "yes" } else { "no" }.into(),
                ),
                ("sub-font-size", request.subtitle_size.to_string()),
                ("config", "no".into()),
                ("terminal", "no".into()),
                ("input-default-bindings", "no".into()),
            ] {
                check(set(
                    handle.as_ptr(),
                    CString::new(key)?.as_ptr(),
                    CString::new(value)?.as_ptr(),
                ))
                .with_context(|| format!("Setting mpv option {key}"))?;
            }
            if !request.audio_only {
                if let Ok(cache) = crate::preferences::shader_cache_path()
                    && let Some(cache) = cache.to_str()
                {
                    let cache = CString::new(cache.replace('\\', "/"))?;
                    check(set(
                        handle.as_ptr(),
                        c"gpu-shader-cache-dir".as_ptr(),
                        cache.as_ptr(),
                    ))
                    .context("Setting the mpv shader cache directory")?;
                }
                let base_layer = matches!(
                    request.route,
                    crate::jellyfin::Route::FelHdr10 | crate::jellyfin::Route::MelHdr10
                );
                // Let gpu-next negotiate the output with the display that owns wid.
                // PQ's reference black is zero even when DXGI reports a higher floor.
                for (key, value) in [
                    (c"gpu-api", c"auto"),
                    (c"gpu-shader-cache", c"yes"),
                    (
                        c"target-colorspace-hint",
                        if base_layer { c"yes" } else { c"auto" },
                    ),
                    (c"target-contrast", c"inf"),
                    (c"tone-mapping", c"auto"),
                    (c"hdr-compute-peak", c"auto"),
                ] {
                    check(set(handle.as_ptr(), key.as_ptr(), value.as_ptr()))
                        .with_context(|| format!("Setting mpv option {}", key.to_string_lossy()))?;
                }
                if base_layer {
                    for (key, value) in [
                        (c"vf", c"format=dolbyvision=no"),
                        (c"target-colorspace-hint-mode", c"source"),
                    ] {
                        check(set(handle.as_ptr(), key.as_ptr(), value.as_ptr()))
                            .context("Configuring HDR10 base-layer playback")?;
                    }
                }
            }
            if !matches!(settings.subtitle, SubtitleStyle::Authored) {
                let color = if settings.subtitle == SubtitleStyle::YellowOutline {
                    c"#ffff00"
                } else {
                    c"#ffffff"
                };
                for (name, value) in [
                    (c"sub-ass-override", c"force"),
                    (c"sub-color", color),
                    (c"sub-border-size", c"3"),
                    (c"sub-border-color", c"#000000"),
                ] {
                    check(set(handle.as_ptr(), name.as_ptr(), value.as_ptr()))?;
                }
            }
            check(initialize(handle.as_ptr())).context("Starting mpv")?;
            if !request.audio_only {
                check(request_log(handle.as_ptr(), c"v".as_ptr()))?;
                if settings.shader == Shader::RtxVsr {
                    // The GPU is only known after mpv initializes the renderer.
                    player.shader = Shader::RtxVsr;
                    player.shader_status = ShaderStatus::Checking;
                } else if settings.shader != Shader::Off
                    && settings.shader.disabled_reason().is_none()
                    && let Err(error) = player.set_shader(settings.shader.clone())
                {
                    player.shader = settings.shader.clone();
                    player.shader_status = ShaderStatus::Failed;
                    player.shader_error = Some(format!("{error:#}"));
                }
            }
            player.run(&["loadfile", request.url.as_str()])?;
            Ok(player)
        }
    }

    pub fn set_shader(&mut self, shader: Shader) -> Result<()> {
        if let Some(reason) = shader.disabled_reason() {
            bail!(reason);
        }
        if shader == Shader::RtxVsr {
            self.rtx_support().map_err(anyhow::Error::msg)?;
            check(unsafe { (self.request_log)(self.handle.as_ptr(), c"v".as_ptr()) })?;
        }
        let path = match &shader {
            Shader::Custom(path) => Some(path.clone()),
            preset => preset
                .asset()
                .map(|file| {
                    Ok::<_, std::io::Error>(
                        env::current_exe()?.with_file_name("shaders").join(file),
                    )
                })
                .transpose()?,
        };
        let (path, names) = match path {
            Some(path) => {
                let names = shader_pass_names(&path)
                    .with_context(|| format!("Could not read shader file {}", path.display()))?;
                let path = CString::new(path.to_str().context("Invalid shader path encoding.")?)?;
                (Some(path), names)
            }
            None => (None, Vec::new()),
        };
        let mut entry = Node {
            value: NodeValue {
                string: path.as_ref().map_or(ptr::null(), |p| p.as_ptr()),
            },
            format: FORMAT_STRING,
        };
        let mut list = NodeList {
            count: c_int::from(path.is_some()),
            values: &raw mut entry,
            keys: ptr::null_mut(),
        };
        let mut node = Node {
            value: NodeValue {
                list: &raw mut list,
            },
            format: FORMAT_NODE_ARRAY,
        };
        if self.shader == Shader::RtxVsr && shader != Shader::RtxVsr {
            self.remove_rtx_filter()?;
            unsafe { (self.request_log)(self.handle.as_ptr(), c"error".as_ptr()) };
        }
        // Set a real list so semicolons and other legal Windows filename characters
        // are passed unchanged. An empty array clears the shaders in one operation.
        check(unsafe {
            (self.set_property)(
                self.handle.as_ptr(),
                c"glsl-shaders".as_ptr(),
                FORMAT_NODE,
                (&raw mut node).cast(),
            )
        })
        .context("Could not change the mpv shader.")?;
        self.shader = shader;
        self.shader_error = None;
        self.shader_status = if self.shader != Shader::Off {
            ShaderStatus::Checking
        } else {
            ShaderStatus::Off
        };
        self.shader_pass_names = names;
        self.shader_selected_at = Instant::now();
        Ok(())
    }

    fn probe_shader(&mut self) {
        if matches!(self.shader, Shader::Off | Shader::RtxVsr)
            || self.shader_status == ShaderStatus::Failed
            || self.shader_probe_pending
            || self.shader_probe_at.elapsed() < Duration::from_millis(550)
            || self.shader_selected_at.elapsed() < Duration::from_millis(300)
        {
            return;
        }
        self.shader_probe_at = Instant::now();
        // Use the asynchronous request so the UI doesn't wait on the video
        // thread for performance counters.
        let result = unsafe {
            (self.property_async)(
                self.handle.as_ptr(),
                SHADER_PROBE_REPLY,
                c"vo-passes".as_ptr(),
                FORMAT_STRING,
            )
        };
        self.shader_probe_pending = result >= 0;
        if result < 0 {
            self.shader_status = ShaderStatus::Unavailable;
        }
    }

    fn shader_probe_reply(&mut self, event: &Event) {
        self.shader_probe_pending = false;
        if self.shader_probe_at < self.shader_selected_at
            || self.shader == Shader::RtxVsr
            || matches!(self.shader_status, ShaderStatus::Off | ShaderStatus::Failed)
        {
            return;
        }
        let passes = if event.error >= 0 && !event.data.is_null() {
            let property = unsafe { &*event.data.cast::<EventProperty>() };
            if property.format == FORMAT_STRING && !property.data.is_null() {
                // mpv owns the string until the next mpv_wait_event call.
                let string = unsafe { *property.data.cast::<*const c_char>() };
                NonNull::new(string.cast_mut()).and_then(|string| {
                    let bytes = unsafe { CStr::from_ptr(string.as_ptr()) }.to_bytes();
                    serde_json::from_slice::<RenderPasses<'_>>(bytes).ok()
                })
            } else {
                None
            }
        } else {
            None
        };
        if let Some(status) = passes
            .as_ref()
            .and_then(|passes| passes.shader_status(&self.shader, &self.shader_pass_names))
        {
            self.shader_status = status;
        } else if self.shader_selected_at.elapsed() >= Duration::from_secs(2) {
            self.shader_status = if let (Some(required), Some(scale)) =
                (minimum_scale(&self.shader), self.render_scale())
                && scale <= required
            {
                ShaderStatus::BelowScale { scale, required }
            } else if passes.is_some() {
                ShaderStatus::NoPasses
            } else {
                ShaderStatus::Unavailable
            };
        }
    }

    fn render_size(&self) -> (f64, f64) {
        let rendered_width = self.number(c"osd-width")
            - self.number(c"osd-dimensions/ml").max(0.)
            - self.number(c"osd-dimensions/mr").max(0.);
        let rendered_height = self.number(c"osd-height")
            - self.number(c"osd-dimensions/mt").max(0.)
            - self.number(c"osd-dimensions/mb").max(0.);
        (rendered_width, rendered_height)
    }

    fn render_scale(&self) -> Option<f64> {
        let mut width = self.number(c"video-out-params/w");
        let mut height = self.number(c"video-out-params/h");
        if self.number(c"video-out-params/rotate") as i32 % 180 != 0 {
            std::mem::swap(&mut width, &mut height);
        }
        let (rendered_width, rendered_height) = self.render_size();
        (width > 0. && height > 0. && rendered_width > 0. && rendered_height > 0.).then(|| {
            let (x, y) = (rendered_width / width, rendered_height / height);
            match self.shader {
                Shader::Nnedi3 => x.max(y),
                Shader::SsimSuperRes => y,
                _ => x.min(y),
            }
        })
    }

    fn run(&self, args: &[&str]) -> Result<()> {
        let args = args
            .iter()
            .map(|v| CString::new(*v))
            .collect::<Result<Vec<_>, _>>()?;
        let mut pointers: Vec<_> = args.iter().map(|s| s.as_ptr()).collect();
        pointers.push(ptr::null());
        check(unsafe { (self.command)(self.handle.as_ptr(), pointers.as_ptr()) })
    }

    fn number(&self, name: &std::ffi::CStr) -> f64 {
        let mut result = 0.0f64;
        unsafe {
            (self.property)(
                self.handle.as_ptr(),
                name.as_ptr(),
                5,
                (&raw mut result).cast(),
            )
        };
        result
    }

    pub fn poll(&mut self, inspect_shader: bool) -> Result<Status> {
        let mut ended = false;
        unsafe {
            loop {
                let e = &*(self.event)(self.handle.as_ptr(), 0.0);
                match e.id {
                    0 => break,
                    1 => ended = true,
                    2 if !e.data.is_null() => {
                        let log = &*e.data.cast::<LogMessage>();
                        let message = std::ffi::CStr::from_ptr(log.text).to_string_lossy();
                        self.rtx_log(
                            CStr::from_ptr(log.prefix).to_bytes(),
                            &message,
                            log.log_level,
                        );
                        if self.shader != Shader::Off
                            && [
                                "Failed to parse user shader",
                                "Failed to load user shader",
                                "Failed compiling shader",
                                "Failed dispatching COMPUTE shader",
                            ]
                            .iter()
                            .any(|error| message.contains(error))
                        {
                            self.shader_status = ShaderStatus::Failed;
                            self.shader_error = Some(
                                "mpv reported a shader error. Choose Off or another shader.".into(),
                            );
                        }
                    }
                    3 if e.reply == SHADER_PROBE_REPLY => self.shader_probe_reply(e),
                    7 if !e.data.is_null() => {
                        let end = &*e.data.cast::<EndFile>();
                        if end.reason == 4 {
                            bail!("mpv could not play this file (error {}).", end.error);
                        }
                        ended = true;
                    }
                    8 => self.loaded = true,
                    _ => {}
                }
            }
        }
        if self.loaded
            && !ended
            && let Some((audio, sub, external)) = self.selection.take()
        {
            if let Some(index) = audio {
                self.select_index("audio", "aid", index)?;
            }
            if let Some(url) = external {
                self.run(&["sub-add", url.as_str(), "select"])?;
            } else if let Some(index) = sub {
                self.select_index("sub", "sid", index)?;
            } else {
                self.run(&["set", "sid", "no"])?;
            }
        }
        if self.loaded && !ended {
            self.update_audio_output()?;
            self.update_rtx();
        }
        let mut paused = 0i32;
        unsafe {
            (self.property)(
                self.handle.as_ptr(),
                c"pause".as_ptr(),
                3,
                (&raw mut paused).cast(),
            )
        };
        let status = Status {
            ready: self.loaded,
            position: self.number(c"time-pos"),
            duration: self.number(c"duration"),
            paused: paused != 0,
            ended,
        };
        if inspect_shader && status.ready && !status.ended {
            self.probe_shader();
        }
        Ok(status)
    }

    fn update_audio_output(&mut self) -> Result<()> {
        let output = self.string_property(c"audio-out-params/format");
        self.pcm_output = output.as_deref().is_some_and(|f| !f.starts_with("spdif"));
        // After WASAPI rejects a bitstream, mpv can switch to a PCM decoder
        // but keep the encoded input and wait indefinitely for an audio clock.
        // Clearing audio-spdif reopens the output at the current playhead.
        if self.passthrough_enabled
            && output.is_none()
            && stale_bitstream_input(
                self.string_property(c"audio-params/format").as_deref(),
                self.string_property(c"current-tracks/audio/decoder")
                    .as_deref(),
            )
        {
            self.run(&["set", "audio-spdif", ""])?;
            self.passthrough_enabled = false;
        }
        if self.pcm_output
            && let Some((normalize, center)) = self.pcm_filters.take()
        {
            if center && self.number(c"audio-params/channel-count") > 2. {
                self.run(&["af","add","lavfi=[pan=stereo|FL<FL+1.4*FC+0.707*BL+0.707*SL|FR<FR+1.4*FC+0.707*BR+0.707*SR]"])?;
            }
            if normalize {
                self.run(&["af", "add", "lavfi=[dynaudnorm=f=150:g=9]"])?;
            }
        }
        Ok(())
    }

    fn string_property(&self, name: &std::ffi::CStr) -> Option<String> {
        let mut out: *mut c_char = ptr::null_mut();
        unsafe {
            let result = (self.property)(
                self.handle.as_ptr(),
                name.as_ptr(),
                1,
                (&raw mut out).cast(),
            );
            if result < 0 || out.is_null() {
                return None;
            }
            let value = std::ffi::CStr::from_ptr(out).to_string_lossy().into_owned();
            (self.free)(out.cast());
            Some(value)
        }
    }

    fn select_index(&self, kind: &str, option: &str, index: i32) -> Result<()> {
        for n in 0..self.number(c"track-list/count") as usize {
            let property =
                |key| self.string_property(&CString::new(format!("track-list/{n}/{key}")).ok()?);
            if property("type").as_deref() == Some(kind)
                && property("ff-index").and_then(|s| s.parse::<i32>().ok()) == Some(index)
                && let Some(id) = property("id")
            {
                return self.run(&["set", option, &id]);
            }
        }
        bail!("mpv could not find the selected {kind} stream {index}.")
    }

    pub fn set_number(&self, name: &str, value: f64) -> Result<()> {
        self.run(&["set", name, &value.to_string()])
    }

    pub fn audio_output(&self) -> Option<String> {
        let format = self.string_property(c"audio-out-params/format")?;
        if format.starts_with("spdif") {
            return Some("Digital bitstream".into());
        }
        let layout = self.string_property(c"audio-out-params/channels");
        Some(layout.map_or_else(|| "PCM".into(), |channels| format!("PCM · {channels}")))
    }
    pub fn has_pcm_output(&self) -> bool {
        self.pcm_output
    }

    pub fn set_volume(&self, volume: f64) -> Result<()> {
        self.run(&["set", "volume", &(volume * 100.).to_string()])
    }

    pub fn toggle(&self) -> Result<()> {
        self.run(&["cycle", "pause"])
    }
    pub fn seek(&self, seconds: f64) -> Result<()> {
        self.run(&["seek", &seconds.to_string(), "relative"])
    }
}

fn stale_bitstream_input(format: Option<&str>, decoder: Option<&str>) -> bool {
    format.is_some_and(|f| f.starts_with("spdif-"))
        && decoder.is_some_and(|d| !d.is_empty() && !d.starts_with("spdif_"))
}

fn check(code: c_int) -> Result<()> {
    ensure!(code >= 0, "libmpv returned {code}");
    Ok(())
}

impl Drop for Mpv {
    fn drop(&mut self) {
        unsafe { (self.destroy)(self.handle.as_ptr()) };
    }
}

#[cfg(test)]
mod tests {
    use super::{RenderPasses, Shader, ShaderStatus, stale_bitstream_input};

    #[test]
    fn verifies_only_passes_for_the_selected_shader() {
        let large: RenderPasses<'_> = serde_json::from_str(
            r#"{"fresh":[{"desc":"feature map 1","last":100000},{"desc":"feature map 3","last":200000}],"redraw":[{"desc":"dithering","last":5000}]}"#,
        )
        .unwrap();
        let names = vec!["feature map 1".into(), "feature map 3".into()];
        assert_eq!(large.shader_status(&Shader::Fsrcnnx8, &names), None);
        assert_eq!(
            large.shader_status(&Shader::Fsrcnnx16, &names),
            Some(ShaderStatus::Running {
                passes: 2,
                milliseconds: Some(0.3)
            })
        );

        let small: RenderPasses<'_> =
            serde_json::from_str(r#"{"fresh":[{"desc":"feature map 1","last":0}],"redraw":[]}"#)
                .unwrap();
        assert_eq!(small.shader_status(&Shader::Fsrcnnx16, &names), None);
        assert_eq!(
            small.shader_status(&Shader::Fsrcnnx8, &names),
            Some(ShaderStatus::Running {
                passes: 1,
                milliseconds: None
            })
        );
    }

    #[test]
    fn distinguishes_pcm_fallback_from_bitstream_startup() {
        assert!(stale_bitstream_input(Some("spdif-eac3"), Some("eac3")));
        assert!(stale_bitstream_input(Some("spdif-dts-hd"), Some("dca")));
        assert!(!stale_bitstream_input(
            Some("spdif-eac3"),
            Some("spdif_eac3")
        ));
        assert!(!stale_bitstream_input(
            Some("spdif-dts-hd"),
            Some("spdif_dts_hd")
        ));
        assert!(!stale_bitstream_input(Some("spdif-eac3"), None));
        assert!(!stale_bitstream_input(None, Some("eac3")));
        assert!(!stale_bitstream_input(Some("floatp"), Some("eac3")));
    }
}
