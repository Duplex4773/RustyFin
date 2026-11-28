mod display;
pub use display::{HdrSettings, SdrOutput};
mod local;
mod mel;
pub use mel::LayerError;
mod mpv;
pub use mpv::Metric;
pub use mpv::ShaderStatus;
mod native;
mod performance;
mod refresh;
pub mod transport;
pub use native::OutputLost;

use crate::jellyfin::{PlayRequest, Route, Server, Source};
use crate::preferences::Shader;
use anyhow::{Context, Result, ensure};
use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    sync::mpsc::{self, Sender},
    time::{Duration, Instant},
};
use windows::{
    Win32::{Foundation::HWND, UI::WindowsAndMessaging::*},
    core::w,
};

const CONTROLS_IDLE: Duration = Duration::from_millis(2600);
const CONTROLS_FADE: Duration = Duration::from_millis(180);

struct ControlsVisibility {
    last_activity: Instant,
    fade_start: Option<Instant>,
}

impl ControlsVisibility {
    fn wake(&mut self, now: Instant) {
        self.last_activity = now;
        self.fade_start = None;
    }

    fn opacity(&mut self, now: Instant, pinned: bool, animate: bool) -> f32 {
        if pinned {
            self.wake(now);
        }
        if now.duration_since(self.last_activity) < CONTROLS_IDLE {
            return 1.;
        }
        if !animate {
            return 0.;
        }
        // Playback status is polled at 5 Hz. Start the fade on the first frame
        // after the timeout, then render the transition at the display cadence.
        let start = *self.fade_start.get_or_insert(now);
        let t = (now.duration_since(start).as_secs_f32() / CONTROLS_FADE.as_secs_f32()).min(1.);
        1. - t * t * (3. - 2. * t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controls_fade_from_the_first_idle_frame_and_wake_on_input() {
        let now = Instant::now();
        let mut controls = ControlsVisibility {
            last_activity: now,
            fade_start: None,
        };
        let idle_frame = now + CONTROLS_IDLE + Duration::from_millis(160);
        assert_eq!(controls.opacity(idle_frame, false, true), 1.);
        assert!(
            (controls.opacity(idle_frame + CONTROLS_FADE / 2, false, true) - 0.5).abs() < 0.001
        );
        assert_eq!(
            controls.opacity(idle_frame + CONTROLS_FADE, false, true),
            0.
        );

        let input = idle_frame + Duration::from_secs(2);
        controls.wake(input);
        assert_eq!(controls.opacity(input, false, true), 1.);
        let paused = input + CONTROLS_IDLE;
        assert_eq!(controls.opacity(paused, true, false), 1.);
        assert_eq!(controls.opacity(paused + CONTROLS_IDLE, false, false), 0.);
    }
}

pub fn sdr_output(window: &Window) -> Option<SdrOutput> {
    display::sdr_output(parent(window).ok()?)
}

pub fn prepare_route(
    request: &mut PlayRequest,
    sdr: Option<SdrOutput>,
    prefs: &crate::preferences::Preferences,
) {
    match request.route {
        Route::Profile7 => {
            request.route = match local::profile7_is_fel(request.url.as_str()) {
                Ok(true) => Route::MpvProfile7 { fel: true },
                Ok(false) if !prefs.mel_to_81 => Route::MelHdr10,
                Ok(false) if prefs.windows_dolby && sdr.is_none() => Route::Mel81,
                Ok(false) | Err(_) => Route::MpvProfile7 { fel: false },
            };
        }
        Route::Dolby(_) if !prefs.windows_dolby => request.route = Route::MpvDolby,
        _ => {}
    }
    if let Some(output) = sdr {
        request.tone_map_to_sdr(output.hdr_available);
    }
}

#[derive(Default, Clone, Copy)]
pub struct Status {
    pub ready: bool,
    pub video_presented: bool,
    pub paused: bool,
    pub ended: bool,
    pub position: f64,
    pub duration: f64,
    pub buffered_end: f64,
}

enum Engine {
    Mpv(mpv::Mpv),
    Native(native::Native),
}
struct Surface(HWND);
impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

pub struct Player {
    // The engine must release its presentation resources before destroying the HWND.
    engine: Engine,
    surface: Surface,
    pub label: String,
    pub source: Source,
    pub route_notice: Option<&'static str>,
    pub performance: performance::Performance,
    pub local_file: bool,
    pub status: Status,
    refresh: Option<refresh::RefreshMatch>,
    pub transport: Option<transport::Transport>,
    report: Sender<(&'static str, f64, bool)>,
    reported_start: bool,
    last_report: Instant,
    size: (i32, i32),
    opened: Instant,
    last_pointer: gpui::Point<gpui::Pixels>,
    controls: ControlsVisibility,
    keyboard_controls: bool,
    pub audio_only: bool,
    pub seekable: bool,
    pub panel_open: bool,
    pub caption_height: f32,
    pub pcm_volume: Option<f64>,
}

pub(crate) fn parent(window: &Window) -> Result<HWND> {
    match HasWindowHandle::window_handle(window)
        .map_err(|e| anyhow::anyhow!("Reading the player window handle: {e}"))?
        .as_raw()
    {
        RawWindowHandle::Win32(w) => Ok(HWND(w.hwnd.get() as *mut _)),
        _ => anyhow::bail!("RustyFin requires Windows."),
    }
}

fn dimensions(window: &Window) -> (i32, i32) {
    let bounds = window.viewport_size();
    let scale = window.scale_factor();
    (
        (f32::from(bounds.width) * scale).round().max(1.0) as i32,
        (f32::from(bounds.height) * scale).round().max(1.0) as i32,
    )
}

impl Player {
    pub fn open(
        window: &Window,
        request: &PlayRequest,
        server: Server,
        item: &crate::jellyfin::Item,
        prefs: &crate::preferences::Preferences,
    ) -> Result<Self> {
        let size = dimensions(window);
        let surface = Surface(unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE,
                w!("STATIC"),
                None,
                WS_CHILD
                    | WS_CLIPSIBLINGS
                    | WS_DISABLED
                    | if request.audio_only {
                        WINDOW_STYLE::default()
                    } else {
                        WS_VISIBLE
                    },
                0,
                0,
                size.0,
                size.1,
                Some(parent(window)?),
                None,
                None,
                None,
            )?
        });
        let ids = (
            request.item.clone(),
            request.source.id.clone(),
            request.session.clone(),
        );
        let seekable = request.seekable;
        let online = request.url.scheme() != "file";
        let (report, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for (event, position, paused) in rx {
                if online && let Err(e) = server.report(event, &ids, position, paused, seekable) {
                    eprintln!("Jellyfin playback report failed: {e}");
                }
            }
        });
        let (engine, label) = match &request.route {
            Route::Mpv
            | Route::MpvDolby
            | Route::MpvProfile7 { .. }
            | Route::FelHdr10
            | Route::MelHdr10
            | Route::Sdr { .. } => (
                Engine::Mpv(mpv::Mpv::open(surface.0, request, &prefs.mpv)?),
                match request.route {
                    Route::Mpv => "mpv",
                    Route::MpvDolby => "mpv · Dolby Vision rendering",
                    Route::MpvProfile7 { .. } => "mpv · Profile 7 rendering",
                    Route::Sdr { .. } if request.route.hdr10_base_layer() => {
                        "mpv · HDR10 base layer · SDR tone mapping"
                    }
                    Route::Sdr { .. } => "mpv · SDR tone mapping",
                    _ => "mpv · HDR10 base layer",
                }
                .into(),
            ),
            Route::Profile7 => {
                anyhow::bail!("The Profile 7 source still needs enhancement-layer inspection.")
            }
            Route::Dolby(_) | Route::Mel81 => {
                let native = native::Native::open(surface.0, request)?;
                let label = native.label();
                (Engine::Native(native), label)
            }
        };
        let transport =
            transport::Transport::new(parent(window)?, item, request.audio_only, seekable)
                .map_err(|e| eprintln!("Windows media controls unavailable: {e:#}"))
                .ok();
        let mut performance = performance::Performance::default();
        performance.set_enabled(prefs.performance_overlay);
        Ok(Self {
            transport,
            refresh: request
                .frame_rate
                .filter(|_| prefs.match_refresh && !request.audio_only)
                .map(refresh::RefreshMatch::new),
            engine,
            surface,
            label,
            source: request.source.clone(),
            route_notice: request.route.startup_notice(),
            local_file: !online,
            performance,
            report,
            size,
            reported_start: false,
            status: Status {
                position: request.resume,
                ..Default::default()
            },
            last_report: Instant::now(),
            opened: Instant::now(),
            last_pointer: window.mouse_position(),
            controls: ControlsVisibility {
                last_activity: Instant::now(),
                fade_start: None,
            },
            keyboard_controls: false,
            audio_only: request.audio_only,
            seekable,
            panel_open: false,
            caption_height: 0.,
            pcm_volume: None,
        })
    }

    pub fn wake_controls(&mut self) {
        self.controls.wake(Instant::now());
    }

    pub fn use_keyboard_controls(&mut self) {
        self.keyboard_controls = true;
        self.wake_controls();
    }

    pub fn use_pointer_controls(&mut self) {
        self.keyboard_controls = false;
        self.wake_controls();
    }

    pub fn keyboard_controls(&self) -> bool {
        self.keyboard_controls
    }

    pub fn pointer_moved(&mut self, pointer: gpui::Point<gpui::Pixels>, window: &Window) -> bool {
        if self.last_pointer == pointer {
            return false;
        }
        self.last_pointer = pointer;
        if !gpui::Bounds::new(Default::default(), window.viewport_size()).contains(&pointer) {
            return false;
        }
        let repaint =
            self.keyboard_controls || self.controls.last_activity.elapsed() >= CONTROLS_IDLE;
        self.use_pointer_controls();
        repaint
    }

    pub fn controls_opacity(&mut self, interacting: bool, animate: bool, window: &Window) -> f32 {
        let pinned = interacting
            || self.panel_open
            || self.audio_only
            || self.status.paused
            || !self.status.ready
            || !self.status.video_presented;
        let opacity = self.controls.opacity(Instant::now(), pinned, animate);
        if self.controls.fade_start.is_some() && opacity > 0. {
            window.request_animation_frame();
        }
        opacity
    }

    pub fn poll(&mut self, window: &Window) -> Result<()> {
        if let Some(refresh) = &mut self.refresh {
            refresh.update(self.surface.0, window.is_fullscreen());
        }
        self.pointer_moved(window.mouse_position(), window);
        let size = dimensions(window);
        if self.size != size {
            unsafe {
                SetWindowPos(
                    self.surface.0,
                    None,
                    0,
                    0,
                    size.0,
                    size.1,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )?;
            }
            if let Engine::Native(p) = &self.engine
                && self.status.ready
            {
                p.resize()?;
            }
            self.size = size;
        }
        let s = match &mut self.engine {
            Engine::Mpv(p) => {
                let status =
                    p.poll((self.panel_open || self.performance.enabled) && !self.audio_only)?;
                if self.performance.enabled && status.ready && !status.ended {
                    p.probe_performance();
                }
                status
            }
            Engine::Native(p) => {
                let status = p.poll()?;
                self.label = p.label();
                status
            }
        };
        ensure!(
            s.ready
                || self.opened.elapsed()
                    < Duration::from_secs(
                        if matches!(&self.engine, Engine::Mpv(p) if !p.route_notice_ready()) {
                            15
                        } else {
                            40
                        }
                    ),
            "Timed out opening the video source."
        );
        let pcm = match &self.engine {
            Engine::Mpv(p) => s.ready && p.has_pcm_output(),
            Engine::Native(p) => s.ready && p.has_pcm_output(),
        };
        self.pcm_volume = pcm.then_some(self.pcm_volume.unwrap_or(1.));
        let pause_changed = s.paused != self.status.paused;
        let last_position = self.status.position;
        self.status = s;
        if self.performance.due() {
            let native = match &self.engine {
                Engine::Native(p) => Some(p),
                Engine::Mpv(_) => None,
            };
            self.performance.sample(self.surface.0, native);
        }
        if s.ended && s.position <= 0.0 {
            self.status.position = last_position;
        }
        if s.ready && !self.reported_start {
            self.send("");
            self.reported_start = true;
        } else if s.ready
            && (pause_changed || self.last_report.elapsed() >= Duration::from_secs(10))
        {
            self.send("Progress");
            self.last_report = Instant::now();
        }
        Ok(())
    }
    fn send(&self, event: &'static str) {
        let _ = self
            .report
            .send((event, self.status.position, self.status.paused));
    }
    pub fn set_volume(&mut self, volume: f64) -> Result<()> {
        if self.pcm_volume.is_some() {
            match &self.engine {
                Engine::Mpv(p) => p.set_volume(volume)?,
                Engine::Native(p) => p.set_volume(volume)?,
            }
            self.pcm_volume = Some(volume);
        }
        Ok(())
    }

    pub fn subtitle_delay(&self, seconds: f64) -> Result<()> {
        if let Engine::Mpv(p) = &self.engine {
            p.set_number("sub-delay", seconds)?;
        }
        Ok(())
    }

    pub fn audio_output(&self) -> Option<String> {
        match &self.engine {
            Engine::Native(p) => p.audio_output(),
            Engine::Mpv(p) => p.audio_output(),
        }
    }

    pub fn metric(&self, metric: Metric) -> Option<&str> {
        match &self.engine {
            Engine::Mpv(p) => p.metric(metric),
            Engine::Native(_) => None,
        }
    }

    pub fn native_bitstream(&self) -> bool {
        matches!(&self.engine, Engine::Native(n) if n.bitstreaming())
    }
    pub fn mpv(&self) -> bool {
        matches!(self.engine, Engine::Mpv(_))
    }
    pub fn shader(&self) -> Option<&Shader> {
        match &self.engine {
            Engine::Mpv(p) if !self.audio_only => Some(&p.shader),
            _ => None,
        }
    }
    pub fn shader_status(&self) -> Option<ShaderStatus> {
        match &self.engine {
            Engine::Mpv(p) if !self.audio_only => Some(p.shader_status),
            _ => None,
        }
    }
    pub fn rtx_support(&self) -> Result<(), &'static str> {
        match &self.engine {
            Engine::Mpv(p) if !self.audio_only => p.rtx_support(),
            _ => Err("RTX Video Super Resolution requires mpv video playback."),
        }
    }
    pub fn set_shader(&mut self, shader: Shader) -> Result<()> {
        match &mut self.engine {
            Engine::Mpv(p) if !self.audio_only => p.set_shader(shader),
            _ => anyhow::bail!("Shaders require mpv video playback."),
        }
    }
    pub fn take_shader_error(&mut self) -> Option<String> {
        match &mut self.engine {
            Engine::Mpv(p) => p.shader_error.take(),
            _ => None,
        }
    }
    pub fn opened_at(&self) -> Instant {
        self.opened
    }
    pub fn supports_speed(&self, rate: f64) -> bool {
        match &self.engine {
            Engine::Native(p) => p.supports_speed(rate),
            Engine::Mpv(_) => true,
        }
    }
    pub fn speed(&self, rate: f64) -> Result<()> {
        match &self.engine {
            Engine::Mpv(p) => p.set_number("speed", rate),
            Engine::Native(p) => p.speed(rate),
        }
    }
    pub fn audio_delay(&self, seconds: f64) -> Result<()> {
        match &self.engine {
            Engine::Mpv(p) => p.set_number("audio-delay", seconds),
            _ => anyhow::bail!("Audio offset requires mpv."),
        }
    }
    pub fn zoom(&self, scale: f64) -> Result<()> {
        match &self.engine {
            Engine::Mpv(p) => p.set_number("video-zoom", scale.log2()),
            _ => anyhow::bail!("Zoom requires mpv."),
        }
    }
    pub fn toggle(&self) -> Result<()> {
        match &self.engine {
            Engine::Mpv(p) => p.toggle(),
            Engine::Native(p) => p.toggle(),
        }
    }
    pub fn seek(&self, seconds: f64) -> Result<()> {
        if !self.seekable {
            return Ok(());
        }
        match &self.engine {
            Engine::Mpv(p) => p.seek(seconds),
            Engine::Native(p) => p.seek(seconds),
        }
    }

    pub fn seek_to(&self, seconds: f64) -> Result<()> {
        if !self.seekable {
            return Ok(());
        }
        match &self.engine {
            Engine::Mpv(p) => p.seek_to(seconds.max(0.)),
            Engine::Native(p) => p.seek_to(seconds),
        }
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        if self.reported_start {
            self.send("Stopped");
        }
    }
}

pub fn diagnostics(file: Option<std::ffi::OsString>, probe_output: bool, mel: bool) -> Result<()> {
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    let result: Result<String> = (|| {
        let mf = native::Foundation::start()?;
        println!(
            "Media Foundation renderer profiles: {}",
            native::profiles(&mf)?.join(", ")
        );
        ensure!(
            !(probe_output || mel) || file.is_some(),
            "Pass a local Dolby Vision sample to this probe."
        );
        if let Some(file) = file {
            let window = Surface(unsafe {
                CreateWindowExW(
                    if probe_output {
                        WS_EX_NOACTIVATE | WS_EX_TOPMOST
                    } else {
                        WS_EX_NOACTIVATE
                    },
                    w!("STATIC"),
                    w!("RustyFin · Dolby Vision output test"),
                    if probe_output {
                        WS_POPUP | WS_VISIBLE
                    } else {
                        WS_POPUP
                    },
                    40,
                    40,
                    960,
                    540,
                    None,
                    None,
                    None,
                    None,
                )?
            });
            let found = native::inspect(window.0, std::path::Path::new(&file), probe_output, mel)?;
            println!(
                "Selected source renderer profiles: {}",
                if found.is_empty() {
                    "none".into()
                } else {
                    found.join(", ")
                }
            );
        }
        if mel && !probe_output {
            return Ok(String::from("MEL metadata probe complete."));
        }
        Ok(format!(
            "Primary display: {}.",
            display::check(HWND::default()).context("Primary display")?
        ))
    })();
    unsafe {
        RoUninitialize();
    }
    println!("{}", result?);
    Ok(())
}
