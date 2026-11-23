use anyhow::{Context, Result, ensure};
use std::mem::size_of;
use windows::{
    Devices::Display::Core::{
        DisplayManager, DisplayManagerOptions, DisplayManagerResult, DisplayWireFormatEotf,
        DisplayWireFormatHdrMetadata,
    },
    Win32::{
        Foundation::HWND, Graphics::Gdi::*, UI::WindowsAndMessaging::EDD_GET_DEVICE_INTERFACE_NAME,
    },
    core::PCWSTR,
};

fn output_mode(
    metadata: DisplayWireFormatHdrMetadata,
    eotf: DisplayWireFormatEotf,
    dolby_in_hdr: bool,
) -> Option<&'static str> {
    if metadata == DisplayWireFormatHdrMetadata::DolbyVisionLowLatency {
        Some("Dolby Vision low-latency wire format")
    } else if eotf == DisplayWireFormatEotf::HdrSmpte2084 && dolby_in_hdr {
        Some("active PQ HDR on a display advertising Dolby Vision in HDR mode")
    } else {
        None
    }
}

// The installed SDK defines GET_ADVANCED_COLOR_INFO_2. windows-rs 0.61
// exposes the common packet header but does not project this Windows 11 packet.
#[repr(C)]
#[derive(Default)]
struct AdvancedColorInfo {
    header: windows::Win32::Devices::Display::DISPLAYCONFIG_DEVICE_INFO_HEADER,
    flags: u32,
    color_encoding: u32,
    bits_per_channel: u32,
    active_mode: i32,
}
fn advanced_color(
    target: &windows::Devices::Display::Core::DisplayTarget,
) -> Result<AdvancedColorInfo> {
    use windows::Win32::{Devices::Display::*, Foundation::LUID};
    let adapter = target.Adapter()?.Id()?;
    let mut info = AdvancedColorInfo {
        header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
            r#type: DISPLAYCONFIG_DEVICE_INFO_TYPE(15),
            size: size_of::<AdvancedColorInfo>() as u32,
            adapterId: LUID {
                LowPart: adapter.LowPart,
                HighPart: adapter.HighPart,
            },
            id: target.AdapterRelativeId()?,
        },
        ..Default::default()
    };
    let code = unsafe { DisplayConfigGetDeviceInfo(&mut info.header) };
    ensure!(
        code == 0,
        "Windows could not read HDR settings (error {code})."
    );
    Ok(info)
}
fn color_settings(info: &AdvancedColorInfo) -> String {
    let user = if info.flags & (1 << 5) != 0 {
        "on"
    } else {
        "off"
    };
    let active = match info.active_mode {
        0 => "SDR",
        1 => "wide color gamut",
        2 => "HDR",
        _ => "unknown",
    };
    format!(
        "Windows HDR preference {user}, active color mode {active}, {} bits per channel.",
        info.bits_per_channel
    )
}

fn display_path(hwnd: HWND) -> Result<windows::Devices::Display::Core::DisplayPath> {
    // Match the actual playback window to its monitor device interface. Mirrored
    // outputs cannot be identified as a single display with this mapping.
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    let mut ids = Vec::new();
    unsafe {
        GetMonitorInfoW(
            MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY),
            &mut info.monitorInfo,
        )
        .ok()?;
        for index in 0.. {
            let mut device = DISPLAY_DEVICEW {
                cb: size_of::<DISPLAY_DEVICEW>() as u32,
                ..Default::default()
            };
            if !EnumDisplayDevicesW(
                PCWSTR(info.szDevice.as_ptr()),
                index,
                &mut device,
                EDD_GET_DEVICE_INTERFACE_NAME,
            )
            .as_bool()
            {
                break;
            }
            if device.StateFlags.contains(DISPLAY_DEVICE_ACTIVE) {
                ids.push(PCWSTR(device.DeviceID.as_ptr()).to_string()?);
            }
        }
    }
    ensure!(
        ids.len() == 1,
        "Windows could not identify a single playback display. Use an extended or single display."
    );
    let manager = DisplayManager::Create(DisplayManagerOptions::None)?;
    let snapshot = manager
        .TryReadCurrentStateForAllTargets()
        .context("Reading the active Windows display configuration")?;
    ensure!(
        snapshot.ErrorCode()? == DisplayManagerResult::Success,
        "Windows could not read the current display configuration."
    );
    for view in snapshot.State()?.Views()? {
        for path in view.Paths()? {
            let Ok(monitor) = path.Target().and_then(|t| t.TryGetMonitor()) else {
                continue;
            };
            if !monitor
                .DeviceId()?
                .to_string()
                .eq_ignore_ascii_case(&ids[0])
            {
                continue;
            }
            return Ok(path);
        }
    }
    anyhow::bail!("Windows could not match the playback window to an active display path.")
}

pub fn check(hwnd: HWND) -> Result<&'static str> {
    let path = display_path(hwnd)?;
    let monitor = path.Target()?.TryGetMonitor()?;
    let wire = path.WireFormat()?;
    let mode = wire.HdrMetadata()?;
    // Some monitors explicitly designate active PQ HDR as a special
    // Dolby Vision mode. Windows exposes this for the Dolby decoder.
    let eotf = wire.Eotf()?;
    let special = monitor.IsDolbyVisionSupportedInHdrMode().unwrap_or(false);
    if let Some(label) = output_mode(mode, eotf, special) {
        return Ok(label);
    }
    let transfer = match eotf {
        DisplayWireFormatEotf::Sdr => "SDR".to_owned(),
        DisplayWireFormatEotf::HdrSmpte2084 => "PQ HDR".to_owned(),
        other => format!("transfer function {}", other.0),
    };
    let settings = path
        .Target()
        .ok()
        .and_then(|t| advanced_color(&t).ok())
        .map(|info| color_settings(&info))
        .unwrap_or_default();
    anyhow::bail!(
        "Windows reports {transfer} and {} for {}. {settings} Dolby Vision in HDR mode support is {special}.",
        match mode {
            DisplayWireFormatHdrMetadata::Hdr10 => "HDR10",
            DisplayWireFormatHdrMetadata::Hdr10Plus => "HDR10+",
            _ => "no Dolby Vision wire metadata",
        },
        monitor.DisplayName()?
    );
}

pub struct HdrSettings {
    pub name: String,
    pub enabled: bool,
    pub supported: bool,
    pub description: String,
    header: windows::Win32::Devices::Display::DISPLAYCONFIG_DEVICE_INFO_HEADER,
}
impl HdrSettings {
    pub fn read(hwnd: HWND) -> Result<Self> {
        let target = display_path(hwnd)?.Target()?;
        let info = advanced_color(&target)?;
        Ok(Self {
            name: target.TryGetMonitor()?.DisplayName()?.to_string(),
            enabled: info.flags & (1 << 5) != 0,
            supported: info.flags & (1 << 4) != 0,
            description: color_settings(&info),
            header: info.header,
        })
    }
    pub fn set(&self, enabled: bool) -> Result<()> {
        use windows::Win32::Devices::Display::*;
        #[repr(C)]
        struct HdrRequest {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
            enable: u32,
        }
        ensure!(
            self.supported,
            "Windows does not expose HDR controls for {}.",
            self.name
        );
        let request = HdrRequest {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_TYPE(16),
                size: size_of::<HdrRequest>() as u32,
                ..self.header
            },
            enable: u32::from(enabled),
        };
        let code = unsafe { DisplayConfigSetDeviceInfo(&request.header) };
        ensure!(
            code == 0,
            "Windows could not change HDR for {} (error {code}).",
            self.name
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{DisplayWireFormatEotf as Eotf, DisplayWireFormatHdrMetadata as Meta, output_mode};
    #[test]
    fn distinguishes_dolby_in_hdr_from_ordinary_hdr10() {
        assert!(output_mode(Meta::Hdr10, Eotf::HdrSmpte2084, true).is_some());
        assert!(output_mode(Meta::Hdr10, Eotf::HdrSmpte2084, false).is_none());
        assert!(output_mode(Meta::None, Eotf::Sdr, true).is_none());
        assert!(output_mode(Meta::DolbyVisionLowLatency, Eotf::HdrSmpte2084, false).is_some());
    }
}
