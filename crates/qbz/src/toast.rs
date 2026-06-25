//! In-app toast controller — the Rust side of the Slint port of Tauri's
//! `toastStore`. One toast at a time (a new one replaces the current),
//! auto-hidden after a per-kind delay. `buffering` is persistent.
//!
//! All functions touch Slint globals and the shared hide `Timer`, so they
//! must run on the event-loop thread. From background threads / async
//! tasks use [`show_weak`] / [`error_weak`], which hop onto the event loop
//! for you (the established `upgrade_in_event_loop` pattern).
//!
//! This is the shared toast API surface; not every helper has a call site
//! yet (error surfacing is being wired in incrementally), so the unused
//! ones are intentionally allowed rather than removed.
#![allow(dead_code)]

use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode, Weak};

use crate::{AppWindow, ToastKind, ToastState};

thread_local! {
    // Single reusable auto-hide timer. Reused (stop + start) so a fresh
    // toast cancels the previous toast's pending hide instead of letting
    // it fire and dismiss the newer one early — mirrors the
    // `clearTimeout` in Tauri's toastStore.
    static HIDE_TIMER: Timer = Timer::default();
}

/// Auto-hide delay per kind. `None` = persistent (buffering). Matches
/// Tauri's `defaultDurations`.
fn auto_hide(kind: ToastKind) -> Option<Duration> {
    let ms = match kind {
        ToastKind::Success => 3000,
        ToastKind::Info => 3000,
        ToastKind::Warning => 4000,
        ToastKind::Error => 5000,
        ToastKind::Buffering => return None,
    };
    Some(Duration::from_millis(ms))
}

/// Show a toast. Must be called on the event-loop thread.
pub fn show(window: &AppWindow, message: impl Into<String>, kind: ToastKind) {
    let message: String = message.into();
    let state = window.global::<ToastState>();

    // Non-error toasts honor the global toggle; errors always show
    // (parity with Tauri's `showToast`).
    if !state.get_enabled() && kind != ToastKind::Error {
        return;
    }

    state.set_message(message.as_str().into());
    state.set_kind(kind);
    state.set_persistent(kind == ToastKind::Buffering);
    state.set_visible(true);

    HIDE_TIMER.with(|t| t.stop());
    if let Some(delay) = auto_hide(kind) {
        let weak = window.as_weak();
        HIDE_TIMER.with(|t| {
            t.start(TimerMode::SingleShot, delay, move || {
                if let Some(w) = weak.upgrade() {
                    w.global::<ToastState>().set_visible(false);
                }
            });
        });
    }
}

/// Hide the current toast immediately (also stops the pending auto-hide).
pub fn hide(window: &AppWindow) {
    HIDE_TIMER.with(|t| t.stop());
    window.global::<ToastState>().set_visible(false);
}

/// Show a toast from any thread (async / background error paths). Hops
/// onto the event loop, so the message must be owned/`Send`.
pub fn show_weak(weak: &Weak<AppWindow>, message: impl Into<String>, kind: ToastKind) {
    let message: String = message.into();
    let _ = weak.upgrade_in_event_loop(move |w| show(&w, message, kind));
}

// ---- Convenience wrappers -------------------------------------------------

pub fn error(window: &AppWindow, message: impl Into<String>) {
    show(window, message, ToastKind::Error);
}

pub fn success(window: &AppWindow, message: impl Into<String>) {
    show(window, message, ToastKind::Success);
}

pub fn info(window: &AppWindow, message: impl Into<String>) {
    show(window, message, ToastKind::Info);
}

pub fn warning(window: &AppWindow, message: impl Into<String>) {
    show(window, message, ToastKind::Warning);
}

/// Error toast from any thread.
pub fn error_weak(weak: &Weak<AppWindow>, message: impl Into<String>) {
    show_weak(weak, message, ToastKind::Error);
}

/// Success toast from any thread.
pub fn success_weak(weak: &Weak<AppWindow>, message: impl Into<String>) {
    show_weak(weak, message, ToastKind::Success);
}

/// Info toast from any thread.
pub fn info_weak(weak: &Weak<AppWindow>, message: impl Into<String>) {
    show_weak(weak, message, ToastKind::Info);
}
