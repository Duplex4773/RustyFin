use std::{
    mem::size_of,
    time::{Duration, Instant},
};
use windows::{
    Win32::{Foundation::HWND, Graphics::Gdi::*},
    core::PCWSTR,
};

fn cadence(fps: f64, reported: u32) -> Option<(u32, f64)> {
    if !(1.0..=240.0).contains(&fps) || reported < 10 {
        return None;
    }
    let hz = match reported {
        23 | 29 | 47 | 59 | 95 | 119 | 143 | 239 => (reported + 1) as f64 / 1.001,
        n => n as f64,
    };
    let multiple = (hz / fps).round();
    let error = (hz / multiple - fps).abs();
    (multiple >= 1. && error < 0.015).then_some((multiple as u32, error))
}

fn mode(device: &[u16], index: ENUM_DISPLAY_SETTINGS_MODE) -> Option<DEVMODEW> {
    let mut value = DEVMODEW {
        dmSize: size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    unsafe {
        EnumDisplaySettingsW(PCWSTR(device.as_ptr()), index, &mut value)
            .as_bool()
            .then_some(value)
    }
}

fn monitor_device(monitor: HMONITOR) -> Option<[u16; 32]> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    unsafe { GetMonitorInfoW(monitor, &mut info.monitorInfo) }
        .as_bool()
        .then_some(info.szDevice)
}

pub(super) fn display_frequency(window: HWND) -> Option<u32> {
    let monitor = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
    let hz = mode(&monitor_device(monitor)?, ENUM_CURRENT_SETTINGS)?.dmDisplayFrequency;
    (hz > 1).then_some(hz)
}
fn same_format(a: &DEVMODEW, b: &DEVMODEW) -> bool {
    a.dmPelsWidth == b.dmPelsWidth
        && a.dmPelsHeight == b.dmPelsHeight
        && a.dmBitsPerPel == b.dmBitsPerPel
        && unsafe {
            a.Anonymous2.dmDisplayFlags == b.Anonymous2.dmDisplayFlags
                && a.Anonymous1.Anonymous2.dmDisplayOrientation
                    == b.Anonymous1.Anonymous2.dmDisplayOrientation
        }
}
fn apply(device: &[u16], base: DEVMODEW, frequency: u32) -> bool {
    let mut requested = base;
    requested.dmDisplayFrequency = frequency;
    requested.dmFields = DM_DISPLAYFREQUENCY;
    unsafe {
        let name = PCWSTR(device.as_ptr());
        ChangeDisplaySettingsExW(name, Some(&requested), None, CDS_TEST, None)
            == DISP_CHANGE_SUCCESSFUL
            && ChangeDisplaySettingsExW(name, Some(&requested), None, CDS_FULLSCREEN, None)
                == DISP_CHANGE_SUCCESSFUL
    }
}

struct ChangedMode {
    device: [u16; 32],
    original: DEVMODEW,
    applied: u32,
}
impl Drop for ChangedMode {
    fn drop(&mut self) {
        if let Some(now) = mode(&self.device, ENUM_CURRENT_SETTINGS)
            && now.dmDisplayFrequency == self.applied
            && same_format(&now, &self.original)
        {
            apply(&self.device, now, self.original.dmDisplayFrequency);
        }
    }
}

pub struct RefreshMatch {
    fps: f64,
    monitor: Option<HMONITOR>,
    changed: Option<ChangedMode>,
    check_at: Option<Instant>,
}
impl RefreshMatch {
    pub fn new(fps: f64) -> Self {
        Self {
            fps,
            monitor: None,
            changed: None,
            check_at: None,
        }
    }
    pub fn update(&mut self, window: HWND, fullscreen: bool) {
        if !fullscreen {
            self.changed = None;
            self.monitor = None;
            self.check_at = None;
            return;
        }
        if self
            .check_at
            .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.check_at = Some(Instant::now());
        let current = unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) };
        if self.monitor == Some(current) {
            return;
        }
        self.changed = None;
        self.monitor = Some(current);
        let Some(device) = monitor_device(current) else {
            return;
        };
        let Some(original) = mode(&device, ENUM_CURRENT_SETTINGS) else {
            return;
        };
        if cadence(self.fps, original.dmDisplayFrequency).is_some() {
            return;
        }
        let chosen = (0..)
            .map_while(|n| mode(&device, ENUM_DISPLAY_SETTINGS_MODE(n)))
            .filter(|m| same_format(m, &original))
            .filter_map(|m| {
                cadence(self.fps, m.dmDisplayFrequency).map(|(n, e)| (n, e, m.dmDisplayFrequency))
            })
            .min_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        if let Some((_, _, applied)) = chosen
            && apply(&device, original, applied)
        {
            self.changed = Some(ChangedMode {
                device,
                original,
                applied,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::cadence;
    #[test]
    fn matches_fractional_and_integer_film_at_whole_multiples() {
        assert_eq!(cadence(24000. / 1001., 23).unwrap().0, 1);
        assert_eq!(cadence(24., 120).unwrap().0, 5);
        assert!(cadence(24., 60).is_none());
        assert!(cadence(24000. / 1001., 24).is_none());
    }
}
