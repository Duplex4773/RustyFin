use super::*;

const FILTER: &str = "@rustyfin-rtx";
const UNSUPPORTED: &str = "RTX Video Super Resolution is not supported on the GPU mpv is using.";

#[derive(Default)]
pub(super) struct RtxVsr {
    gpu_supported: Option<bool>,
    rejected: Option<&'static str>,
    filter_scale: Option<u16>,
    candidate: Option<u16>,
    last_check: Option<Instant>,
    driver_accepted: bool,
}

impl Mpv {
    pub fn rtx_support(&self) -> Result<(), &'static str> {
        if let Some(reason) = self.rtx.rejected {
            return Err(reason);
        }
        match self.rtx.gpu_supported {
            Some(true) => Ok(()),
            Some(false) => Err(UNSUPPORTED),
            None => Err("mpv has yet to identify a supported NVIDIA RTX GPU."),
        }
    }

    pub(super) fn rtx_log(&mut self, prefix: &[u8], message: &str, level: c_int) {
        // mpv logs the adapter that owns its D3D11 device. A system-wide GPU
        // list would give false positives on hybrid graphics.
        if prefix == b"vo/gpu-next/d3d11"
            && let Some(device) = message.trim().strip_prefix("Device Name: ")
        {
            self.rtx.gpu_supported = Some(supported_device(device));
            if self.shader != Shader::RtxVsr {
                unsafe { (self.request_log)(self.handle.as_ptr(), c"error".as_ptr()) };
            }
        }
        if self.shader != Shader::RtxVsr || !prefix.ends_with(b"d3d11vpp") {
            return;
        }
        if message.contains("NVIDIA RTX Super Resolution enabled") {
            self.rtx.driver_accepted = true;
        } else if message.contains("Failed to enable NVIDIA RTX Super Resolution") || level < 30 {
            self.rtx.rejected =
                Some("The graphics driver rejected mpv’s RTX Video Super Resolution filter.");
        }
    }

    pub(super) fn remove_rtx_filter(&mut self) -> Result<()> {
        if self.rtx.filter_scale.is_some() {
            self.run(&["vf", "remove", FILTER])?;
            self.rtx.filter_scale = None;
        }
        self.rtx.candidate = None;
        self.rtx.driver_accepted = false;
        Ok(())
    }

    pub(super) fn update_rtx(&mut self) {
        if self.rtx.gpu_supported.is_none()
            && self.number(c"osd-width") > 0.
            && self
                .string_property(c"current-gpu-context")
                .is_some_and(|context| context != "d3d11")
        {
            self.rtx.gpu_supported = Some(false);
            unsafe { (self.request_log)(self.handle.as_ptr(), c"error".as_ptr()) };
        }
        if self.shader != Shader::RtxVsr {
            return;
        }
        if self.rtx.gpu_supported.is_none() && self.number(c"osd-width") <= 0. {
            return;
        }
        if let Err(reason) = self.rtx_support() {
            let _ = self.remove_rtx_filter();
            self.shader_status = ShaderStatus::RtxUnavailable(reason);
            return;
        }
        if self
            .rtx
            .last_check
            .is_some_and(|last| last.elapsed() < Duration::from_millis(500))
        {
            return;
        }
        self.rtx.last_check = Some(Instant::now());
        let width = self.number(c"video-params/w");
        let height = self.number(c"video-params/h");
        let (mut target_width, mut target_height) = self.render_size();
        if self.number(c"video-params/rotate") as i32 % 180 != 0 {
            std::mem::swap(&mut target_width, &mut target_height);
        }
        if width <= 0. || height <= 0. || target_width <= 0. || target_height <= 0. {
            return;
        }
        let scale = rtx_scale(width, height, target_width, target_height);
        if self.rtx.candidate.replace(scale) != Some(scale) && self.rtx.filter_scale.is_some() {
            // Wait for the size to settle before rebuilding the video filter.
            return;
        }
        if scale <= 100 {
            if let Err(error) = self.remove_rtx_filter() {
                self.shader_error = Some(format!("Could not remove the RTX filter. {error:#}"));
                return;
            }
            let display_scale = (target_width / width).min(target_height / height);
            self.shader_status = if display_scale > 1.
                && (width.max(height) >= 3840. || width.min(height) >= 2160.)
            {
                ShaderStatus::RtxAtUhd
            } else {
                ShaderStatus::BelowScale {
                    scale: display_scale,
                    required: 1.,
                }
            };
        } else if self.rtx.filter_scale != Some(scale) {
            self.rtx.driver_accepted = false;
            let filter = format!(
                "{FILTER}:d3d11vpp=scale={:.2}:scaling-mode=nvidia",
                f64::from(scale) / 100.
            );
            match self.run(&["vf", "add", &filter]) {
                Ok(()) => {
                    self.rtx.filter_scale = Some(scale);
                    self.shader_status = ShaderStatus::Checking;
                }
                Err(_) => {
                    self.rtx.rejected =
                        Some("This mpv build could not start its Direct3D 11 RTX filter.");
                }
            }
        } else if self.number(c"video-out-params/w") > width
            && self.number(c"video-out-params/h") > height
        {
            self.shader_status = ShaderStatus::RtxRunning {
                scale: f64::from(scale) / 100.,
                driver_accepted: self.rtx.driver_accepted,
            };
        }
    }
}

fn supported_device(device: &str) -> bool {
    device.starts_with("NVIDIA ") && device.split_whitespace().any(|word| word == "RTX")
}

fn rtx_scale(width: f64, height: f64, target_width: f64, target_height: f64) -> u16 {
    // Fit the video area and cap intermediate textures at UHD dimensions.
    let scale = (target_width / width)
        .min(target_height / height)
        .min(3840. / width.max(height))
        .min(2160. / width.min(height))
        .clamp(1., 2.);
    (scale * 100.).floor() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtx_requires_a_supported_renderer_adapter() {
        assert!(supported_device("NVIDIA GeForce RTX 4080 SUPER"));
        assert!(supported_device("NVIDIA RTX A2000"));
        assert!(!supported_device("NVIDIA GeForce GTX 1660 Ti"));
        assert!(!supported_device("AMD Radeon RX 7900 XTX"));
        assert!(!supported_device("Microsoft Basic Render Driver"));
    }

    #[test]
    fn sizes_rtx_to_fit_the_video_area_without_overscaling_uhd() {
        assert_eq!(rtx_scale(1920., 1080., 2400., 1350.), 125);
        assert_eq!(rtx_scale(1920., 1080., 3840., 2160.), 200);
        assert_eq!(rtx_scale(3840., 2160., 3840., 2160.), 100);
        assert_eq!(rtx_scale(3840., 2160., 7680., 4320.), 100);
    }
}
