//! Local output-device quality cap (#638 fix 3, spec Phase C).
//!
//! Caches the local DAC's detected rate ceiling, mapped to a Qobuz tier, so
//! the request-time resolution (`playback::local_playback_quality`) can clamp
//! the streaming-quality preference without probing per call. Detection is
//! the proven read-only `qbz_audio::query_dac_capabilities` (reads
//! `/proc/asound` and shells out to `pw-dump`; never opens a stream), so a
//! refresh runs inside `spawn_blocking` on EXPLICIT triggers only — startup,
//! the Settings toggle, an output-device/backend change, reset-to-defaults —
//! never on the playback hot path or the poll tick. A stale cap after a
//! hot-unplug (until the next device change), and an uncapped first track
//! when a session-restore play beats the startup refresh, are the accepted
//! trades — same class as the HiFi wizard's behavior; both self-heal.
//!
//! PRECEDENCE (owner decision, #638): the cap of the device ACTUALLY PLAYING
//! governs. This cache is for LOCAL playback only — the cast path must never
//! read it (the local DAC is not in a cast's signal path).

use std::sync::RwLock;

use qbz_models::Quality;

/// The cached cap. `tier` is the coarse Qobuz-tier mapping of the detected
/// ceiling (`max_rate_hz`); `detected` false = the probe fell back to the
/// common rate set, so the Settings caveat must disclose that the cap may
/// not match the hardware (owner Decision B: it still applies).
#[derive(Clone)]
pub struct CapState {
    pub tier: Quality,
    pub detected: bool,
    pub max_rate_hz: u32,
    pub description: String,
}

/// None = the cap is disabled (toggle off) or not refreshed yet.
static CAP: RwLock<Option<CapState>> = RwLock::new(None);

/// Map a detected max rate to the tier we may REQUEST (spec C.3). Coarse by
/// design: Qobuz sells four discrete tiers and no 48 kHz tier exists, so a
/// 48 kHz (or 44.1 kHz) ceiling steps down to CD 16/44.1 — bit depth
/// included; the Settings summary says it plainly instead of letting the
/// user discover it. > 96 kHz keeps Hi-Res+ = no effective cap (still
/// cached so Settings can display what was detected).
fn tier_for_max_rate_hz(max_hz: u32) -> Quality {
    if max_hz > 96_000 {
        Quality::UltraHiRes
    } else if max_hz >= 88_200 {
        Quality::HiRes
    } else {
        Quality::Lossless
    }
}

/// Cheap read for the request-time resolution: `(tier, detected)`.
/// None = no cap configured.
pub fn cap() -> Option<(Quality, bool)> {
    CAP.read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|c| (c.tier, c.detected))
}

/// The Settings "Detected device limit" value line: `(summary, detected)`,
/// e.g. `("192 kHz · Hi-Res+", true)`. Untranslated data composition — the
/// same convention as the quality badge's "24-bit / 96 kHz" (tier names are
/// product names). `("", true)` when no cap is active: the row hides on the
/// empty summary, and `true` keeps the fallback caveat from flashing before
/// the first refresh lands.
pub fn summary() -> (String, bool) {
    match CAP.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        Some(c) => (
            format!(
                "{} · {}",
                rate_khz_label(c.max_rate_hz),
                tier_display(c.tier)
            ),
            c.detected,
        ),
        None => (String::new(), true),
    }
}

/// Product-name tier label for the summary line. The CD entry spells out the
/// bit-depth cost (spec C.3: no 48 kHz tier exists, so the step below Hi-Res
/// loses depth too — say it, don't let the user discover it).
fn tier_display(tier: Quality) -> &'static str {
    match tier {
        Quality::UltraHiRes => "Hi-Res+",
        Quality::HiRes => "Hi-Res",
        Quality::Lossless => "CD 16-bit / 44.1 kHz",
        // Unreachable from tier_for_max_rate_hz; total match for safety.
        Quality::Mp3 => "MP3 320",
    }
}

/// "192 kHz" / "44.1 kHz" from Hz — integer when whole, one decimal
/// otherwise (matches `crate::quality::detail`'s rate formatting).
fn rate_khz_label(hz: u32) -> String {
    let khz = hz as f64 / 1000.0;
    if khz.fract().abs() < f64::EPSILON {
        format!("{} kHz", khz as i64)
    } else {
        format!("{khz} kHz")
    }
}

/// Re-run detection and refresh the cache. `limit_enabled` off clears it
/// immediately (no probe). The probe runs in `spawn_blocking` (pw-dump
/// subprocess + /proc reads — never on the UI thread). Await-able so the
/// Settings controller can re-push the summary row right after.
pub async fn refresh(limit_enabled: bool, output_device: Option<String>) {
    if !limit_enabled {
        *CAP.write().unwrap_or_else(|e| e.into_inner()) = None;
        log::info!("[qbz-slint] device cap: disabled");
        return;
    }
    let probed = tokio::task::spawn_blocking(move || {
        // The configured device id, or the system-default sink when the
        // selection is "System default" (None). An unresolvable default
        // probes with an empty node name, which lands on the fallback set →
        // detected=false → Hi-Res+ no-op cap with the caveat disclosed.
        let node = output_device.unwrap_or_else(default_output_node);
        qbz_audio::query_dac_capabilities(&node)
    })
    .await;
    let caps = match probed {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[qbz-slint] device cap: probe task failed: {e}");
            return;
        }
    };
    let max_rate_hz = caps.sample_rates.iter().copied().max().unwrap_or(0);
    // assemble() always yields a non-empty rate list (fallback set), but
    // never store a 0 Hz cap if that invariant ever breaks.
    if max_rate_hz == 0 {
        *CAP.write().unwrap_or_else(|e| e.into_inner()) = None;
        return;
    }
    let state = CapState {
        tier: tier_for_max_rate_hz(max_rate_hz),
        detected: caps.detected,
        max_rate_hz,
        description: caps.description.unwrap_or_else(|| caps.node_name.clone()),
    };
    log::info!(
        "[qbz-slint] device cap: {} -> max {} Hz -> {:?} ({})",
        state.description,
        state.max_rate_hz,
        state.tier,
        if state.detected {
            "detected"
        } else {
            "fallback set"
        },
    );
    *CAP.write().unwrap_or_else(|e| e.into_inner()) = Some(state);
}

/// The system-default PipeWire sink's node id, for the "System default"
/// device selection. Empty when nothing enumerates (no PipeWire) — the probe
/// then reports the fallback set honestly.
fn default_output_node() -> String {
    qbz_audio::backend::BackendManager::create_backend(
        qbz_audio::backend::AudioBackendType::PipeWire,
    )
    .ok()
    .and_then(|b| b.enumerate_devices().ok())
    .and_then(|devs| devs.into_iter().find(|d| d.is_default))
    .map(|d| d.id)
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_mapping_matches_spec_c3_table() {
        // > 96 kHz → Hi-Res+ (no effective cap).
        assert_eq!(tier_for_max_rate_hz(192_000), Quality::UltraHiRes);
        assert_eq!(tier_for_max_rate_hz(176_400), Quality::UltraHiRes);
        // 96 / 88.2 kHz → Hi-Res.
        assert_eq!(tier_for_max_rate_hz(96_000), Quality::HiRes);
        assert_eq!(tier_for_max_rate_hz(88_200), Quality::HiRes);
        // ≤ 48 kHz → CD 16/44.1 (bit depth lost too — no 48 kHz tier).
        assert_eq!(tier_for_max_rate_hz(48_000), Quality::Lossless);
        assert_eq!(tier_for_max_rate_hz(44_100), Quality::Lossless);
    }

    #[test]
    fn rate_label_formats_whole_and_fractional_khz() {
        assert_eq!(rate_khz_label(192_000), "192 kHz");
        assert_eq!(rate_khz_label(44_100), "44.1 kHz");
        assert_eq!(rate_khz_label(176_400), "176.4 kHz");
    }
}
