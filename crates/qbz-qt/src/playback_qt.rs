//! Minimal playback controller — drives `QbzCore`'s EXISTING public API
//! only (qbz-audio / qbz-player are protected; no audio logic is
//! reimplemented here). Ports the *shape* of `crates/qbz/src/playback.rs`:
//! play_album (fetch -> QueueTrack vec -> set_queue -> play_track_resolved)
//! and the poll loop (PlaybackEvent -> UI state, track-change meta refresh,
//! end-of-track advance).
//!
//! POC-NOTEs (deliberate cuts vs playback.rs, named for the effort report):
//! - Streaming quality: the ui_prefs DEFAULT ("hires_plus" ->
//!   `Quality::UltraHiRes`). The ui_prefs store and the #638 device-cap
//!   clamp live in the Slint glue; there is no prefs UI in the POC.
//! - Blacklist filtering of album tracks: skipped (store not open).
//! - Offline-cache tier (offline bytes), gapless prefetch (`play_next`),
//!   prefetch warming, stop-after, infinite refill, session persist,
//!   QConnect/cast branches, recently-played recording: all out of scope.
//!   Auto-advance therefore plays each successor at fetch time (audible
//!   gap between tracks) — the engine's gapless path is NOT wired.
//! - `advance_to_playable` (skip-unavailable walk): the POC advance takes
//!   the core queue's next directly; a failed play logs and stops.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_models::{Quality, QueueTrack, RepeatMode};

/// The request tier for every play: the ui_prefs default ("hires_plus").
/// See module docs (no prefs UI in the POC).
const POC_QUALITY: Quality = Quality::UltraHiRes;

// Mute bookkeeping (mirrors playback.rs MUTED / PREMUTE_VOLUME: there is no
// dedicated mute API on the core — mute is volume 0 with a stash).
static MUTED: AtomicBool = AtomicBool::new(false);
static PREMUTE_VOLUME: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Play an album (album-card click on Home)
// ---------------------------------------------------------------------------

/// Fetch the album, build the queue (playback.rs `make_queue_track` port),
/// set it starting at track 1, and play through the core's resolved path.
async fn fetch_album_queue(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
) -> Result<Vec<QueueTrack>, String> {
    let album = runtime
        .core()
        .get_album(album_id)
        .await
        .map_err(|e| format!("get_album {album_id} failed: {e}"))?;

    let album_title = album.title.clone();
    let album_artist = album.artist.name.clone();
    let album_artwork = album.image.best().cloned().unwrap_or_default();
    let raw_tracks = album
        .tracks
        .as_ref()
        .map(|container| container.items.as_slice())
        .unwrap_or_default();
    if raw_tracks.is_empty() {
        return Err(format!("album {album_id} has no playable tracks"));
    }
    let tracks: Vec<QueueTrack> = raw_tracks
        .iter()
        .map(|track| QueueTrack {
            id: track.id,
            title: track.title.clone(),
            version: track.version.clone(),
            artist: track
                .performer
                .as_ref()
                .map(|p| p.name.clone())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| album_artist.clone()),
            album: album_title.clone(),
            album_version: album
                .version
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string),
            duration_secs: track.duration as u64,
            artwork_url: if album_artwork.is_empty() {
                None
            } else {
                Some(album_artwork.clone())
            },
            hires: track.hires,
            bit_depth: track.maximum_bit_depth,
            sample_rate: track.maximum_sampling_rate,
            is_local: false,
            album_id: Some(album.id.clone()),
            artist_id: track.performer.as_ref().map(|p| p.id),
            streamable: track.streamable,
            source: Some("qobuz".to_string()),
            parental_warning: track.parental_warning,
            source_item_id_hint: Some(album.id.clone()),
            context_kind: Some("album".to_string()),
            context_id: Some(album.id.clone()),
        })
        .collect();
    Ok(tracks)
}

/// "Play next" / "Add to queue" for an album (AlbumCard ⋯ menu): resolve
/// the album's tracks and insert them after the current track (mode
/// "next") or append them (mode "later").
pub async fn enqueue_album(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
    mode: &str,
) -> Result<(), String> {
    let tracks = fetch_album_queue(runtime, album_id).await?;
    log::info!("[qbz-qt] enqueue_album {album_id} ({mode}): {} tracks", tracks.len());
    if mode == "next" {
        // add_track_next inserts directly after the current track — feed
        // in reverse so the album's first track lands first.
        for track in tracks.into_iter().rev() {
            runtime.core().add_track_next(track).await;
        }
    } else {
        runtime.core().add_tracks(tracks).await;
    }
    publish_queue(runtime).await;
    Ok(())
}

pub async fn play_album(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
) -> Result<(), String> {
    log::info!("[qbz-qt] play_album: resolving {album_id}");
    let tracks = fetch_album_queue(runtime, album_id).await?;
    let first_id = tracks[0].id;
    let count = tracks.len();
    runtime.core().set_queue(tracks, Some(0)).await;
    publish_queue(runtime).await;
    log::info!("[qbz-qt] play_album: queue set ({count} tracks), playing track {first_id}");
    runtime
        .core()
        .play_track_resolved(first_id, POC_QUALITY, None, None, 0)
        .await
        .map_err(|e| format!("play_track {first_id} failed: {e}"))?;
    log::info!("[qbz-qt] play_album: play_track_resolved started for {first_id}");
    refresh_now_playing(runtime).await;
    Ok(())
}

/// Play a single track as a one-element queue (Library track rows). The
/// queue meta is rebuilt from the Library feed row; the audio resolves by
/// id through the same `play_track_resolved` path.
pub async fn play_single_track(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    track_id: u64,
) -> Result<(), String> {
    let id = track_id.to_string();
    let item = crate::library_qt::with_library(|d| {
        d.feed
            .iter()
            .find(|i| i.kind == "track" && i.id == id)
            .cloned()
    })
    .flatten()
    .ok_or_else(|| format!("track {track_id} not in the library feed"))?;
    let duration_secs = {
        let mut parts = item.duration.split(':');
        parts.next().and_then(|m| m.parse::<u64>().ok()).unwrap_or(0) * 60
            + parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0)
    };
    let qt = QueueTrack {
        id: track_id,
        title: item.title.clone(),
        version: None,
        artist: item.artist.clone(),
        album: item.album.clone(),
        album_version: None,
        duration_secs,
        artwork_url: if item.image_url.is_empty() {
            None
        } else {
            Some(item.image_url.clone())
        },
        hires: item.quality_tier == "hires",
        bit_depth: None,
        sample_rate: None,
        is_local: false,
        album_id: if item.album_id.is_empty() {
            None
        } else {
            Some(item.album_id.clone())
        },
        artist_id: item.artist_id.parse::<u64>().ok(),
        streamable: true,
        source: Some("qobuz".to_string()),
        parental_warning: item.explicit,
        source_item_id_hint: None,
        context_kind: None,
        context_id: None,
    };
    runtime.core().set_queue(vec![qt], Some(0)).await;
    publish_queue(runtime).await;
    log::info!("[qbz-qt] play_single_track: playing {track_id}");
    runtime
        .core()
        .play_track_resolved(track_id, POC_QUALITY, None, None, 0)
        .await
        .map_err(|e| format!("play_track {track_id} failed: {e}"))?;
    refresh_now_playing(runtime).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

pub async fn toggle_play(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let event = runtime.core().player().get_playback_event();
    let result = if event.is_playing {
        runtime.core().pause()
    } else {
        runtime.core().resume()
    };
    if let Err(e) = result {
        log::warn!("[qbz-qt] toggle-play failed: {e}");
    }
}

pub async fn next(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    if let Some(track) = runtime.core().next_track().await {
        play_queue_track(runtime, track.id).await;
    }
}

pub async fn previous(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    if let Some(track) = runtime.core().previous_track().await {
        play_queue_track(runtime, track.id).await;
    }
}

async fn play_queue_track(runtime: &Arc<AppRuntime<LoggingAdapter>>, track_id: u64) {
    if let Err(e) = runtime
        .core()
        .play_track_resolved(track_id, POC_QUALITY, None, None, 0)
        .await
    {
        log::error!("[qbz-qt] playback: play_track {track_id} failed: {e}");
        return;
    }
    refresh_now_playing(runtime).await;
    publish_queue(runtime).await;
}

pub async fn seek_frac(runtime: &Arc<AppRuntime<LoggingAdapter>>, frac: f32) {
    let event = runtime.core().player().get_playback_event();
    if event.duration == 0 {
        return;
    }
    let target = (frac.clamp(0.0, 1.0) * event.duration as f32) as u64;
    if let Err(e) = runtime.core().seek(target) {
        log::warn!("[qbz-qt] seek failed: {e}");
    }
}

pub async fn set_volume(runtime: &Arc<AppRuntime<LoggingAdapter>>, volume: f32) {
    let _ = runtime.core().set_volume(volume.clamp(0.0, 1.0));
    MUTED.store(volume <= 0.0 && PREMUTE_VOLUME.load(Ordering::Relaxed) != 0, Ordering::Relaxed);
}

pub async fn toggle_mute(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    if MUTED.swap(false, Ordering::Relaxed) {
        // Unmute — restore the stored level.
        let restored = f32::from_bits(PREMUTE_VOLUME.load(Ordering::Relaxed) as u32);
        let restored = if restored > 0.0 { restored } else { 0.7 };
        let _ = runtime.core().set_volume(restored);
        crate::now_playing::set_muted(false);
    } else {
        // Mute — stash the current level, then drop to zero.
        let current = runtime.core().player().get_playback_event().volume;
        let current = if current > 0.0 { current } else { 0.7 };
        PREMUTE_VOLUME.store(current.to_bits() as u64, Ordering::Relaxed);
        MUTED.store(true, Ordering::Relaxed);
        let _ = runtime.core().set_volume(0.0);
        crate::now_playing::set_muted(true);
    }
}

pub async fn toggle_shuffle(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let enabled = runtime.core().toggle_shuffle().await;
    crate::now_playing::set_shuffle(enabled);
    publish_queue(runtime).await;
}

pub async fn cycle_repeat(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let next_mode = match crate::now_playing::repeat_mode() {
        0 => RepeatMode::All,
        1 => RepeatMode::One,
        _ => RepeatMode::Off,
    };
    runtime.core().set_repeat_mode(next_mode).await;
    let value = match next_mode {
        RepeatMode::Off => 0,
        RepeatMode::All => 1,
        RepeatMode::One => 2,
    };
    crate::now_playing::set_repeat_mode(value);
}

// ---------------------------------------------------------------------------
// Meta + queue publishing
// ---------------------------------------------------------------------------

/// Push the queue's current-track meta into the NowPlayingModel (title /
/// artist / quality / artwork via the phase-3 pipeline).
pub async fn refresh_now_playing(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let state = runtime.core().get_queue_state().await;
    let Some(track) = state.current_track else {
        crate::now_playing::clear_track();
        return;
    };
    let title = match track.version.as_deref().filter(|v| !v.is_empty()) {
        Some(version) => format!("{} ({version})", track.title),
        None => track.title.clone(),
    };
    log::info!(
        "[qbz-qt] now playing: '{title}' — {} ({}s)",
        track.artist,
        track.duration_secs,
    );
    let (tier, label) = quality_badge(&track);
    crate::now_playing::set_track(crate::now_playing::TrackMeta {
        title,
        artist: track.artist.clone(),
        duration_secs: track.duration_secs as i32,
        quality_tier: tier,
        quality_label: label,
        artwork_url: track.artwork_url.clone().unwrap_or_default(),
        shuffle: state.shuffle,
        repeat_mode: match state.repeat {
            RepeatMode::Off => 0,
            RepeatMode::All => 1,
            RepeatMode::One => 2,
        },
    });
    // Artwork through the same cache pipeline as Home (attach + background
    // download + republish — single url here).
    crate::artwork_qt::attach_now_playing(&track.artwork_url.clone().unwrap_or_default());
}

/// Quality tier + exact label from a queue track's bit depth / sample rate
/// (kHz float, catalog), matching the discover card badge format.
fn quality_badge(track: &QueueTrack) -> (String, String) {
    let tier = match track.bit_depth {
        Some(d) if d >= 24 => "hires",
        Some(_) => "cd",
        None if track.hires => "hires",
        None => "",
    }
    .to_string();
    let label = match (track.bit_depth, track.sample_rate) {
        (Some(bd), Some(sr)) => format!("{}-bit / {} kHz", bd, crate::home_qt::format_rate(sr)),
        _ => String::new(),
    };
    (tier, label)
}

/// Publish the real queue into `queueModel` (now-playing row first, then
/// up-next rows) as one QVariant per row.
///
/// POC-NOTE: rows are JSON-encoded strings inside the QVariants — the same
/// cxx-qt-lib nesting limitation as homeSectionsJson (a QVariant cannot
/// hold a QVariantMap in 0.7.3); QML parses each row.
pub async fn publish_queue(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let state = runtime.core().get_queue_state().await;
    #[derive(serde::Serialize)]
    struct QueueRow {
        id: String,
        title: String,
        artist: String,
        duration: String,
        #[serde(rename = "artPath")]
        art_path: String,
        current: bool,
    }
    let mut rows: Vec<QueueRow> = Vec::new();
    let mut push = |track: &QueueTrack, current: bool| {
        rows.push(QueueRow {
            id: track.id.to_string(),
            title: track.title.clone(),
            artist: track.artist.clone(),
            duration: fmt_mmss(track.duration_secs),
            art_path: crate::artwork_qt::cached_path(
                &track.artwork_url.clone().unwrap_or_default(),
            ),
            current,
        });
    };
    if let Some(current) = state.current_track.as_ref() {
        push(current, true);
    }
    for track in &state.upcoming {
        push(track, false);
    }
    let json_rows: Vec<String> = rows
        .iter()
        .map(|r| serde_json::to_string(r).unwrap_or_default())
        .collect();
    crate::ui(move |mut b| {
        b.as_mut()
            .set_queue_model(crate::json_rows_to_qvariant_list(json_rows));
    });
}

fn fmt_mmss(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

// ---------------------------------------------------------------------------
// 1 Hz state pump (playback.rs `start_poll_loop`, minimized)
// ---------------------------------------------------------------------------

static POLL_STARTED: AtomicBool = AtomicBool::new(false);

/// Start the 1 Hz pump on shell entry (idempotent). Publishes position /
/// playing / cache into the NowPlayingModel, detects track changes (meta +
/// queue refresh), and advances the queue at end-of-track through the core.
pub fn start_poll_loop(runtime: Arc<AppRuntime<LoggingAdapter>>) {
    if POLL_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    crate::spawn(async move {
        let mut last_track_id: u64 = 0;
        let mut was_playing = false;
        let mut seen_position: u64 = 0;
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            ticker.tick().await;

            // Surface audio-stream failures once (playback.rs drains the same
            // slot for its toast).
            if let Some(msg) = runtime.core().player().state.take_stream_error_message() {
                log::error!("[qbz-qt] audio output error: {msg}");
            }

            let event = runtime.core().player().get_playback_event();
            let track_id = event.track_id;
            let position = event.position;
            let duration = event.duration;
            let is_playing = event.is_playing;
            let cache = event.buffer_progress.unwrap_or(0.0);

            // --- Track-change edge (new current track surfaced) ----------
            if track_id != 0 && track_id != last_track_id {
                // Reconcile the queue pointer (a real advance moves it; a
                // manual play already did) and refresh meta + queue.
                let _ = runtime.core().sync_current_to_id(track_id).await;
                refresh_now_playing(&runtime).await;
                publish_queue(&runtime).await;
                last_track_id = track_id;
            }

            if track_id != 0 {
                log::debug!("[qbz-qt] poll: id={track_id} pos={position}/{duration} playing={is_playing} cache={cache:.2}");
            }

            // --- Position / state push ------------------------------------
            crate::now_playing::set_position(
                position as i32,
                duration as i32,
                is_playing,
                cache,
                track_id != 0,
            );

            // --- End-of-track edge (playback.rs condition, verbatim) ------
            let track_ended = was_playing
                && !is_playing
                && last_track_id != 0
                && (track_id == 0 || track_id == last_track_id)
                && duration > 0
                && seen_position + 2 >= duration;
            if track_ended {
                last_track_id = 0;
                // POC-NOTE: no stop-after / infinite-refill / skip-unavailable
                // walk — the core queue's own next (repeat/shuffle aware).
                if let Some(track) = runtime.core().next_track().await {
                    let next_id = track.id;
                    log::info!("[qbz-qt] poll: advancing to {next_id}");
                    play_queue_track(&runtime, next_id).await;
                } else {
                    log::info!("[qbz-qt] poll: queue finished");
                    crate::now_playing::set_playing(false);
                }
            }

            seen_position = position;
            was_playing = is_playing;
        }
    });
}
