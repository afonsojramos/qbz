//! Playback / queue controller.
//!
//! Owns the orchestration between the UI and `QbzCore`'s player + queue.
//! Albums and tracks are turned into a `Vec<QueueTrack>`, handed to the
//! core's `QueueManager`, and then played audibly through
//! `Player::play_track` (the self-contained "fetch URL → download → play"
//! path — the protected bit-perfect audio backend is untouched).
//!
//! There is no event stream from the player, so a `tokio` poll task reads
//! `Player::get_playback_event()` a few times a second and pushes the
//! values onto the `NowPlayingState` global. The same task drives
//! auto-advance when a track ends.

use std::sync::Arc;

use qbz_app::shell::AppRuntime;
use qbz_models::{Quality, QueueTrack, RepeatMode};
use slint::{ComponentHandle, Model};

use crate::adapter::SlintAdapter;
use crate::{AppWindow, NowPlayingState, QueueItem, QueueState};

/// Streaming quality used for all playback in the MVP — highest tier,
/// the player falls back internally when it is not available.
const PLAYBACK_QUALITY: Quality = Quality::UltraHiRes;

/// Convenience alias for the runtime handle threaded through every call.
type Runtime = Arc<AppRuntime<SlintAdapter>>;

/// Run the audible step for `track_id`: grab the Qobuz client and call
/// the player's self-contained `play_track`. Errors are logged, not
/// surfaced — the poll loop keeps the UI consistent regardless.
async fn play_audible(runtime: &Runtime, track_id: u64) {
    let client_lock = runtime.core().client();
    let guard = client_lock.read().await;
    let Some(client) = guard.as_ref() else {
        log::error!("[qbz-slint] playback: no Qobuz client — cannot start audio");
        return;
    };
    let player = runtime.core().player();
    if let Err(e) = player.play_track(client, track_id, PLAYBACK_QUALITY).await {
        log::error!("[qbz-slint] playback: play_track {track_id} failed: {e}");
    }
}

/// Plain, `Send` queue-row data produced on the worker thread. The
/// `QueueItem` (which holds a non-`Send` `slint::Image`) is built from
/// this inside the event-loop closure.
struct QueueRowData {
    id: String,
    title: String,
    artist: String,
    artwork_url: String,
    playing: bool,
}

/// Push the current queue snapshot onto the `QueueState` global. Builds
/// the ordered list (history → current → upcoming) so the sidebar shows
/// the whole queue with the playing row marked.
async fn refresh_queue(runtime: &Runtime, weak: &slint::Weak<AppWindow>) {
    let state = runtime.core().get_queue_state().await;

    let mut rows: Vec<QueueRowData> = Vec::new();
    let mut current_index: i32 = -1;

    for track in &state.history {
        rows.push(queue_row(track, false));
    }
    if let Some(current) = state.current_track.as_ref() {
        current_index = rows.len() as i32;
        rows.push(queue_row(current, true));
    }
    for track in &state.upcoming {
        rows.push(queue_row(track, false));
    }

    // Collect the cover jobs before the rows are moved into the closure.
    let art_jobs: Vec<(usize, String)> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| !row.artwork_url.is_empty())
        .map(|(idx, row)| (idx, row.artwork_url.clone()))
        .collect();

    let _ = weak.upgrade_in_event_loop(move |w| {
        let items: Vec<QueueItem> = rows
            .into_iter()
            .map(|row| QueueItem {
                id: row.id.into(),
                title: row.title.into(),
                artist: row.artist.into(),
                artwork: slint::Image::default(),
                playing: row.playing,
            })
            .collect();
        let qs = w.global::<QueueState>();
        qs.set_items(slint::ModelRc::new(slint::VecModel::from(items)));
        qs.set_current_index(current_index);
    });

    load_queue_artwork(weak.clone(), art_jobs);
}

/// Map a `QueueTrack` to plain `Send` row data.
fn queue_row(track: &QueueTrack, playing: bool) -> QueueRowData {
    let title = match track.version.as_deref().filter(|v| !v.is_empty()) {
        Some(version) => format!("{} ({version})", track.title),
        None => track.title.clone(),
    };
    QueueRowData {
        id: track.id.to_string(),
        title,
        artist: track.artist.clone(),
        artwork_url: track.artwork_url.clone().unwrap_or_default(),
        playing,
    }
}

/// Resolve cover art for a list of queue rows and apply each decoded
/// image onto its `QueueState.items` row by index. Spawns one task per
/// cover; misses are silently skipped.
fn load_queue_artwork(weak: slint::Weak<AppWindow>, jobs: Vec<(usize, String)>) {
    let Some(cache) = crate::artwork::shared_cache() else {
        return;
    };
    for (idx, url) in jobs {
        let weak = weak.clone();
        let cache = cache.clone();
        tokio::spawn(async move {
            let Some((pixels, w, h)) =
                crate::artwork::fetch_and_decode(&url, &cache, 96).await
            else {
                return;
            };
            let _ = weak.upgrade_in_event_loop(move |win| {
                let items = win.global::<QueueState>().get_items();
                if let Some(mut item) = items.row_data(idx) {
                    item.artwork = crate::artwork::pixels_to_image(&pixels, w, h);
                    items.set_row_data(idx, item);
                }
            });
        });
    }
}

/// Resolve the now-playing cover and apply it to `NowPlayingState`.
fn load_now_playing_artwork(weak: slint::Weak<AppWindow>, url: String) {
    if url.is_empty() {
        return;
    }
    let Some(cache) = crate::artwork::shared_cache() else {
        return;
    };
    tokio::spawn(async move {
        let Some((pixels, w, h)) =
            crate::artwork::fetch_and_decode(&url, &cache, 160).await
        else {
            return;
        };
        let _ = weak.upgrade_in_event_loop(move |win| {
            let img = crate::artwork::pixels_to_image(&pixels, w, h);
            win.global::<NowPlayingState>().set_artwork(img);
        });
    });
}

/// `M:SS` for the elapsed string.
fn fmt_elapsed(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// `-M:SS` for the remaining string.
fn fmt_remaining(position: u64, duration: u64) -> String {
    let left = duration.saturating_sub(position);
    format!("-{}:{:02}", left / 60, left % 60)
}

/// Push the now-playing values for the current queue track onto
/// `NowPlayingState`. Called when a new track starts so the song card
/// updates immediately (the poll loop only refreshes position/progress).
async fn refresh_now_playing_meta(runtime: &Runtime, weak: &slint::Weak<AppWindow>) {
    let state = runtime.core().get_queue_state().await;
    let Some(track) = state.current_track else {
        let _ = weak.upgrade_in_event_loop(|w| {
            w.global::<NowPlayingState>().set_has_track(false);
        });
        return;
    };

    let title = match track.version.as_deref().filter(|v| !v.is_empty()) {
        Some(version) => format!("{} ({version})", track.title),
        None => track.title.clone(),
    };
    let artist = track.artist.clone();
    let album = track.album.clone();
    let album_id = track.album_id.clone().unwrap_or_default();
    let artist_id = track.artist_id.map(|id| id.to_string()).unwrap_or_default();
    let track_id = track.id.to_string();
    let duration = track.duration_secs;
    let artwork_url = track.artwork_url.clone().unwrap_or_default();

    let _ = weak.upgrade_in_event_loop(move |w| {
        let np = w.global::<NowPlayingState>();
        np.set_has_track(true);
        np.set_title(title.into());
        np.set_artist(artist.into());
        np.set_album(album.into());
        np.set_album_id(album_id.into());
        np.set_artist_id(artist_id.into());
        np.set_track_id(track_id.into());
        np.set_duration_secs(duration as i32);
        np.set_position_secs(0);
        np.set_progress(0.0);
        np.set_cache(0.0);
        np.set_elapsed("0:00".into());
        np.set_remaining(fmt_remaining(0, duration).into());
        np.set_playing(true);
        // Clear the previous cover so it does not linger while the new
        // one resolves.
        np.set_artwork(slint::Image::default());
    });

    load_now_playing_artwork(weak.clone(), artwork_url);
}

/// Build a `QueueTrack` for the queue from the catalog `Track`, filling
/// the album metadata from `album_meta` (the track's own album summary is
/// often partial in album responses).
fn make_queue_track(
    track: &qbz_models::Track,
    album_id: &str,
    album_title: &str,
    album_artist: &str,
    album_artwork: &str,
) -> QueueTrack {
    QueueTrack {
        id: track.id,
        title: track.title.clone(),
        version: track.version.clone(),
        artist: track
            .performer
            .as_ref()
            .map(|p| p.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| album_artist.to_string()),
        album: album_title.to_string(),
        duration_secs: track.duration as u64,
        artwork_url: if album_artwork.is_empty() {
            None
        } else {
            Some(album_artwork.to_string())
        },
        hires: track.hires,
        bit_depth: track.maximum_bit_depth,
        sample_rate: track.maximum_sampling_rate,
        is_local: false,
        album_id: Some(album_id.to_string()),
        artist_id: track.performer.as_ref().map(|p| p.id),
        streamable: track.streamable,
        source: Some("qobuz".to_string()),
        parental_warning: track.parental_warning,
        source_item_id_hint: Some(album_id.to_string()),
    }
}

/// Record the currently playing queue track in the recently-played store
/// so the Discover "Recently Played" sections fill.
async fn record_recent(runtime: &Runtime) {
    let state = runtime.core().get_queue_state().await;
    let Some(track) = state.current_track else {
        return;
    };
    let artwork = track.artwork_url.clone().unwrap_or_default();
    crate::recently::record(crate::recently::RecentTrack {
        id: track.id.to_string(),
        title: track.title.clone(),
        subtitle: track.artist.clone(),
        artwork_url: artwork.clone(),
        album_id: track.album_id.clone().unwrap_or_default(),
        album_title: track.album.clone(),
        album_artist: track.artist.clone(),
        album_artwork_url: artwork,
    });
}

/// Play `album_id` from `start_index`: fetch the album, build the queue,
/// hand it to the core, and start audio on the start track.
pub fn play_album(
    runtime: Runtime,
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    album_id: String,
    start_index: usize,
) {
    handle.spawn(async move {
        let album = match runtime.core().get_album(&album_id).await {
            Ok(album) => album,
            Err(e) => {
                log::error!("[qbz-slint] playback: get_album {album_id} failed: {e}");
                return;
            }
        };

        let album_title = album.title.clone();
        let album_artist = album.artist.name.clone();
        let album_artwork = album.image.best().cloned().unwrap_or_default();

        let tracks: Vec<QueueTrack> = album
            .tracks
            .as_ref()
            .map(|container| container.items.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|track| {
                make_queue_track(
                    track,
                    &album.id,
                    &album_title,
                    &album_artist,
                    &album_artwork,
                )
            })
            .collect();

        if tracks.is_empty() {
            log::warn!("[qbz-slint] playback: album {album_id} has no tracks");
            return;
        }

        let start = start_index.min(tracks.len() - 1);
        let start_track_id = tracks[start].id;

        runtime.core().set_queue(tracks, Some(start)).await;
        refresh_queue(&runtime, &weak).await;
        refresh_now_playing_meta(&runtime, &weak).await;
        record_recent(&runtime).await;
        play_audible(&runtime, start_track_id).await;
    });
}

/// Play a single track immediately as a one-track queue.
pub fn play_track_now(
    runtime: Runtime,
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    track_id: u64,
) {
    handle.spawn(async move {
        let track = match runtime.core().get_track(track_id).await {
            Ok(track) => track,
            Err(e) => {
                log::error!("[qbz-slint] playback: get_track {track_id} failed: {e}");
                return;
            }
        };

        let (album_id, album_title, album_artwork) = match track.album.as_ref() {
            Some(album) => (
                album.id.clone(),
                album.title.clone(),
                album.image.best().cloned().unwrap_or_default(),
            ),
            None => (String::new(), String::new(), String::new()),
        };
        let album_artist = track
            .performer
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default();

        let queue_track = make_queue_track(
            &track,
            &album_id,
            &album_title,
            &album_artist,
            &album_artwork,
        );

        runtime.core().set_queue(vec![queue_track], Some(0)).await;
        refresh_queue(&runtime, &weak).await;
        refresh_now_playing_meta(&runtime, &weak).await;
        record_recent(&runtime).await;
        play_audible(&runtime, track_id).await;
    });
}

/// Enqueue an album's tracks at the end of the current queue.
pub fn enqueue_album(runtime: Runtime, weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle, album_id: String) {
    handle.spawn(async move {
        let album = match runtime.core().get_album(&album_id).await {
            Ok(album) => album,
            Err(e) => {
                log::error!("[qbz-slint] playback: enqueue get_album {album_id} failed: {e}");
                return;
            }
        };
        let album_title = album.title.clone();
        let album_artist = album.artist.name.clone();
        let album_artwork = album.image.best().cloned().unwrap_or_default();
        let tracks: Vec<QueueTrack> = album
            .tracks
            .as_ref()
            .map(|container| container.items.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|track| {
                make_queue_track(track, &album.id, &album_title, &album_artist, &album_artwork)
            })
            .collect();
        if tracks.is_empty() {
            return;
        }
        runtime.core().add_tracks(tracks).await;
        refresh_queue(&runtime, &weak).await;
    });
}

/// Enqueue a single track at the end of the current queue.
pub fn enqueue_track(runtime: Runtime, weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle, track_id: u64) {
    handle.spawn(async move {
        let track = match runtime.core().get_track(track_id).await {
            Ok(track) => track,
            Err(e) => {
                log::error!("[qbz-slint] playback: enqueue get_track {track_id} failed: {e}");
                return;
            }
        };
        let (album_id, album_title, album_artwork) = match track.album.as_ref() {
            Some(album) => (
                album.id.clone(),
                album.title.clone(),
                album.image.best().cloned().unwrap_or_default(),
            ),
            None => (String::new(), String::new(), String::new()),
        };
        let album_artist = track
            .performer
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let queue_track =
            make_queue_track(&track, &album_id, &album_title, &album_artist, &album_artwork);
        runtime.core().add_track(queue_track).await;
        refresh_queue(&runtime, &weak).await;
    });
}

/// Jump to queue entry `index` and start audio for it.
pub fn play_queue_index(
    runtime: Runtime,
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    index: usize,
) {
    handle.spawn(async move {
        let Some(track) = runtime.core().play_index(index).await else {
            log::warn!("[qbz-slint] playback: play_index {index} out of range");
            return;
        };
        let track_id = track.id;
        refresh_queue(&runtime, &weak).await;
        refresh_now_playing_meta(&runtime, &weak).await;
        record_recent(&runtime).await;
        play_audible(&runtime, track_id).await;
    });
}

/// Toggle play / pause on the live player.
pub fn toggle_play_pause(runtime: Runtime, handle: tokio::runtime::Handle) {
    handle.spawn(async move {
        let playing = runtime.core().get_playback_state().is_playing;
        let result = if playing {
            runtime.core().pause()
        } else {
            runtime.core().resume()
        };
        if let Err(e) = result {
            log::error!("[qbz-slint] playback: toggle play/pause failed: {e}");
        }
    });
}

/// Advance to the next queue track and play it.
pub fn next(runtime: Runtime, weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    handle.spawn(async move {
        let Some(track) = runtime.core().next_track().await else {
            log::info!("[qbz-slint] playback: end of queue");
            return;
        };
        let track_id = track.id;
        refresh_queue(&runtime, &weak).await;
        refresh_now_playing_meta(&runtime, &weak).await;
        record_recent(&runtime).await;
        play_audible(&runtime, track_id).await;
    });
}

/// Go to the previous queue track and play it.
pub fn previous(runtime: Runtime, weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    handle.spawn(async move {
        let Some(track) = runtime.core().previous_track().await else {
            log::info!("[qbz-slint] playback: start of queue");
            return;
        };
        let track_id = track.id;
        refresh_queue(&runtime, &weak).await;
        refresh_now_playing_meta(&runtime, &weak).await;
        record_recent(&runtime).await;
        play_audible(&runtime, track_id).await;
    });
}

/// Seek to `fraction` (0..1) of the current track's duration.
pub fn seek(runtime: Runtime, handle: tokio::runtime::Handle, fraction: f32) {
    handle.spawn(async move {
        let state = runtime.core().get_playback_state();
        if state.duration == 0 {
            return;
        }
        let fraction = fraction.clamp(0.0, 1.0);
        let position = (fraction as f64 * state.duration as f64).round() as u64;
        if let Err(e) = runtime.core().seek(position) {
            log::error!("[qbz-slint] playback: seek failed: {e}");
        }
    });
}

/// Mute state and the volume to restore on unmute. `PREMUTE_VOLUME`
/// holds the f32 level as bits; `MUTED` is the authoritative flag.
static MUTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static PREMUTE_VOLUME: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Set the player volume from `fraction` (0..1). A non-zero level clears
/// any active mute, so dragging the slider or stepping volume unmutes.
pub fn set_volume(
    runtime: Runtime,
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
    fraction: f32,
) {
    handle.spawn(async move {
        let fraction = fraction.clamp(0.0, 1.0);
        if let Err(e) = runtime.core().set_volume(fraction) {
            log::error!("[qbz-slint] playback: set_volume failed: {e}");
        }
        if fraction > 0.0 && MUTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
            let _ = weak.upgrade_in_event_loop(|w| {
                w.global::<NowPlayingState>().set_muted(false);
            });
        }
    });
}

/// Toggle mute: silence the player and remember the level, or restore it.
pub fn toggle_mute(runtime: Runtime, weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    use std::sync::atomic::Ordering;
    handle.spawn(async move {
        if MUTED.swap(false, Ordering::Relaxed) {
            // Unmute — restore the stored level.
            let restored = f32::from_bits(PREMUTE_VOLUME.load(Ordering::Relaxed));
            let restored = if restored > 0.0 { restored } else { 0.7 };
            let _ = runtime.core().set_volume(restored);
            let _ = weak.upgrade_in_event_loop(|w| {
                w.global::<NowPlayingState>().set_muted(false);
            });
        } else {
            // Mute — stash the current level, then drop to zero.
            let current = runtime.core().player().get_playback_event().volume;
            let current = if current > 0.0 { current } else { 0.7 };
            PREMUTE_VOLUME.store(current.to_bits(), Ordering::Relaxed);
            MUTED.store(true, Ordering::Relaxed);
            let _ = runtime.core().set_volume(0.0);
            let _ = weak.upgrade_in_event_loop(|w| {
                w.global::<NowPlayingState>().set_muted(true);
            });
        }
    });
}

/// Toggle shuffle on the queue and reflect the new state on NowPlayingState.
pub fn toggle_shuffle(
    runtime: Runtime,
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
) {
    handle.spawn(async move {
        let on = runtime.core().toggle_shuffle().await;
        let _ = weak.upgrade_in_event_loop(move |w| {
            w.global::<NowPlayingState>().set_shuffle(on);
        });
    });
}

/// Cycle the repeat mode Off -> All -> One -> Off and reflect it.
pub fn cycle_repeat(
    runtime: Runtime,
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
) {
    handle.spawn(async move {
        let next = match runtime.core().get_queue_state().await.repeat {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        };
        runtime.core().set_repeat_mode(next).await;
        let mode: i32 = match next {
            RepeatMode::Off => 0,
            RepeatMode::All => 1,
            RepeatMode::One => 2,
        };
        let _ = weak.upgrade_in_event_loop(move |w| {
            w.global::<NowPlayingState>().set_repeat_mode(mode);
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elapsed_pads_seconds() {
        assert_eq!(fmt_elapsed(0), "0:00");
        assert_eq!(fmt_elapsed(9), "0:09");
        assert_eq!(fmt_elapsed(65), "1:05");
        assert_eq!(fmt_elapsed(605), "10:05");
    }

    #[test]
    fn remaining_counts_down_and_pads() {
        assert_eq!(fmt_remaining(0, 200), "-3:20");
        assert_eq!(fmt_remaining(195, 200), "-0:05");
        assert_eq!(fmt_remaining(200, 200), "-0:00");
        // Position past duration must not underflow.
        assert_eq!(fmt_remaining(250, 200), "-0:00");
    }

    #[test]
    fn queue_row_appends_version() {
        let track = QueueTrack {
            id: 7,
            title: "Song".to_string(),
            version: Some("Live".to_string()),
            artist: "Artist".to_string(),
            album: "Album".to_string(),
            duration_secs: 100,
            artwork_url: None,
            hires: false,
            bit_depth: None,
            sample_rate: None,
            is_local: false,
            album_id: None,
            artist_id: None,
            streamable: true,
            source: None,
            parental_warning: false,
            source_item_id_hint: None,
        };
        let row = queue_row(&track, true);
        assert_eq!(row.title, "Song (Live)");
        assert_eq!(row.id, "7");
        assert!(row.playing);
    }
}

/// Start the playback poll loop. Runs for the app lifetime: every ~450ms
/// it reads the player event and pushes position / progress onto
/// `NowPlayingState`. When a track ends it auto-advances the queue.
pub fn start_poll_loop(
    runtime: Runtime,
    weak: slint::Weak<AppWindow>,
    handle: tokio::runtime::Handle,
) {
    let spawn_handle = handle.clone();
    spawn_handle.spawn(async move {
        // Track whether the last poll observed an active track, so the
        // end-of-track edge is detected once rather than every tick.
        let mut last_track_id: u64 = 0;
        let mut was_playing = false;
        let mut seen_position: u64 = 0;

        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(450));
        loop {
            ticker.tick().await;

            let event = runtime.core().player().get_playback_event();

            let track_id = event.track_id;
            let position = event.position;
            let duration = event.duration;
            let is_playing = event.is_playing;
            let volume = event.volume;

            // Detect end-of-track: there was a track, it has reached the
            // end (position within the duration) and is no longer playing.
            let track_ended = was_playing
                && !is_playing
                && last_track_id != 0
                && (track_id == 0 || track_id == last_track_id)
                && duration > 0
                && seen_position + 2 >= duration;

            // Push the live values onto NowPlayingState.
            let progress = if duration > 0 {
                (position as f32 / duration as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let elapsed = fmt_elapsed(position);
            let remaining = fmt_remaining(position, duration);
            let _ = weak.upgrade_in_event_loop(move |w| {
                let np = w.global::<NowPlayingState>();
                np.set_position_secs(position as i32);
                if duration > 0 {
                    np.set_duration_secs(duration as i32);
                }
                np.set_progress(progress);
                np.set_elapsed(elapsed.into());
                np.set_remaining(remaining.into());
                np.set_playing(is_playing);
                np.set_volume(volume.clamp(0.0, 1.0));
            });

            if track_id != 0 {
                last_track_id = track_id;
                seen_position = position;
            }
            was_playing = is_playing;

            // Auto-advance on track end.
            if track_ended {
                last_track_id = 0;
                was_playing = false;
                seen_position = 0;
                if let Some(track) = runtime.core().next_track().await {
                    let next_id = track.id;
                    refresh_queue(&runtime, &weak).await;
                    refresh_now_playing_meta(&runtime, &weak).await;
                    record_recent(&runtime).await;
                    play_audible(&runtime, next_id).await;
                } else {
                    log::info!("[qbz-slint] playback: queue finished");
                    let _ = weak.upgrade_in_event_loop(|w| {
                        let np = w.global::<NowPlayingState>();
                        np.set_playing(false);
                        np.set_progress(0.0);
                        np.set_position_secs(0);
                    });
                }
            }
        }
    });
}
