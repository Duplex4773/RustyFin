use super::*;
use windows::Win32::UI::Input::XboxController::*;

const BUTTONS: &[(XINPUT_GAMEPAD_BUTTON_FLAGS, &str)] = &[
    (XINPUT_GAMEPAD_DPAD_UP, "up"),
    (XINPUT_GAMEPAD_DPAD_DOWN, "down"),
    (XINPUT_GAMEPAD_DPAD_LEFT, "left"),
    (XINPUT_GAMEPAD_DPAD_RIGHT, "right"),
    (XINPUT_GAMEPAD_A, "enter"),
    (XINPUT_GAMEPAD_B, "escape"),
    (XINPUT_GAMEPAD_LEFT_SHOULDER, "shift-tab"),
    (XINPUT_GAMEPAD_RIGHT_SHOULDER, "tab"),
    (XINPUT_GAMEPAD_START, "space"),
    (XINPUT_GAMEPAD_Y, "settings"),
    (XINPUT_GAMEPAD_X, "preview"),
];

fn buttons(state: &XINPUT_GAMEPAD) -> u16 {
    let mut bits = state.wButtons.0;
    // The left stick and D-pad send the same navigation actions.
    for (pressed, button) in [
        (state.sThumbLX < -18000, XINPUT_GAMEPAD_DPAD_LEFT),
        (state.sThumbLX > 18000, XINPUT_GAMEPAD_DPAD_RIGHT),
        (state.sThumbLY < -18000, XINPUT_GAMEPAD_DPAD_DOWN),
        (state.sThumbLY > 18000, XINPUT_GAMEPAD_DPAD_UP),
    ] {
        if pressed {
            bits |= button.0;
        }
    }
    bits
}

pub(super) fn listen(w: &Window, cx: &mut Context<Library>) {
    cx.spawn_in(w, async move |view, cx| {
        let mut delay = Duration::from_secs(1);
        let mut previous = 0;
        let mut held = Instant::now();
        let mut repeat = Instant::now();
        let mut controller = None;
        loop {
            cx.background_executor().timer(delay).await;
            let Ok(active) = cx.update(|w, _| w.is_window_active()) else {
                break;
            };
            if !active {
                delay = Duration::from_millis(500);
                previous = 0;
                continue;
            }
            let mut state = XINPUT_STATE::default();
            controller = controller
                .filter(|i| unsafe { XInputGetState(*i, &mut state) } == 0)
                .or_else(|| (0..4).find(|&i| unsafe { XInputGetState(i, &mut state) } == 0));
            if controller.is_none() {
                previous = 0;
                delay = Duration::from_secs(1);
                continue;
            }
            delay = Duration::from_millis(45);
            let now = Instant::now();
            let current = buttons(&state.Gamepad);
            let mut keys = current & !previous;
            if current != previous {
                held = now;
                repeat = now;
            } else if now.duration_since(held) >= Duration::from_millis(400)
                && now.duration_since(repeat) >= Duration::from_millis(115)
            {
                keys = current & 0x000f;
                repeat = now;
            }
            previous = current;
            if keys == 0 {
                continue;
            }
            if cx
                .update(|w, cx| {
                    for &(button, name) in BUTTONS {
                        if keys & button.0 == 0 {
                            continue;
                        }
                        let handled = view.update(cx, |s, cx| {
                            if let Some(p) = s.player.as_mut().filter(|_| !s.controls.mini) {
                                p.use_keyboard_controls();
                                cx.notify();
                            }
                            if s.server.is_some()
                                && matches!(s.state.screen, Screen::Settings(_))
                                && (s.player.is_none() || s.controls.mini)
                                && !s.navigation_focus.contains_focused(w, cx)
                                && s.rebind.is_none()
                            {
                                if s.settings_key(name, w, cx) {
                                    return true;
                                }
                                if name == "escape" {
                                    s.back(w, cx);
                                    return true;
                                }
                            }
                            s.controller_search_key(name, w, cx)
                        });
                        if handled.unwrap_or(false) {
                            continue;
                        }
                        match name {
                            "settings" | "preview" => {
                                let _ = view.update(cx, |s, cx| {
                                    if s.player.is_some() && !s.controls.mini {
                                        if name == "settings" {
                                            s.toggle_player_menu(PlayerMenu::Settings, w, cx);
                                        } else {
                                            s.start_preview(w, cx);
                                        }
                                    } else if name == "preview" && s.popup_menu.is_none() {
                                        s.focused_continue_actions(w, cx);
                                    }
                                });
                            }
                            "tab" | "shift-tab" => {
                                let _ = view.update(cx, |s, cx| {
                                    if s.player.is_some() && !s.controls.mini {
                                        s.step_player_focus(name == "tab", w, cx);
                                    } else if name == "tab" {
                                        w.focus_next();
                                    } else {
                                        w.focus_prev();
                                    }
                                });
                            }
                            "enter" | "space" => {
                                // GPUI activates a focused button on key-up. Deliver the pair to
                                // RustyFin's own window so its ordinary keyboard path handles it.
                                use windows::Win32::{
                                    Foundation::{LPARAM, WPARAM},
                                    UI::WindowsAndMessaging::{PostMessageW, WM_KEYDOWN, WM_KEYUP},
                                };
                                let key = if name == "enter" { 0x0d } else { 0x20 };
                                if let Ok(hwnd) = crate::player::parent(w) {
                                    unsafe {
                                        let _ = PostMessageW(
                                            Some(hwnd),
                                            WM_KEYDOWN,
                                            WPARAM(key),
                                            LPARAM(1),
                                        );
                                        let _ = PostMessageW(
                                            Some(hwnd),
                                            WM_KEYUP,
                                            WPARAM(key),
                                            LPARAM(0xc0000001),
                                        );
                                    }
                                }
                            }
                            other => {
                                if let Ok(key) = Keystroke::parse(other) {
                                    w.dispatch_keystroke(key, cx);
                                }
                            }
                        }
                    }
                })
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
}
