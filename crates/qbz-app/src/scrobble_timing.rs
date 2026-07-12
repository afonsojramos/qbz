//! Pure scrobble arming rules (no UI / network).
//!
//! Shared so the Slint shell and unit tests agree on when a delayed scrobble
//! may fire. Unknown duration (`0`) must never arm an immediate scrobble.

/// `min(50% of duration, 240s)` in seconds — the Last.fm rule, applied to both
/// Last.fm and ListenBrainz.
///
/// Returns `None` when duration is unknown (`0`): callers must **not** scrobble
/// immediately — that would pollute history for local/Plex metadata holes.
pub fn scrobble_delay_secs(duration_secs: u64) -> Option<u64> {
    if duration_secs == 0 {
        return None;
    }
    Some((duration_secs / 2).min(240))
}

#[cfg(test)]
mod tests {
    use super::scrobble_delay_secs;

    #[test]
    fn unknown_duration_skips_scrobble() {
        assert_eq!(scrobble_delay_secs(0), None);
    }

    #[test]
    fn half_duration_capped_at_240() {
        assert_eq!(scrobble_delay_secs(100), Some(50));
        assert_eq!(scrobble_delay_secs(600), Some(240));
        // 1s track: half truncates to 0 — still Some, so we arm (immediate)
        // rather than skip (duration is known, just very short).
        assert_eq!(scrobble_delay_secs(1), Some(0));
    }
}
