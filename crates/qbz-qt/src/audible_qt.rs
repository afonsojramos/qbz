//! THE audible step — design 02 §9 stage 3.
//!
//! One exhaustive `match` over [`PlaybackTicket`], and it is the ONLY place
//! this port enters the PROTECTED audio path for a row a source owns. Before
//! this module there were three near-copies of it, and they had already
//! drifted apart:
//!
//! | copy | Plex arm | CUE fast path | offline row id |
//! |---|---|---|---|
//! | `local_playback::play_audible` | `plex_rating_key` ladder | by track **id** | queue id (the Qobuz id — wrong row) |
//! | `local_album_actions::play_audible` | same ladder, copy-pasted | none | same |
//! | `local_ephemeral::play_file` | n/a | by file **path** | n/a |
//!
//! Three copies, three behaviours, and every one of them a `match` on
//! `track.source` — the shape design 02 §0 catalogues as the defect the seam
//! exists to remove. `qbz_source::SourceRegistry::playback` claims the row
//! ONCE and answers with a ticket; this file performs it.
//!
//! **What stays here and cannot move into `qbz-source`:** every call below
//! that touches `runtime.core().player()`. That crate deliberately does not
//! link `qbz-player` / `qbz-audio` (design 02 §8), so it can describe what to
//! play and never how the samples are handled. Nothing in this file touches
//! sample rate, resampling or device selection — the bytes go to the same
//! `play_data` / `play_dsd_file` seams the Slint frontend uses, so bit depth
//! and rate stay whatever the decoder found.
//!
//! ## The CUE fast path is now exact
//!
//! Every virtual track of a CUE album shares ONE audio file, so moving between
//! them should seek, not re-read a 300 MB container. The two old copies tested
//! for that differently and BOTH were incomplete:
//!
//! - `local_playback` compared the loaded **track id** to the one being
//!   played. Each virtual track is its own `local_tracks` row with its own id,
//!   so that test only passed when re-playing the *same* virtual track — i.e.
//!   never for the case the fast path exists to serve.
//! - `local_ephemeral` compared **file paths**, which is the right question,
//!   but it could only answer it for rows in the session store.
//!
//! [`LAST_LOADED`] records what the last successful `play_data` actually read.
//! The test becomes "does the player still hold audio, is it still the load I
//! made, and was that load this same file?" — correct for library CUE albums
//! and ephemeral ones alike, with no source lookup on the hot path.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_library::Reach;
use qbz_models::QueueTrack;
use qbz_source::{PlaybackTicket, SourceError};

type Runtime = Arc<AppRuntime<LoggingAdapter>>;

/// What the last successful `play_data` in this process loaded: the play id it
/// was given, and the file it read.
///
/// This is a fact about THIS module's own behaviour, so it is owned here
/// rather than asked of the player — `player().state` knows the id it is
/// playing, never the path it came from.
static LAST_LOADED: Mutex<Option<(u64, PathBuf)>> = Mutex::new(None);

fn remember_loaded(play_id: u64, path: &Path) {
    if let Ok(mut slot) = LAST_LOADED.lock() {
        *slot = Some((play_id, path.to_path_buf()));
    }
}

/// Is `path` the container the player is playing right now?
fn container_is_loaded(runtime: &Runtime, path: &Path) -> bool {
    if !runtime.core().player().has_loaded_audio() {
        return false;
    }
    let loaded = runtime.core().player().state.current_track_id();
    matches!(
        LAST_LOADED.lock().ok().and_then(|s| s.clone()),
        Some((id, ref p)) if id == loaded && p == path
    )
}

/// Claim `track`, ask its source how to play it, and play it.
///
/// `Ok(true)` — playing. `Ok(false)` — the source answered, the player
/// refused (a dead share, an unreadable file); the caller decides whether that
/// is skippable. `Err` — nobody claimed the row, or the source could not
/// resolve it.
///
/// [`PlaybackTicket::Catalog`] is returned as `Err(Unsupported)` on purpose:
/// this is the LOCAL audible step, and the Qobuz tier-walk is a different
/// entry (`playback_qt::play_resolved_offline_aware`) with its own offline
/// cache, quality and resume handling. Silently doing "something" for a
/// catalog row here is how a queue ends up with two playback policies.
pub(crate) async fn play_queue_track(
    runtime: &Runtime,
    track: &QueueTrack,
) -> Result<bool, SourceError> {
    let ticket = qbz_source::registry().playback(track).await?;
    Ok(play_ticket(runtime, ticket).await)
}

/// Perform a ticket. The `match` is exhaustive by design: [`PlaybackTicket`]
/// is deliberately not `#[non_exhaustive]`, so a variant added later breaks
/// THIS function until somebody decides what it means.
pub(crate) async fn play_ticket(runtime: &Runtime, ticket: PlaybackTicket) -> bool {
    match ticket {
        PlaybackTicket::DsdFile { path, play_id } => {
            // DSD stays on its additive path, streamed from disk by the
            // player rather than slurped into RAM.
            if let Err(e) = runtime.core().player().play_dsd_file(path, play_id) {
                log::error!("[qbz-qt] audible: play_dsd_file {play_id} failed: {e}");
                return false;
            }
            true
        }

        PlaybackTicket::File {
            path,
            play_id,
            seek_secs,
        } => play_file(runtime, path, play_id, seek_secs).await,

        PlaybackTicket::Bytes { bytes, play_id } => {
            let len = bytes.len();
            if let Err(e) = runtime.core().player().play_data(bytes, play_id) {
                log::error!("[qbz-qt] audible: play_data {play_id} ({len} B) failed: {e}");
                return false;
            }
            // NOT a file load, so it must not be remembered as one — a stale
            // path here would let a later CUE seek skip a real read.
            if let Ok(mut slot) = LAST_LOADED.lock() {
                *slot = None;
            }
            true
        }

        PlaybackTicket::Stream {
            url,
            play_id,
            duration_secs,
            start_secs,
            log_tag,
        } => play_stream(runtime, &url, play_id, duration_secs, start_secs, log_tag).await,

        PlaybackTicket::SeekLoaded { play_id, secs } => {
            let _ = runtime.core().player().seek(secs as u64);
            log::debug!("[qbz-qt] audible: seek-in-loaded {play_id} -> {secs}s");
            true
        }

        PlaybackTicket::Catalog { track_id } => {
            // Reachable only if a source starts emitting it into this step.
            // Say so instead of playing nothing quietly.
            log::error!(
                "[qbz-qt] audible: catalog ticket for {track_id} reached the local audible step — \
                 it belongs to playback_qt::play_resolved_offline_aware"
            );
            false
        }
    }
}

/// Read a file and hand the bytes to the player, with the CUE fast path in
/// front of it.
async fn play_file(
    runtime: &Runtime,
    path: PathBuf,
    play_id: u64,
    seek_secs: Option<f64>,
) -> bool {
    // CUE fast path — see the module header for why this test changed shape.
    if let Some(start) = seek_secs.filter(|s| *s > 0.0) {
        if container_is_loaded(runtime, &path) {
            let _ = runtime.core().player().seek(start as u64);
            return true;
        }
    }

    // BOUNDED PROBE, then an unbounded read — and the split is the whole
    // point. A mounted-but-unreachable share (the user is on a different
    // network today) does not make `exists()` fail, it makes it BLOCK for the
    // mount's timeout, so this await could never return: playing one dead file
    // wedged playback with no way out. The probe now stops WAITING.
    //
    // The READ deliberately keeps no deadline. A hi-res FLAC over a
    // working-but-slow share legitimately takes longer than any probe budget,
    // and timing the transfer out would turn "your network is slow today" into
    // "this track is gone" — worse than the bug being fixed.
    let read_path = path.clone();
    let (reach, bytes) = tokio::task::spawn_blocking(move || {
        let reach = qbz_library::probe_default(&read_path);
        let bytes = if reach == Reach::Present {
            std::fs::read(&read_path).ok()
        } else {
            None
        };
        (reach, bytes)
    })
    .await
    .unwrap_or((Reach::Unreachable, None));

    let Some(bytes) = bytes else {
        // Missing and Unreachable are NOT the same answer and must not be
        // logged as one. `Missing` is the filesystem saying the file is gone —
        // a caller may clean on it. `Unreachable` is the filesystem saying
        // NOTHING; the file may be perfectly fine on a share this network
        // cannot see, so it may only ever be SKIPPED.
        match reach {
            Reach::Unreachable => log::warn!(
                "[qbz-qt] audible: {} did not answer — share unreachable from this network; \
                 skipping, NOT removing",
                path.display()
            ),
            _ => log::error!(
                "[qbz-qt] audible: file not available at {} (drive unmounted?)",
                path.display()
            ),
        }
        return false;
    };

    if let Err(e) = runtime.core().player().play_data(bytes, play_id) {
        log::error!("[qbz-qt] audible: play_data {play_id} failed: {e}");
        return false;
    }
    remember_loaded(play_id, &path);

    if let Some(start) = seek_secs.filter(|s| *s > 0.0) {
        tokio::time::sleep(std::time::Duration::from_millis(120)).await;
        let _ = runtime.core().player().seek(start as u64);
    }
    true
}

/// Range-stream the original bytes into the player, falling back to a whole-body
/// GET of the SAME url when the feeder cannot start.
///
/// The fallback is what `local_playback::play_plex_track` did, minus its second
/// metadata round trip: the ticket already carries the url that round trip was
/// there to rebuild.
async fn play_stream(
    runtime: &Runtime,
    url: &str,
    play_id: u64,
    duration_secs: u64,
    start_secs: u64,
    log_tag: &'static str,
) -> bool {
    let t0 = std::time::Instant::now();
    // The url carries the server's token in its query string. `origin` is what
    // may be logged: it answers the question these lines exist for — whether
    // the transfer is going to a LAN address or being relayed through the
    // vendor's servers, which is the difference between "slow because it is a
    // big file" and "slow because it left the building".
    let origin = url.split('?').next().unwrap_or("").to_string();

    match qbz_player::remote_stream::stream_remote_track_into_player(
        &runtime.core().player(),
        play_id,
        duration_secs,
        start_secs,
        url,
        log_tag,
    )
    .await
    {
        Ok(()) => {
            log::info!(
                "[qbz-qt][perf] {log_tag} play {play_id}: STREAMED — first audio {:?} — {origin}",
                t0.elapsed(),
            );
            if let Ok(mut slot) = LAST_LOADED.lock() {
                *slot = None;
            }
            return true;
        }
        // A server that refuses Range, or a part that will not probe, still
        // plays — just slowly.
        Err(e) => log::warn!(
            "[qbz-qt] {log_tag} play {play_id}: streaming failed ({e}) — \
             falling back to whole-file download"
        ),
    }

    let bytes = match fetch_body(url).await {
        Ok(b) => b,
        Err(e) => {
            log::error!("[qbz-qt] {log_tag} play {play_id}: download failed ({e}) — {origin}");
            return false;
        }
    };
    let fetched = t0.elapsed();
    let len = bytes.len();
    if let Err(e) = runtime.core().player().play_data(bytes, play_id) {
        log::error!("[qbz-qt] {log_tag} play: play_data {play_id} failed: {e}");
        return false;
    }
    log::info!(
        "[qbz-qt][perf] {log_tag} play {play_id}: DOWNLOADED {len} bytes in {fetched:?} \
         ({:.1} MB/s) — {origin}",
        (len as f64 / 1_048_576.0) / fetched.as_secs_f64().max(0.001),
    );
    if let Ok(mut slot) = LAST_LOADED.lock() {
        *slot = None;
    }
    true
}

/// GET the whole body.
///
/// `error_for_status` is NOT optional here, and it is not enough on its own for
/// every backend this will serve: a Subsonic server answers a REFUSED request
/// with **HTTP 200** and a ~200-byte JSON error envelope (measured against
/// Navidrome 0.63.2, 2026-08-20). Handing that to `play_data` would push a JSON
/// blob into the decoder. So the body is also checked for a plausible audio
/// length before it is allowed anywhere near the player.
async fn fetch_body(url: &str) -> Result<Vec<u8>, String> {
    let resp = reqwest::get(url)
        .await
        .map_err(|e| format!("request failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("status error: {e}"))?;
    let ctype = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ctype.starts_with("application/json") || ctype.starts_with("text/") {
        return Err(format!(
            "server answered with {ctype} instead of audio — this is an error envelope, not a track"
        ));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("read failed: {e}"))?
        .to_vec();
    // Below any real track and above every error envelope seen.
    if bytes.len() < 4096 {
        return Err(format!("body is only {} bytes — not a track", bytes.len()));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The CUE fast path may only fire for the file it actually loaded. A stale
    /// entry pointing at another container is how a seek silently plays the
    /// wrong audio.
    #[test]
    fn last_loaded_matches_on_both_the_id_and_the_path() {
        remember_loaded(7, Path::new("/m/album.flac"));
        let slot = LAST_LOADED.lock().unwrap().clone().unwrap();
        assert_eq!(slot.0, 7);
        assert_eq!(slot.1, PathBuf::from("/m/album.flac"));

        // Same container, a DIFFERENT virtual track: the id moves, the path
        // does not. This is the case the old id-only test could never see.
        remember_loaded(8, Path::new("/m/album.flac"));
        let slot = LAST_LOADED.lock().unwrap().clone().unwrap();
        assert_eq!(slot.0, 8);
        assert_eq!(slot.1, PathBuf::from("/m/album.flac"));
    }
}
