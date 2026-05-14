//! Boot watchdog for graphics settings that can crash WebKit before it
//! paints a single frame (HW acceleration, DMA-BUF, preferred-GPU
//! selection that picks an unreachable stack).
//!
//! Flow:
//!   1. `before_webkit_init()` is called at startup with the risky
//!      settings of the current boot. It writes a `pending.json` marker
//!      to `<data_dir>/qbz/boot_state/` recording which settings were
//!      attempted.
//!   2. After WebKit boots and the frontend signals first-paint (via
//!      `v2_mark_boot_succeeded`), `mark_boot_succeeded()` removes the
//!      pending marker and writes `last-good.json` carrying the
//!      successful settings.
//!   3. On the NEXT boot, `before_webkit_init` first calls
//!      `check_previous_boot()`. If a pending marker exists without a
//!      matching success, the previous boot crashed during graphics
//!      init — we auto-revert the offending setting(s), record a
//!      crash recovery flag, and only THEN write the new pending
//!      marker for the current boot.
//!
//! Two consecutive crashes with the same setting promote to "lockout"
//! state: the setting stays disabled until the user explicitly clears
//! the flag from Settings. Prevents an infinite retry loop.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const STATE_DIR: &str = "boot_state";
const PENDING_FILE: &str = "pending.json";
const LAST_GOOD_FILE: &str = "last-good.json";
const CRASH_FLAGS_FILE: &str = "crash-flags.json";

/// Snapshot of the risky settings attempted in a boot. Compared against
/// the previous boot's snapshot to decide which setting "caused" a crash.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BootAttempt {
    pub hardware_acceleration: bool,
    pub force_dmabuf: bool,
    pub preferred_gpu: String,
    pub force_x11: bool,
}

/// Sticky flags written when a setting was auto-reverted after a crash.
/// The UI reads these to render the recovery banner. Frontend can also
/// clear them via `v2_clear_crash_recovery_flag`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CrashFlags {
    /// Last boot crashed with hardware_acceleration ON; we reverted it.
    pub hardware_acceleration_disabled: bool,
    pub hardware_acceleration_consecutive_failures: u8,
    /// Same for force_dmabuf.
    pub force_dmabuf_disabled: bool,
    pub force_dmabuf_consecutive_failures: u8,
    /// Same for preferred_gpu (reset to "auto").
    pub preferred_gpu_disabled: bool,
    pub preferred_gpu_consecutive_failures: u8,
    /// Lockout — set when consecutive_failures >= 2 for a given knob.
    /// The runtime continues to revert; the UI shows a stronger
    /// "disabled due to repeated crashes" message and gates re-enable
    /// behind an explicit clear action.
    pub hardware_acceleration_locked: bool,
    pub force_dmabuf_locked: bool,
    pub preferred_gpu_locked: bool,
}

/// Result of `before_webkit_init`. The caller (main.rs) replaces its
/// own variables for hardware_accel / force_dmabuf / preferred_gpu
/// with these values before proceeding to apply env vars.
#[derive(Debug, Clone)]
pub struct WatchdogResolution {
    pub hardware_acceleration: bool,
    pub force_dmabuf: bool,
    pub preferred_gpu: String,
    /// Reverted-in-this-boot flags so the UI knows which setting was
    /// just rolled back. Persisted in CrashFlags as well.
    pub recovery_messages: Vec<String>,
}

fn state_dir() -> Option<PathBuf> {
    let base = dirs::data_dir()?.join("qbz").join(STATE_DIR);
    fs::create_dir_all(&base).ok()?;
    Some(base)
}

fn read_pending() -> Option<BootAttempt> {
    let path = state_dir()?.join(PENDING_FILE);
    let text = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_pending(attempt: &BootAttempt) {
    let Some(dir) = state_dir() else {
        return;
    };
    if let Ok(text) = serde_json::to_string_pretty(attempt) {
        let _ = fs::write(dir.join(PENDING_FILE), text);
    }
}

fn clear_pending() {
    if let Some(dir) = state_dir() {
        let _ = fs::remove_file(dir.join(PENDING_FILE));
    }
}

fn write_last_good(attempt: &BootAttempt) {
    let Some(dir) = state_dir() else {
        return;
    };
    if let Ok(text) = serde_json::to_string_pretty(attempt) {
        let _ = fs::write(dir.join(LAST_GOOD_FILE), text);
    }
}

fn read_crash_flags() -> CrashFlags {
    let Some(dir) = state_dir() else {
        return CrashFlags::default();
    };
    let path = dir.join(CRASH_FLAGS_FILE);
    let Ok(text) = fs::read_to_string(&path) else {
        return CrashFlags::default();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn write_crash_flags(flags: &CrashFlags) {
    let Some(dir) = state_dir() else {
        return;
    };
    if let Ok(text) = serde_json::to_string_pretty(flags) {
        let _ = fs::write(dir.join(CRASH_FLAGS_FILE), text);
    }
}

/// Called at startup with the settings the current boot WOULD apply if
/// nothing crashed. Returns the resolved settings to actually use —
/// usually the same input, but with risky knobs reverted if the
/// previous boot left a pending marker.
///
/// Also writes the new pending marker for THIS boot so a crash before
/// the frontend signals success is detectable on the next launch.
pub fn before_webkit_init(intended: BootAttempt) -> WatchdogResolution {
    let mut flags = read_crash_flags();
    let mut resolved = intended.clone();
    let mut messages: Vec<String> = Vec::new();

    // If a previous boot left a pending marker, it crashed before
    // first paint. Compare what it tried against what we'd try now,
    // and revert the field(s) that look like the cause.
    if let Some(prev) = read_pending() {
        // Hardware acceleration: if previous tried ON and we're also
        // trying ON, blame this knob and revert.
        if prev.hardware_acceleration && resolved.hardware_acceleration {
            flags.hardware_acceleration_consecutive_failures =
                flags.hardware_acceleration_consecutive_failures.saturating_add(1);
            resolved.hardware_acceleration = false;
            flags.hardware_acceleration_disabled = true;
            if flags.hardware_acceleration_consecutive_failures >= 2 {
                flags.hardware_acceleration_locked = true;
            }
            messages.push(
                "Hardware acceleration was disabled because the previous launch crashed during \
                 graphics init."
                    .to_string(),
            );
        }
        // Force DMA-BUF: same logic.
        if prev.force_dmabuf && resolved.force_dmabuf {
            flags.force_dmabuf_consecutive_failures =
                flags.force_dmabuf_consecutive_failures.saturating_add(1);
            resolved.force_dmabuf = false;
            flags.force_dmabuf_disabled = true;
            if flags.force_dmabuf_consecutive_failures >= 2 {
                flags.force_dmabuf_locked = true;
            }
            messages.push(
                "DMA-BUF renderer was disabled because the previous launch crashed during \
                 graphics init."
                    .to_string(),
            );
        }
        // Preferred GPU: only revert when the SAME non-auto choice
        // was attempted. Switching from one explicit GPU to another
        // shouldn't inherit a crash flag from the prior choice.
        if prev.preferred_gpu == resolved.preferred_gpu && resolved.preferred_gpu != "auto" {
            flags.preferred_gpu_consecutive_failures =
                flags.preferred_gpu_consecutive_failures.saturating_add(1);
            resolved.preferred_gpu = "auto".to_string();
            flags.preferred_gpu_disabled = true;
            if flags.preferred_gpu_consecutive_failures >= 2 {
                flags.preferred_gpu_locked = true;
            }
            messages.push(format!(
                "Rendering GPU preference was reset to Auto because pinning to {:?} crashed the \
                 previous launch.",
                prev.preferred_gpu
            ));
        }
        // If a previous attempt is present but none of the three
        // matched, the crash was caused by something else — leave the
        // flags as-is. The pending marker still gets cleared below.

        // Persist the revert decisions back to graphics_settings.db so
        // the user sees the new state in the UI and the next boot
        // doesn't try the same setting again.
        persist_reverts_to_db(&resolved, &intended);

        // Clear consecutive-failure counters for any knob that was
        // attempted AND succeeded (we infer "succeeded" from "this
        // boot's input matches last-good"). Handled in
        // mark_boot_succeeded for paths that actually paint.
    } else {
        // No pending marker → previous boot either succeeded or never
        // ran the watchdog. Reset disabled flags to non-sticky values
        // so the UI doesn't keep showing a banner across launches
        // where everything's fine. Locked flags STAY sticky — those
        // require explicit user clear.
        if !flags.hardware_acceleration_locked {
            flags.hardware_acceleration_disabled = false;
            flags.hardware_acceleration_consecutive_failures = 0;
        }
        if !flags.force_dmabuf_locked {
            flags.force_dmabuf_disabled = false;
            flags.force_dmabuf_consecutive_failures = 0;
        }
        if !flags.preferred_gpu_locked {
            flags.preferred_gpu_disabled = false;
            flags.preferred_gpu_consecutive_failures = 0;
        }
    }

    write_crash_flags(&flags);
    write_pending(&resolved);

    WatchdogResolution {
        hardware_acceleration: resolved.hardware_acceleration,
        force_dmabuf: resolved.force_dmabuf,
        preferred_gpu: resolved.preferred_gpu,
        recovery_messages: messages,
    }
}

/// Called from a Tauri command after the frontend signals first paint.
/// Removes the pending marker so the next launch doesn't think this
/// boot crashed; writes last-good as the new baseline; clears
/// consecutive-failure counters for whatever settings were active.
pub fn mark_boot_succeeded() {
    // Read what we attempted in this boot — the pending file is the
    // source of truth for "what was active when WebKit first paint
    // happened".
    let attempt = read_pending().unwrap_or_default();
    clear_pending();
    write_last_good(&attempt);

    let mut flags = read_crash_flags();
    // Successful paint with HW accel ON resets that knob's failure
    // streak entirely (including lockout — the issue resolved).
    if attempt.hardware_acceleration {
        flags.hardware_acceleration_consecutive_failures = 0;
        flags.hardware_acceleration_disabled = false;
        flags.hardware_acceleration_locked = false;
    }
    if attempt.force_dmabuf {
        flags.force_dmabuf_consecutive_failures = 0;
        flags.force_dmabuf_disabled = false;
        flags.force_dmabuf_locked = false;
    }
    if attempt.preferred_gpu != "auto" {
        flags.preferred_gpu_consecutive_failures = 0;
        flags.preferred_gpu_disabled = false;
        flags.preferred_gpu_locked = false;
    }
    write_crash_flags(&flags);
}

/// Read-only view of the crash flags for the Settings UI.
pub fn get_crash_flags() -> CrashFlags {
    read_crash_flags()
}

/// Clear a single crash recovery flag by name. Frontend uses this when
/// the user clicks "Try again" / "I want to re-enable this" on the
/// recovery banner. Unknown names are no-ops.
pub fn clear_crash_flag(flag: &str) {
    let mut flags = read_crash_flags();
    match flag {
        "hardware_acceleration" => {
            flags.hardware_acceleration_disabled = false;
            flags.hardware_acceleration_consecutive_failures = 0;
            flags.hardware_acceleration_locked = false;
        }
        "force_dmabuf" => {
            flags.force_dmabuf_disabled = false;
            flags.force_dmabuf_consecutive_failures = 0;
            flags.force_dmabuf_locked = false;
        }
        "preferred_gpu" => {
            flags.preferred_gpu_disabled = false;
            flags.preferred_gpu_consecutive_failures = 0;
            flags.preferred_gpu_locked = false;
        }
        _ => return,
    }
    write_crash_flags(&flags);
}

/// Best-effort write of reverted values back to graphics_settings.db
/// and developer_settings.db. Failure is non-fatal — the runtime
/// values for THIS boot are still the reverted ones (we pass them
/// back to main.rs via WatchdogResolution), and the next boot's read
/// will see the reverted DB rows too.
fn persist_reverts_to_db(resolved: &BootAttempt, intended: &BootAttempt) {
    if resolved.hardware_acceleration != intended.hardware_acceleration {
        if let Ok(store) = crate::config::graphics_settings::GraphicsSettingsStore::new() {
            let _ = store.set_hardware_acceleration(resolved.hardware_acceleration);
        }
    }
    if resolved.force_dmabuf != intended.force_dmabuf {
        if let Ok(store) = crate::config::developer_settings::DeveloperSettingsStore::new() {
            let _ = store.set_force_dmabuf(resolved.force_dmabuf);
        }
    }
    if resolved.preferred_gpu != intended.preferred_gpu {
        if let Ok(store) = crate::config::graphics_settings::GraphicsSettingsStore::new() {
            let _ = store.set_preferred_gpu(&resolved.preferred_gpu);
        }
    }
}
