//! Sleep timer (queue footer).
//!
//! Frontend-only behaviour, 1:1 with the Tauri `sleepTimerStore`: a single armed
//! deadline that pauses playback when it elapses, plus a 1 Hz countdown. Ported to
//! a Rust-owned `tokio` task driving a monotonic `Instant` deadline (robust to
//! laptop suspend / clock changes, unlike the Tauri wall-clock `setTimeout`).
//! Nothing is persisted — an armed timer is lost on restart (deliberate).
//!
//! A process-wide generation counter invalidates an in-flight task on cancel or
//! re-arm, so there is never a stale timer that pauses playback unexpectedly.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use slint::ComponentHandle;

use crate::adapter::SlintAdapter;
use crate::{AppWindow, SleepTimerState};

type Runtime = Arc<qbz_app::shell::AppRuntime<SlintAdapter>>;

/// Monotonically increasing token. Each `set`/`cancel` bumps it; a running task
/// keeps the value it was spawned with and exits as soon as it no longer matches.
static GENERATION: AtomicU64 = AtomicU64::new(0);

const MIN_MINUTES: i32 = 1;
const MAX_MINUTES: i32 = 24 * 60; // 1440

fn push_state(weak: &slint::Weak<AppWindow>, active: bool, remaining: String) {
    let _ = weak.upgrade_in_event_loop(move |w| {
        let st = w.global::<SleepTimerState>();
        st.set_active(active);
        st.set_remaining(remaining.into());
    });
}

/// Arm (or re-arm) the sleep timer for `minutes` (clamped to [1, 1440]). Replaces
/// any running timer. At expiry it pauses playback (if anything is playing) and
/// returns to idle.
pub fn set(runtime: Runtime, weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle, minutes: i32) {
    if minutes <= 0 {
        return;
    }
    let minutes = minutes.clamp(MIN_MINUTES, MAX_MINUTES);
    // Bump the generation; this task owns `my_gen` until the next set/cancel.
    let my_gen = GENERATION.fetch_add(1, Ordering::SeqCst).wrapping_add(1);

    handle.spawn(async move {
        let deadline = Instant::now() + Duration::from_secs((minutes as u64) * 60);
        // Immediate feedback: armed + initial countdown.
        push_state(&weak, true, qbz_text_utils::format_sleep_remaining((minutes as i64) * 60));

        let mut ticker = tokio::time::interval(Duration::from_secs(1));
        loop {
            ticker.tick().await; // first tick fires immediately, then every 1s
            if GENERATION.load(Ordering::SeqCst) != my_gen {
                return; // cancelled or superseded
            }
            let now = Instant::now();
            if now >= deadline {
                if runtime.core().get_playback_state().is_playing {
                    if let Err(e) = runtime.core().pause() {
                        log::warn!("[qbz-slint] sleep-timer: pause on expiry failed: {e}");
                    }
                }
                push_state(&weak, false, String::new());
                return;
            }
            let remaining = (deadline - now).as_secs() as i64;
            push_state(&weak, true, qbz_text_utils::format_sleep_remaining(remaining));
        }
    });
}

/// Cancel any armed timer and return to idle.
pub fn cancel(weak: slint::Weak<AppWindow>) {
    GENERATION.fetch_add(1, Ordering::SeqCst); // invalidate the running task
    push_state(&weak, false, String::new());
}
