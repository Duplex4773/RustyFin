mod display;
pub use display::HdrSettings;
mod local;
mod mel;
pub use mel::LayerError;
mod mpv;
mod native;
mod refresh;
mod timeline;
pub use timeline::{HEIGHT as TIMELINE_HEIGHT, RAIL_INSET};
pub mod transport;
pub use native::OutputLost;

use crate::jellyfin::{PlayRequest, Route, Server};
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

pub const CONTROLS_HEIGHT: f32 = 78.0;

pub fn resolve_profile7(request: &mut PlayRequest, mel_to_81: bool) -> Result<()> {
    if request.route == Route::Profile7 {
        request.route = local::profile7_route(request.url.as_str(), mel_to_81)?;
    }
    Ok(())
}

#[derive(Default, Clone, Copy)]
pub struct Status {
    pub ready: bool,
    pub paused: bool,
    pub ended: bool,
    pub position: f64,
    pub duration: f64,
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

#[derive(Clone, Copy, PartialEq)]
struct VideoRegion {
    width: i32,
    bottom: i32,
    holes: [Option<[i32; 5]>; 3],
}

pub struct Player {
    // The engine must release its presentation resources before destroying the HWND.
    engine: Engine,
    surface: Surface,
    pub label: String,
    pub status: Status,
    refresh: Option<refresh::RefreshMatch>,
    pub transport: Option<transport::Transport>,
    report: Sender<(&'static str, f64, bool)>,
    reported_start: bool,
    last_report: Instant,
    size: (i32, i32),
    opened: Instant,
    last_pointer: gpui::Point<gpui::Pixels>,
    last_motion: Instant,
    pub controls_visible: bool,
    pub audio_only: bool,
    pub seekable: bool,
    pub panel_open: bool,
    cutout: std::cell::Cell<Option<VideoRegion>>,
    timeline: std::cell::OnceCell<std::cell::RefCell<Option<timeline::Timeline>>>,
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
            request.source.clone(),
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
            Route::Mpv | Route::FelHdr10 | Route::MelHdr10 => (
                Engine::Mpv(mpv::Mpv::open(surface.0, request, &prefs.mpv)?),
                if request.route == Route::Mpv {
                    "mpv"
                } else {
                    "mpv · HDR10 base layer"
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
        Ok(Self {
            transport,
            refresh: request
                .frame_rate
                .filter(|_| prefs.match_refresh && !request.audio_only)
                .map(refresh::RefreshMatch::new),
            engine,
            surface,
            label,
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
            last_motion: Instant::now(),
            controls_visible: !window.is_fullscreen(),
            audio_only: request.audio_only,
            seekable,
            panel_open: false,
            cutout: Default::default(),
            timeline: Default::default(),
            caption_height: 0.,
            pcm_volume: None,
        })
    }

    pub fn poll(&mut self, window: &Window) -> Result<()> {
        if let Some(refresh) = &mut self.refresh {
            refresh.update(self.surface.0, window.is_fullscreen());
        }
        let pointer = window.mouse_position();
        if pointer != self.last_pointer {
            self.last_pointer = pointer;
            self.last_motion = Instant::now();
        }
        let over_controls = pointer.y >= window.viewport_size().height - gpui::px(CONTROLS_HEIGHT)
            && pointer.y <= window.viewport_size().height
            && pointer.x >= gpui::px(0.)
            && pointer.x <= window.viewport_size().width;
        self.controls_visible = self.panel_open
            || !window.is_fullscreen()
            || (over_controls
                && (self.controls_visible || self.last_motion.elapsed() < Duration::from_secs(2)));
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
            Engine::Mpv(p) => p.poll()?,
            Engine::Native(p) => {
                let status = p.poll()?;
                self.label = p.label();
                status
            }
        };
        ensure!(
            s.ready || self.opened.elapsed() < Duration::from_secs(40),
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

    pub fn show_timeline(
        &self,
        w: &Window,
        visible: bool,
        progress: f32,
        elapsed: &str,
        remaining: &str,
    ) -> bool {
        if !visible {
            if let Some(cell) = self.timeline.get()
                && let Some(layer) = cell.borrow_mut().as_mut()
            {
                layer.hide();
            }
            return false;
        }
        // The previous player may still own the window's composition target
        // while its replacement is opening. Acquire it on first presentation.
        let cell = self.timeline.get_or_init(|| {
            std::cell::RefCell::new(
                parent(w)
                    .and_then(timeline::Timeline::new)
                    .map_err(|e| eprintln!("Seek overlay unavailable: {e:#}"))
                    .ok(),
            )
        });
        let mut layer = cell.borrow_mut();
        let Some(overlay) = layer.as_mut() else {
            return false;
        };
        let scale = w.scale_factor();
        let y = ((f32::from(w.viewport_size().height) - CONTROLS_HEIGHT) * scale).round() as i32;
        let width = (f32::from(w.viewport_size().width) * scale).round() as i32;
        match overlay.show(width, y, scale, progress, elapsed, remaining) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("Seek overlay unavailable: {e:#}");
                layer.take();
                false
            }
        }
    }

    /// Leave the video at its natural viewport size. Expose only opaque GPUI
    /// elements and the lower button row underneath the video child window.
    pub fn overlay_cutouts(
        &self,
        holes: [Option<(gpui::Bounds<gpui::Pixels>, f32)>; 3],
        reserved: f32,
        scale: f32,
    ) {
        use windows::Win32::Graphics::Gdi::*;
        let device = |v: gpui::Pixels| (f32::from(v) * scale).round() as i32;
        let holes = holes.map(|rect| {
            rect.map(|(b, r)| {
                [
                    device(b.left()),
                    device(b.top()),
                    device(b.right()),
                    device(b.bottom()),
                    (2. * r * scale).round() as i32,
                ]
            })
        });
        let geometry =
            (reserved > 0. || holes.iter().any(Option::is_some)).then_some(VideoRegion {
                width: self.size.0,
                bottom: self.size.1 - (reserved * scale).round() as i32,
                holes,
            });
        if self.cutout.get() == geometry {
            return;
        }
        unsafe {
            let Some(region) = geometry else {
                if SetWindowRgn(self.surface.0, None, true) != 0 {
                    self.cutout.set(None);
                }
                return;
            };
            let visible = CreateRectRgn(0, 0, region.width, region.bottom);
            if visible.is_invalid() {
                return;
            }
            for [x, y, right, bottom, diameter] in region.holes.into_iter().flatten() {
                let hole = CreateRoundRectRgn(x, y, right + 1, bottom + 1, diameter, diameter);
                let ok = !hole.is_invalid()
                    && CombineRgn(Some(visible), Some(visible), Some(hole), RGN_DIFF) != RGN_ERROR;
                if !hole.is_invalid() {
                    let _ = DeleteObject(hole.into());
                }
                if !ok {
                    let _ = DeleteObject(visible.into());
                    return;
                }
            }
            // SetWindowRgn takes ownership on success.
            if SetWindowRgn(self.surface.0, Some(visible), true) != 0 {
                self.cutout.set(geometry);
            } else {
                let _ = DeleteObject(visible.into());
            }
        }
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

    pub fn native_bitstream(&self) -> bool {
        matches!(&self.engine, Engine::Native(n) if n.bitstreaming())
    }
    pub fn mpv(&self) -> bool {
        matches!(self.engine, Engine::Mpv(_))
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
