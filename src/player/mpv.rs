use std::{
    env,
    ffi::{CString, c_char, c_int, c_void},
    ptr::{self, NonNull},
};

use anyhow::{Context, Result, bail, ensure};
use libloading::Library;
use windows::Win32::Foundation::HWND;

use super::Status;
use crate::preferences::{Channels, MpvSettings, Shader, SubtitleStyle};
use std::io::Write;

type Handle = *mut c_void;
type SetOption = unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int;
type GetProperty = unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int;
type Command = unsafe extern "C" fn(Handle, *const *const c_char) -> c_int;

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

pub struct Mpv {
    handle: NonNull<c_void>,
    destroy: unsafe extern "C" fn(Handle),
    command: Command,
    property: GetProperty,
    free: unsafe extern "C" fn(*mut c_void),
    event: unsafe extern "C" fn(Handle, f64) -> *const Event,
    loaded: bool,
    passthrough_enabled: bool,
    pcm_output: bool,
    pcm_filters: Option<(bool, bool)>,
    _shader: Option<tempfile::NamedTempFile>,
    selection: Option<(Option<i32>, Option<i32>, Option<url::Url>)>,
    _library: Library,
}

impl Mpv {
    pub fn open(
        hwnd: HWND,
        request: &crate::jellyfin::PlayRequest,
        settings: &MpvSettings,
    ) -> Result<Self> {
        let shader = match settings.shader {
            Shader::Sharpen if !request.audio_only => Some(include_str!("sharpen.glsl")),
            Shader::Soften if !request.audio_only => Some(include_str!("soften.glsl")),
            _ => None,
        }
        .map(|text| -> Result<_> {
            let mut file = tempfile::Builder::new().suffix(".glsl").tempfile()?;
            file.write_all(text.as_bytes())?;
            file.flush()?;
            Ok(file)
        })
        .transpose()?;
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
            let event = *lib.get(b"mpv_wait_event\0")?;
            let create = *lib.get::<unsafe extern "C" fn() -> Handle>(b"mpv_create\0")?;
            let handle = NonNull::new(create()).context("Creating an mpv player")?;
            let player = Self {
                handle,
                destroy,
                command,
                property,
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
                _shader: shader,
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
            if matches!(
                request.route,
                crate::jellyfin::Route::FelHdr10 | crate::jellyfin::Route::MelHdr10
            ) {
                for (key, value) in [
                    (c"vf", c"format=dolbyvision=no"),
                    (c"target-colorspace-hint", c"yes"),
                    (c"target-colorspace-hint-mode", c"source"),
                ] {
                    check(set(handle.as_ptr(), key.as_ptr(), value.as_ptr()))
                        .context("Configuring HDR10 base-layer playback")?;
                }
            }
            if let Some(shader) = &player._shader {
                check(set(
                    handle.as_ptr(),
                    c"glsl-shaders".as_ptr(),
                    CString::new(shader.path().to_string_lossy().as_bytes())?.as_ptr(),
                ))?;
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
            player.run(&["loadfile", request.url.as_str()])?;
            Ok(player)
        }
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

    pub fn poll(&mut self) -> Result<Status> {
        let mut ended = false;
        unsafe {
            loop {
                let e = &*(self.event)(self.handle.as_ptr(), 0.0);
                match e.id {
                    0 => break,
                    1 => ended = true,
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
        Ok(Status {
            ready: self.loaded,
            position: self.number(c"time-pos"),
            duration: self.number(c"duration"),
            paused: paused != 0,
            ended,
        })
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
    use super::stale_bitstream_input;

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
