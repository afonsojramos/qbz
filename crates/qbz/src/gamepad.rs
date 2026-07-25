//! Native gamepad input — Steam Deck plan PR 3
//! (qbz-nix-docs/steam-deck/STEAM_DECK_GAME_MODE_PLAN.md §5).
//!
//! Steam Input's default template for non-Steam shortcuts exposes a VIRTUAL
//! Xbox 360 pad (uinput), not keyboard events — and QBZ reads neither by
//! default. `gilrs` (pure Rust, evdev) reads that pad and translates
//! D-pad / left stick / A / B into synthetic Slint key events through the
//! same `dispatch_event` pipeline as the winit synthetic-key hook
//! (main.rs install_browser_mouse_nav). PR 2's kiosk FocusScope consumes
//! them as ordinary arrow/Enter/Escape presses; any USB/BT pad on a HTPC
//! gets the same ride.
//!
//! The thread is spawned only when the kiosk profile is active or
//! `QBZ_GAMEPAD=1` forces it (see the call site in main.rs).

use std::time::{Duration, Instant};

use gilrs::{Axis, Button, EventType, Gilrs};
use slint::platform::{Key, WindowEvent};
use slint::ComponentHandle;

use crate::AppWindow;

/// Axis→direction hysteresis: engage when |axis| crosses the high threshold,
/// release below the low one (classic 10-foot anti-jitter; stick drift near
/// center must cause zero ghost navigation).
const AXIS_ENGAGE: f32 = 0.6;
const AXIS_RELEASE: f32 = 0.4;
/// Key repeat while a direction is held: initial delay, then interval.
const REPEAT_DELAY: Duration = Duration::from_millis(250);
const REPEAT_INTERVAL: Duration = Duration::from_millis(80);
/// Double-input guard for a user-selected keyboard-EMULATING Steam layout:
/// the same nav action can then arrive from Steam's kbd hook AND from gilrs —
/// collapse repeats of the same key inside this window.
const DEBOUNCE: Duration = Duration::from_millis(30);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dir {
    Up,
    Down,
    Left,
    Right,
}

impl Dir {
    fn key(self) -> Key {
        match self {
            Dir::Up => Key::UpArrow,
            Dir::Down => Key::DownArrow,
            Dir::Left => Key::LeftArrow,
            Dir::Right => Key::RightArrow,
        }
    }
}

/// Spawn the gamepad listener thread (no-op log on failure — gamepad input
/// is a convenience, never fatal).
pub fn spawn(weak: slint::Weak<AppWindow>) {
    if let Err(e) = std::thread::Builder::new()
        .name("qbz-gamepad".into())
        .spawn(move || run(weak))
    {
        log::warn!("[gamepad] could not spawn thread: {e}");
    }
}

/// Press+release `key` as one synthetic tap on the UI thread, debounced.
/// A Return/Escape tap is a single action; arrows are also emitted by the
/// repeat engine. While a text field is focused, Escape is SUPPRESSED (B
/// on the pad must close Steam's OSK without also navigating back — plan
/// §4.5) and Return passes through (the OSK's own accept).
fn emit_tap(weak: &slint::Weak<AppWindow>, key: Key, last: &mut Option<(Key, Instant)>) {
    let now = Instant::now();
    if let Some((prev, at)) = last {
        if *prev == key && now.duration_since(*at) < DEBOUNCE {
            return;
        }
    }
    *last = Some((key, now));
    let _ = weak.upgrade_in_event_loop(move |w| {
        if key == Key::Escape
            && w.global::<crate::UiFocusState>().get_text_input_focused()
        {
            return;
        }
        let text: slint::SharedString = key.into();
        w.window().dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        w.window().dispatch_event(WindowEvent::KeyReleased { text });
    });
}

fn run(weak: slint::Weak<AppWindow>) {
    let mut gilrs = match Gilrs::new() {
        Ok(g) => g,
        Err(e) => {
            log::warn!("[gamepad] gilrs init failed (no evdev access?): {e}");
            return;
        }
    };
    let pads = gilrs.gamepads().count();
    log::info!("[gamepad] listening ({pads} pad(s) connected)");

    // Held direction for repeat: (dir, engaged_at, last_emit). One at a
    // time — the newest direction wins, like any 10-foot UI.
    let mut held: Option<(Dir, Instant, Instant)> = None;
    let mut last_emit: Option<(Key, Instant)> = None;

    loop {
        // Block until the next pad event, waking at most by the repeat tick
        // so a held direction keeps firing with no pad traffic.
        let wake = held.map(|(_, _, last_emit_at)| {
            REPEAT_INTERVAL.saturating_sub(last_emit_at.elapsed())
        });
        let event = match wake {
            Some(t) => gilrs.next_event_blocking(Some(t)),
            None => gilrs.next_event_blocking(None),
        };
        let now = Instant::now();

        match event.map(|e| e.event) {
            Some(EventType::ButtonPressed(Button::South, _)) => {
                emit_tap(&weak, Key::Return, &mut last_emit);
            }
            Some(EventType::ButtonPressed(Button::East, _)) => {
                emit_tap(&weak, Key::Escape, &mut last_emit);
            }
            Some(EventType::ButtonPressed(btn, _)) => {
                if let Some(dir) = dpad_dir(btn) {
                    emit_tap(&weak, dir.key(), &mut last_emit);
                    held = Some((dir, now, now));
                }
            }
            Some(EventType::ButtonReleased(btn, _)) => {
                if let Some(dir) = dpad_dir(btn) {
                    if held.map(|(d, _, _)| d) == Some(dir) {
                        held = None;
                    }
                }
            }
            Some(EventType::AxisChanged(axis, value, _)) => {
                let (neg, pos) = match axis {
                    Axis::LeftStickX => (Dir::Left, Dir::Right),
                    Axis::LeftStickY => (Dir::Up, Dir::Down),
                    _ => {
                        continue;
                    }
                };
                if value.abs() >= AXIS_ENGAGE {
                    let dir = if value < 0.0 { neg } else { pos };
                    if held.map(|(d, _, _)| d) != Some(dir) {
                        emit_tap(&weak, dir.key(), &mut last_emit);
                        held = Some((dir, now, now));
                    }
                } else if value.abs() < AXIS_RELEASE {
                    // Only disengage when the held direction lives on THIS axis.
                    if let Some((d, _, _)) = held {
                        if d == neg || d == pos {
                            held = None;
                        }
                    }
                }
            }
            Some(_) => {}
            // Timeout wake (or a spurious None): fire the repeat engine.
            None => {}
        }

        // Repeat: a held direction re-emits after the initial delay, then on
        // every interval.
        if let Some((dir, engaged_at, last_emit_at)) = held.as_mut() {
            if engaged_at.elapsed() >= REPEAT_DELAY
                && last_emit_at.elapsed() >= REPEAT_INTERVAL
            {
                let key = dir.key();
                emit_tap(&weak, key, &mut last_emit);
                *last_emit_at = Instant::now();
            }
        }
    }
}

fn dpad_dir(btn: Button) -> Option<Dir> {
    match btn {
        Button::DPadUp => Some(Dir::Up),
        Button::DPadDown => Some(Dir::Down),
        Button::DPadLeft => Some(Dir::Left),
        Button::DPadRight => Some(Dir::Right),
        _ => None,
    }
}
