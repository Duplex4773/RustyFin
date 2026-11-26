#![windows_subsystem = "windows"]

mod anilist;
mod credentials;
mod jellyfin;
mod navigation_icons;
mod player;
mod preferences;
mod seerr;
mod services;
mod ui;

use gpui::{prelude::*, *};
use gpui_component::{Root, Theme, ThemeMode};

fn main() -> anyhow::Result<()> {
    if let Some(arg) = std::env::args_os().nth(1) {
        // Explorer launches the player without a console. Diagnostic commands
        // can still write to the terminal that invoked them.
        unsafe {
            use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }
        match arg.to_str() {
            Some("--diagnostics") => {
                return player::diagnostics(std::env::args_os().nth(2), false, false);
            }
            Some("--probe-output") => {
                return player::diagnostics(std::env::args_os().nth(2), true, false);
            }
            Some("--probe-mel") => {
                return player::diagnostics(std::env::args_os().nth(2), false, true);
            }
            Some("--probe-mel-output") => {
                return player::diagnostics(std::env::args_os().nth(2), true, true);
            }
            _ => anyhow::bail!(
                "Usage: rustyfin [--diagnostics [FILE] | --probe-output FILE | --probe-mel FILE | --probe-mel-output FILE]"
            ),
        }
    }
    Application::new()
        .with_assets(navigation_icons::NavigationIcons)
        .run(|cx| {
            gpui_component::init(cx);
            ui::bind_player_keys(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            // Browsing pages paint their own background. Leave the root of GPUI's
            // topmost composition layer clear so it can overlay the video child.
            Theme::global_mut(cx).background = transparent_black();
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let bounds = Bounds::centered(None, size(px(1080.), px(720.)), cx);
            cx.open_window(
                WindowOptions {
                    show: false,
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(650.), px(460.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("RustyFin".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let library = cx.new(|cx| ui::Library::new(window, cx));
                    cx.new(|cx| Root::new(library, window, cx))
                },
            )
            .expect("opening the RustyFin window")
            .update(cx, |_, window, _| window.activate_window())
            .expect("showing the RustyFin window");
            cx.activate(true);
        });
    Ok(())
}
