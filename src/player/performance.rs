use std::{
    cell::Cell,
    mem::size_of,
    rc::Rc,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::HWND,
    System::{
        ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetCurrentProcess,
    },
};

#[derive(Default)]
pub struct Performance {
    pub enabled: bool,
    pub paints: Rc<Cell<u32>>,
    pub ui_fps: Option<f64>,
    pub memory: Option<usize>,
    pub display_hz: Option<u32>,
    pub native_render_fps: Option<f64>,
    pub native_dropped: Option<u32>,
    pub native_buffer_seconds: Option<f64>,
    sampled: Option<Instant>,
    native_frames: Option<u32>,
}

impl Performance {
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        self.sampled = None;
        self.native_frames = None;
        self.ui_fps = None;
        self.native_render_fps = None;
        self.paints.set(0);
    }

    pub(super) fn due(&self) -> bool {
        self.enabled
            && self
                .sampled
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(1))
    }

    pub(super) fn sample(&mut self, hwnd: HWND, native: Option<&super::native::Native>) {
        let now = Instant::now();
        let elapsed = self
            .sampled
            .replace(now)
            .map(|at| now.duration_since(at).as_secs_f64());
        let paints = self.paints.replace(0);
        self.ui_fps = elapsed.map(|seconds| f64::from(paints) / seconds);
        let bytes = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let mut counters = PROCESS_MEMORY_COUNTERS {
            cb: bytes,
            ..Default::default()
        };
        self.memory = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, bytes) }
            .is_ok()
            .then_some(counters.WorkingSetSize);
        self.display_hz = super::refresh::display_frequency(hwnd);
        if let Some(native) = native {
            let (rendered, dropped, buffered) = native.performance_counters();
            self.native_render_fps = elapsed.zip(rendered).zip(self.native_frames).and_then(
                |((seconds, frames), previous)| {
                    frames
                        .checked_sub(previous)
                        .map(|count| f64::from(count) / seconds)
                },
            );
            self.native_frames = rendered;
            self.native_dropped = dropped;
            self.native_buffer_seconds = buffered;
        }
    }
}
