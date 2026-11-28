use std::{
    ptr,
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, Instant},
};

use super::{Status, display, local::LocalSource};
use anyhow::{Context, Result, ensure};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HMODULE, HWND, RECT},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE, Direct3D11::*, DirectComposition::*,
            Dxgi::IDXGIDevice,
        },
        Media::MediaFoundation::*,
        System::Com::{
            CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree, StructuredStorage::*,
        },
        UI::WindowsAndMessaging::GetClientRect,
    },
    core::{BSTR, Interface, PWSTR, implement},
};

#[derive(Debug)]
pub enum OutputLost {
    DeviceChanged,
    BitstreamFailed,
}
impl std::fmt::Display for OutputLost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("The encoded audio endpoint is unavailable.")
    }
}
impl std::error::Error for OutputLost {}

pub struct Foundation;
impl Foundation {
    pub fn start() -> Result<Self> {
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL)? };
        Ok(Self)
    }
}
impl Drop for Foundation {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
        }
    }
}

fn take_string(s: PWSTR) -> Result<String> {
    let value = unsafe { s.to_string() };
    unsafe { CoTaskMemFree(Some(s.0.cast())) };
    Ok(value?)
}

pub fn profiles(_mf: &Foundation) -> Result<Vec<String>> {
    let mut entries = ptr::null_mut();
    let mut count = 0;
    unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_RENDERER_EFFECT,
            MFT_ENUM_FLAG_ALL | MFT_ENUM_FLAG_SORTANDFILTER,
            None,
            None,
            &mut entries,
            &mut count,
        )?;
    }
    let mut profiles = Vec::new();
    if !entries.is_null() {
        for i in 0..count as usize {
            // Each activation object and the array itself are caller-owned.
            let Some(effect) = (unsafe { entries.add(i).read() }) else {
                continue;
            };
            let mut value = PROPVARIANT::default();
            unsafe {
                if effect
                    .GetItem(&MFT_ENUM_VIDEO_RENDERER_EXTENSION_PROFILE, Some(&mut value))
                    .is_ok()
                {
                    for n in 0..PropVariantGetElementCount(&value) {
                        if let Some(s) = PropVariantGetStringElem(&value, n)
                            .ok()
                            .and_then(|s| take_string(s).ok())
                        {
                            profiles.push(s.to_ascii_lowercase());
                        }
                    }
                }
                let _ = PropVariantClear(&mut value);
            }
        }
        unsafe { CoTaskMemFree(Some(entries.cast())) };
    }
    profiles.sort();
    profiles.dedup();
    Ok(profiles)
}

#[implement(IMFMediaEngineNotify)]
struct Notify(Sender<(u32, usize, u32)>);
impl IMFMediaEngineNotify_Impl for Notify_Impl {
    fn EventNotify(&self, event: u32, arg: usize, code: u32) -> windows::core::Result<()> {
        // Media Foundation invokes this on a worker. Engine calls stay on the UI thread.
        let _ = self.0.send((event, arg, code));
        Ok(())
    }
}

pub struct Native {
    engine: IMFMediaEngineEx,
    composition: IDCompositionDevice,
    target: IDCompositionTarget,
    events: Receiver<(u32, usize, u32)>,
    hwnd: HWND,
    profile: String,
    resume: f64,
    started: bool,
    first_frame_ready: bool,
    video_presented: bool,
    start_paused: bool,
    source: Option<LocalSource>,
    audio: i32,
    moving: bool,
    last_display_check: Instant,
    _mf: Foundation,
}

impl Native {
    pub fn open(hwnd: HWND, request: &crate::jellyfin::PlayRequest) -> Result<Self> {
        let profile = request
            .route
            .native_profile()
            .context("Missing native renderer profile")?;
        let mel = request.route == crate::jellyfin::Route::Mel81;
        let mf = Foundation::start()?;
        let installed = profiles(&mf)?;
        ensure!(
            installed.iter().any(|p| p == profile),
            "Windows has no registered Dolby Vision renderer for {profile}. Check Dolby Vision Extensions and the source profile."
        );
        // Windows can change the active wire format while creating the Dolby decoder.
        // Validate it in present(), immediately before attaching the video surface.
        let mut player = Self::load(
            mf,
            hwnd,
            "rustyfin://direct",
            profile,
            request.resume,
            Some(LocalSource::open(
                request.url.as_str(),
                profile,
                request.audio_index,
                request.allow_passthrough,
                mel,
            )?),
        )?;
        player.start_paused = request.start_paused;
        Ok(player)
    }

    fn load(
        mf: Foundation,
        hwnd: HWND,
        url: &str,
        profile: &str,
        resume: f64,
        source: Option<LocalSource>,
    ) -> Result<Self> {
        unsafe {
            let mut d3d = None;
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut d3d),
                None,
                None,
            )?;
            let d3d = d3d.context("Creating a video device")?;
            let _ = d3d
                .cast::<ID3D11Multithread>()?
                .SetMultithreadProtected(true);
            let mut token = 0;
            let mut manager = None;
            MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
            let manager = manager.context("Creating the Media Foundation device manager")?;
            manager.ResetDevice(&d3d, token)?;
            let composition: IDCompositionDevice =
                DCompositionCreateDevice(&d3d.cast::<IDXGIDevice>()?)?;
            let target = composition.CreateTargetForHwnd(hwnd, true)?;

            let (tx, events) = mpsc::channel();
            let callback: IMFMediaEngineNotify = Notify(tx).into();
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 4)?;
            let attrs = attrs.context("Creating Media Engine attributes")?;
            attrs.SetUnknown(&MF_MEDIA_ENGINE_CALLBACK, &callback)?;
            attrs.SetUINT32(
                &MF_MEDIA_ENGINE_AUDIO_ENDPOINT_ROLE,
                windows::Win32::Media::Audio::eMultimedia.0 as u32,
            )?;
            attrs.SetUnknown(&MF_MEDIA_ENGINE_DXGI_MANAGER, &manager)?;
            attrs.SetUINT64(&MF_MEDIA_ENGINE_OPM_HWND, hwnd.0 as u64)?;
            attrs.SetGUID(
                &MF_MEDIA_ENGINE_BROWSER_COMPATIBILITY_MODE,
                &MF_MEDIA_ENGINE_BROWSER_COMPATIBILITY_MODE_IE_EDGE,
            )?;
            if let Some(local) = &source {
                attrs.SetUnknown(&MF_MEDIA_ENGINE_EXTENSION, &local.extension)?;
            }
            let factory: IMFMediaEngineClassFactory =
                CoCreateInstance(&CLSID_MFMediaEngineClassFactory, None, CLSCTX_INPROC_SERVER)?;
            let engine: IMFMediaEngineEx = factory.CreateInstance(0, &attrs)?.cast()?;
            let player = Self {
                engine,
                composition,
                target,
                events,
                hwnd,
                profile: profile.into(),
                resume,
                started: false,
                first_frame_ready: false,
                video_presented: false,
                start_paused: false,
                source,
                audio: 0,
                moving: false,
                last_display_check: Instant::now(),
                _mf: mf,
            };
            player.engine.SetAutoPlay(false)?;
            player.engine.EnableWindowlessSwapchainMode(true)?;
            player
                .engine
                .SetPreload(MF_MEDIA_ENGINE_PRELOAD_AUTOMATIC)?;
            player.engine.SetSource(&BSTR::from(url))?;
            player.engine.Load()?;
            Ok(player)
        }
    }

    fn source_profiles(&self) -> Result<Vec<String>> {
        unsafe {
            // Verify that the Windows demuxer exposed the renderer extension
            // profile for the selected stream. A playable HEVC base layer alone
            // would also satisfy a generic codec capability check.
            let mut profiles = Vec::new();
            for index in 0..self.engine.GetNumberOfStreams()? {
                if !self.engine.GetStreamSelection(index)?.as_bool() {
                    continue;
                }
                if let Ok(mut attr) = self
                    .engine
                    .GetStreamAttribute(index, &MF_MT_VIDEO_RENDERER_EXTENSION_PROFILE)
                {
                    let profile = PropVariantToStringAlloc(&attr)
                        .ok()
                        .and_then(|s| take_string(s).ok());
                    let _ = PropVariantClear(&mut attr);
                    if let Some(p) = profile {
                        profiles.push(p.to_ascii_lowercase());
                    }
                }
            }
            Ok(profiles)
        }
    }

    fn present(&mut self) -> Result<()> {
        display::check(self.hwnd)?;
        unsafe {
            if !self.source_profiles()?.contains(&self.profile) {
                anyhow::bail!(
                    "Windows did not expose {} on the selected video stream.",
                    self.profile
                );
            }
            self.resize()?;
            let handle: HANDLE = self.engine.GetVideoSwapchainHandle()?;
            let surface = self.composition.CreateSurfaceFromHandle(handle);
            let _ = CloseHandle(handle);
            let visual = self.composition.CreateVisual()?;
            visual.SetContent(&surface?)?;
            self.target.SetRoot(&visual)?;
            self.composition.Commit()?;
            if self.resume > 0.0 {
                self.engine.SetCurrentTime(self.resume)?;
            }
            if !self.start_paused {
                self.engine.Play()?;
            }
            self.started = true;
        }
        Ok(())
    }

    pub fn resize(&self) -> Result<()> {
        let mut rect = RECT::default();
        unsafe {
            GetClientRect(self.hwnd, &mut rect)?;
            self.engine.UpdateVideoStream(None, Some(&rect), None)?;
        }
        Ok(())
    }

    fn check_source(&self) -> Result<()> {
        if let Some(reason) = self.source.as_ref().and_then(LocalSource::mel_error) {
            anyhow::bail!(reason);
        }
        Ok(())
    }
    pub fn poll(&mut self) -> Result<Status> {
        self.check_source()?;
        while let Ok((event, arg, code)) = self.events.try_recv() {
            match MF_MEDIA_ENGINE_EVENT(event as i32) {
                MF_MEDIA_ENGINE_EVENT_FIRSTFRAMEREADY => self.first_frame_ready = true,
                MF_MEDIA_ENGINE_EVENT_PLAYING | MF_MEDIA_ENGINE_EVENT_SEEKED => self.moving = true,
                MF_MEDIA_ENGINE_EVENT_PAUSE
                | MF_MEDIA_ENGINE_EVENT_WAITING
                | MF_MEDIA_ENGINE_EVENT_SEEKING
                | MF_MEDIA_ENGINE_EVENT_ENDED => self.moving = false,
                MF_MEDIA_ENGINE_EVENT_ERROR => {
                    self.check_source()?;
                    anyhow::bail!(
                        "Windows could not play this Dolby Vision source (media error {arg}, HRESULT 0x{code:08X})."
                    )
                }
                MF_MEDIA_ENGINE_EVENT_LOADEDMETADATA if !self.started => self.present()?,
                _ => {}
            }
        }
        if self.started && self.last_display_check.elapsed() >= Duration::from_secs(1) {
            if self
                .source
                .as_ref()
                .is_some_and(LocalSource::endpoint_changed)
            {
                return Err(OutputLost::DeviceChanged.into());
            }
            display::check(self.hwnd)?;
            self.last_display_check = Instant::now();
        }
        unsafe {
            let position = self.engine.GetCurrentTime();
            let paused = self.engine.IsPaused().as_bool();
            let seeking = self.engine.IsSeeking().as_bool();
            if self.started && !self.video_presented {
                let rendered = self.frame_count(MF_MEDIA_ENGINE_STATISTIC_FRAMES_RENDERED);
                // A paused first image can be visible without advancing the rendered frame count.
                self.video_presented = rendered.is_some_and(|frames| frames > 0)
                    || (self.first_frame_ready
                        && !seeking
                        && (paused || (rendered.is_none() && self.moving)));
            }
            if let Some(local) = &self.source {
                self.audio = local.clock(position, self.moving && !paused && !seeking);
                if self.audio == -2 {
                    return Err(OutputLost::BitstreamFailed.into());
                }
            }
            Ok(Status {
                ready: self.started,
                video_presented: self.video_presented,
                position,
                duration: self.engine.GetDuration(),
                buffered_end: self.buffered_end(position).unwrap_or(0.),
                paused,
                ended: self.engine.IsEnded().as_bool(),
            })
        }
    }
    pub fn label(&self) -> String {
        let profile = if self.source.as_ref().is_some_and(|s| s.mel) {
            "Dolby Vision 7 MEL → 8.1"
        } else {
            &self.profile
        };
        match self.audio {
            -1 => format!("Windows · {profile} · Audio bitstream"),
            n if n > 0 => format!("Windows · {profile} · PCM {n} ch"),
            _ => format!("Windows · {profile}"),
        }
    }
    pub fn has_pcm_output(&self) -> bool {
        self.audio > 0
    }

    fn frame_count(&self, kind: MF_MEDIA_ENGINE_STATISTIC) -> Option<u32> {
        unsafe {
            let mut statistic = self.engine.GetStatistics(kind).ok()?;
            let count = PropVariantToUInt32(&statistic).ok();
            let _ = PropVariantClear(&mut statistic);
            count
        }
    }

    fn buffered_end(&self, position: f64) -> Option<f64> {
        unsafe {
            self.engine.GetBuffered().ok().and_then(|ranges| {
                (0..ranges.GetLength()).find_map(|i| {
                    let (start, end) = (ranges.GetStart(i).ok()?, ranges.GetEnd(i).ok()?);
                    (position >= start && position <= end && end.is_finite()).then_some(end)
                })
            })
        }
    }

    pub(super) fn performance_counters(&self) -> (Option<u32>, Option<u32>, Option<f64>) {
        if !self.started {
            return (None, None, None);
        }
        let position = unsafe { self.engine.GetCurrentTime() };
        let buffered = self.buffered_end(position).map(|end| end - position);
        (
            self.frame_count(MF_MEDIA_ENGINE_STATISTIC_FRAMES_RENDERED),
            self.frame_count(MF_MEDIA_ENGINE_STATISTIC_FRAMES_DROPPED),
            buffered,
        )
    }
    pub fn set_volume(&self, volume: f64) -> Result<()> {
        unsafe {
            self.engine.SetVolume(volume)?;
        }
        Ok(())
    }

    pub fn audio_output(&self) -> Option<String> {
        match self.audio {
            -1 => Some("Digital bitstream".into()),
            n if n > 0 => Some(format!("PCM · {n} channels")),
            _ => None,
        }
    }
    pub fn bitstreaming(&self) -> bool {
        self.audio == -1
    }
    pub fn supports_speed(&self, rate: f64) -> bool {
        (rate == 1. || !self.bitstreaming())
            && unsafe { self.engine.IsPlaybackRateSupported(rate).as_bool() }
    }
    pub fn speed(&self, rate: f64) -> Result<()> {
        ensure!(
            self.audio != -1 || rate == 1.,
            "The receiver requires normal speed for bitstream audio."
        );
        unsafe {
            ensure!(
                self.engine.IsPlaybackRateSupported(rate).as_bool(),
                "Windows does not support that speed for this source."
            );
            self.engine.SetPlaybackRate(rate)?;
        }
        Ok(())
    }
    pub fn toggle(&self) -> Result<()> {
        if self.started {
            unsafe {
                if self.engine.IsPaused().as_bool() {
                    self.engine.Play()?
                } else {
                    if let Some(s) = &self.source {
                        s.clock(self.engine.GetCurrentTime(), false);
                    }
                    self.engine.Pause()?
                }
            }
        }
        Ok(())
    }
    pub fn seek(&self, seconds: f64) -> Result<()> {
        if self.started {
            self.seek_to(unsafe { self.engine.GetCurrentTime() } + seconds)?;
        }
        Ok(())
    }

    pub fn seek_to(&self, seconds: f64) -> Result<()> {
        if self.started {
            unsafe {
                if let Some(s) = &self.source {
                    s.clock(self.engine.GetCurrentTime(), false);
                }
                self.engine
                    .SetCurrentTime(seconds.max(0.0).min(self.engine.GetDuration()))?;
            }
        }
        Ok(())
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        unsafe {
            if let Some(source) = &self.source {
                source.close();
            }
            let _ = self.engine.Shutdown();
        }
    }
}

// Inspect the same demuxer and selected Media Engine streams used for playback.
pub fn inspect(
    hwnd: HWND,
    file: &std::path::Path,
    probe_output: bool,
    mel: bool,
) -> Result<Vec<String>> {
    let url = url::Url::from_file_path(file.canonicalize()?)
        .map_err(|_| anyhow::anyhow!("Invalid file path"))?;
    let local = (probe_output || mel)
        .then(|| {
            LocalSource::open(
                url.as_str(),
                if mel { "dvhe.08" } else { "" },
                None,
                false,
                mel,
            )
        })
        .transpose()?;
    let source_url = if local.is_some() {
        "rustyfin://direct"
    } else {
        url.as_str()
    };
    let mut native = Native::load(Foundation::start()?, hwnd, source_url, "", 0., local)?;
    let start = Instant::now();
    loop {
        native.check_source()?;
        use windows::Win32::UI::WindowsAndMessaging::*;
        unsafe {
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        while let Ok((event, _, code)) = native.events.try_recv() {
            match MF_MEDIA_ENGINE_EVENT(event as i32) {
                MF_MEDIA_ENGINE_EVENT_ERROR => {
                    native.check_source()?;
                    anyhow::bail!("Media Foundation could not open the sample (0x{code:08X}).")
                }
                MF_MEDIA_ENGINE_EVENT_LOADEDMETADATA if !native.started => {
                    let selected = native.source_profiles()?;
                    if mel {
                        ensure!(
                            selected.iter().any(|p| p == "dvhe.08"),
                            "Windows did not select the converted Dolby Vision Profile 8 stream."
                        );
                    }
                    if !probe_output {
                        return Ok(selected);
                    }
                    let installed = profiles(&native._mf)?;
                    let profile = selected
                        .first()
                        .filter(|p| installed.contains(p))
                        .context("The sample has no matching Dolby Vision renderer.")?;
                    native.profile = profile.clone();
                    unsafe {
                        native.engine.SetMuted(true)?;
                    }
                    native.present()?;
                }
                _ => {}
            }
        }
        if native.started && unsafe { native.engine.GetCurrentTime() } >= 6.0 {
            unsafe {
                let mut stat = native
                    .engine
                    .GetStatistics(MF_MEDIA_ENGINE_STATISTIC_FRAMES_RENDERED)?;
                let frames = PropVariantToUInt32(&stat);
                let _ = PropVariantClear(&mut stat);
                let frames = frames?;
                println!(
                    "Native renderer {} presented {frames} frames in {:.1} seconds.",
                    native.profile,
                    native.engine.GetCurrentTime()
                );
                ensure!(
                    frames > 0,
                    "The native renderer did not present any frames."
                );
            }
            println!(
                "Display during playback: {}.",
                display::check(hwnd).context("Display mode during native playback")?
            );
            if let Some(local) = &native.source {
                let channels = local.clock(0.0, false);
                if channels > 0 {
                    println!("Local audio output: PCM {channels} ch (muted).");
                }
            }
            return native.source_profiles();
        }
        ensure!(
            start.elapsed() < Duration::from_secs(20),
            "Timed out waiting for Windows stream metadata."
        );
        std::thread::sleep(Duration::from_millis(15));
    }
}
