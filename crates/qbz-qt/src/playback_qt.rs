//! Minimal playback controller — drives `QbzCore`'s EXISTING public API
//! only (qbz-audio / qbz-player are protected; no audio logic is
//! reimplemented here). Ports the *shape* of `crates/qbz/src/playback.rs`:
//! play_album (fetch -> QueueTrack vec -> set_queue -> play_track_resolved)
//! and the poll loop (PlaybackEvent -> UI state, track-change meta refresh,
//! end-of-track advance).
//!
//! POC-NOTEs (deliberate cuts vs playback.rs, named for the effort report):
//! - Streaming quality: seeded from ui_prefs ("streaming_quality") and
//!   live-updated by Settings > Audio (settings_qt). The #638 device-cap
//!   clamp lives in the Slint glue and is NOT ported.
//! - Blacklist filtering of album tracks: skipped (store not open).
//! - Offline-cache tier (offline bytes), gapless prefetch (`play_next`),
//!   prefetch warming, stop-after, infinite refill, session persist,
//!   QConnect/cast branches, recently-played recording: all out of scope.
//!   Auto-advance therefore plays each successor at fetch time (audible
//!   gap between tracks) — the engine's gapless path is NOT wired.
//! - `advance_to_playable` (skip-unavailable walk): the POC advance takes
//!   the core queue's next directly; a failed play logs and stops.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use cxx_qt_lib::QString;
use qbz_models::{Quality, QueueTrack, RepeatMode};

/// The request tier for every play. Persisted in ui_prefs.json ("streaming_quality")
/// and applied here from Settings > Audio (settings_qt). Default "hires_plus".
static POC_QUALITY: RwLock<Quality> = RwLock::new(Quality::UltraHiRes);

/// Map a ui_prefs quality key to the request tier (settings.rs STREAMING_QUALITIES).
fn quality_for_key(key: &str) -> Quality {
    match key {
        "mp3" => Quality::Mp3,
        "cd" => Quality::Lossless,
        "hires" => Quality::HiRes,
        _ => Quality::UltraHiRes,
    }
}

/// Settings > Audio > Streaming quality writes through here (also used at
/// startup to seed from the persisted prefs).
pub fn set_streaming_quality(key: &str) {
    *POC_QUALITY.write().unwrap() = quality_for_key(key);
}

pub(crate) fn current_quality() -> Quality {
    *POC_QUALITY.read().unwrap()
}

// Mute bookkeeping (mirrors playback.rs MUTED / PREMUTE_VOLUME: there is no
// dedicated mute API on the core — mute is volume 0 with a stash).
static MUTED: AtomicBool = AtomicBool::new(false);
static PREMUTE_VOLUME: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Playback CONTEXT — the "playing from" origin (song-card layers glyph)
// ---------------------------------------------------------------------------

/// The container a queue was launched FROM. `kind` is "album" | "artist" |
/// "playlist" | "label" and `id` that container's navigation id — 1:1 with the
/// Slint `open-context(kind, id)` -> `media-action(kind, id, "open")` arms
/// (qbz/src/main.rs:12695 artist, :12701 album, :12707 playlist, :13206 label).
///
/// HARDENING (owner ask): the origin is NOT something a play path has to
/// remember. It travels WITH the queue — every play/enqueue entry point in this
/// module funnels through `set_queue_stamped` / `stamped`, which stamp it onto
/// EVERY track, and when the caller passes none it is DERIVED from the queue
/// itself (`derive_context`). A new entry point therefore cannot silently ship
/// an unstamped queue; the worst case is the same album fallback the Slint uses
/// in `refresh_now_playing_meta` (playback.rs:1959-1965).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayContext {
    pub kind: String,
    pub id: String,
}

impl PlayContext {
    /// None for an empty kind/id — an empty context must never be stamped (it
    /// would shadow the per-track album fallback with a dead glyph).
    pub fn new(kind: &str, id: &str) -> Option<Self> {
        if kind.is_empty() || id.is_empty() {
            return None;
        }
        Some(Self { kind: kind.to_string(), id: id.to_string() })
    }

    pub fn album(id: &str) -> Option<Self> {
        Self::new("album", id)
    }

    pub fn artist(id: &str) -> Option<Self> {
        Self::new("artist", id)
    }

    #[allow(dead_code)] // used once the playlist play path routes through here
    pub fn playlist(id: &str) -> Option<Self> {
        Self::new("playlist", id)
    }
}

/// Infer the container from the queue itself: one shared album -> that album,
/// otherwise one shared artist -> that artist (an artist page's Popular Tracks
/// span many albums but ONE artist — the exact case that shipped contextless).
/// Nothing shared -> None, and the per-track album fallback takes over.
fn derive_context(tracks: &[QueueTrack]) -> Option<PlayContext> {
    let mut album: Option<&str> = None;
    let mut one_album = true;
    let mut artist: Option<u64> = None;
    let mut one_artist = true;

    for track in tracks {
        match track.album_id.as_deref().filter(|s| !s.is_empty()) {
            None => one_album = false,
            Some(id) => {
                if let Some(prev) = album {
                    if prev != id {
                        one_album = false;
                    }
                } else {
                    album = Some(id);
                }
            }
        }
        match track.artist_id {
            None => one_artist = false,
            Some(id) => {
                if let Some(prev) = artist {
                    if prev != id {
                        one_artist = false;
                    }
                } else {
                    artist = Some(id);
                }
            }
        }
    }

    if one_album {
        if let Some(id) = album {
            return PlayContext::album(id);
        }
    }
    // A SINGLE track that shares "one artist" is not an artist container — it
    // is a bare track play, whose Slint fallback is its own album. Only a
    // multi-track queue earns the artist origin.
    if one_artist && tracks.len() > 1 {
        if let Some(id) = artist {
            return PlayContext::artist(&id.to_string());
        }
    }
    None
}

/// Stamp the origin onto every track that does not already carry one. Tracks
/// that arrive pre-stamped (the album/playlist/local builders) keep theirs, so
/// a mixed queue stays honest per row.
fn stamp_context(tracks: &mut [QueueTrack], explicit: Option<PlayContext>) {
    let resolved = match explicit {
        Some(ctx) => Some(ctx),
        None => derive_context(tracks),
    };
    let Some(ctx) = resolved else {
        return;
    };
    for track in tracks.iter_mut() {
        let already_stamped = track.context_kind.as_deref().is_some_and(|k| !k.is_empty())
            && track.context_id.as_deref().is_some_and(|i| !i.is_empty());
        if already_stamped {
            continue;
        }
        track.context_kind = Some(ctx.kind.clone());
        track.context_id = Some(ctx.id.clone());
    }
}

/// The ONLY `core().set_queue` call in this module — every play path goes
/// through it, so the origin can never be dropped on the floor.
pub(crate) async fn set_queue_stamped(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    mut tracks: Vec<QueueTrack>,
    start: Option<usize>,
    context: Option<PlayContext>,
) {
    stamp_context(&mut tracks, context);
    runtime.core().set_queue(tracks, start).await;
}

/// Same guarantee for the ADD paths (play-next / add-to-queue): appended tracks
/// carry their own origin, so the glyph stays right after the queue advances
/// into them.
fn stamped(mut tracks: Vec<QueueTrack>, context: Option<PlayContext>) -> Vec<QueueTrack> {
    stamp_context(&mut tracks, context);
    tracks
}

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

/// Artist-card play (ArtistGridCard overlay / menu, phase 16) — playback.rs
/// `play_artist` 1:1: fetch the artist page ONCE, play the Popular tracks;
/// when the artist has none, fall back to the STUDIO discography (release
/// buckets album/epSingle/ep/single in page order, deduped), concatenating
/// each album's tracks and skipping albums that fail (a bulk play must not
/// abort on one unavailable album).
pub async fn play_artist(runtime: &Arc<AppRuntime<LoggingAdapter>>, artist_id: &str) -> Result<(), String> {
    const STUDIO_TYPES: &[&str] = &["album", "epSingle", "ep", "single"];
    let id: u64 = artist_id
        .parse()
        .map_err(|_| format!("invalid artist id: {artist_id}"))?;
    let page = runtime
        .core()
        .get_artist_page(id, None)
        .await
        .map_err(|e| format!("get_artist_page {artist_id} failed: {e}"))?;
    let artist_name = page.name.display.clone();

    // 1) Popular tracks — the primary behavior.
    let top: Vec<QueueTrack> = page
        .top_tracks
        .unwrap_or_default()
        .iter()
        .map(|track| {
            // make_top_track_queue: /artist/page tracks carry a thinner
            // audio_info than /album/get tracks; fields fall back.
            let audio = track.audio_info.as_ref();
            let album = track.album.as_ref();
            let album_id = album.map(|a| a.id.clone());
            QueueTrack {
                id: track.id,
                title: track.title.clone(),
                version: track.version.clone(),
                artist: track
                    .artist
                    .as_ref()
                    .map(|a| a.name.display.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| artist_name.clone()),
                album: album.map(|a| a.title.clone()).unwrap_or_default(),
                album_version: None,
                duration_secs: track.duration.unwrap_or(0) as u64,
                artwork_url: album
                    .and_then(|a| a.image.as_ref())
                    .and_then(|img| img.best().cloned()),
                hires: audio
                    .and_then(|a| a.maximum_bit_depth)
                    .map(|b| b > 16)
                    .unwrap_or(false),
                bit_depth: audio.and_then(|a| a.maximum_bit_depth),
                sample_rate: audio.and_then(|a| a.maximum_sampling_rate),
                is_local: false,
                album_id: album_id.clone(),
                artist_id: track.artist.as_ref().map(|a| a.id),
                streamable: track
                    .rights
                    .as_ref()
                    .and_then(|r| r.streamable)
                    .unwrap_or(true),
                source: Some("qobuz".to_string()),
                parental_warning: track.parental_warning.unwrap_or(false),
                source_item_id_hint: album_id,
                context_kind: Some("artist".to_string()),
                context_id: Some(artist_id.to_string()),
            }
        })
        .collect();
    if !top.is_empty() {
        let first_id = top[0].id;
        set_queue_stamped(runtime, top, Some(0), PlayContext::artist(artist_id)).await;
        publish_queue(runtime).await;
        runtime
            .core()
            .play_track_resolved(first_id, current_quality(), None, None, 0)
            .await
            .map_err(|e| format!("play_track {first_id} failed: {e}"))?;
        refresh_now_playing(runtime).await;
        return Ok(());
    }

    // 2) Fallback — the studio discography (deduped album ids in the page's
    // section order; compilation/live/other omitted — studio releases only).
    let mut album_ids: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for group in page.releases.unwrap_or_default() {
        if STUDIO_TYPES.contains(&group.release_type.as_str()) {
            for item in group.items {
                if seen.insert(item.id.clone()) {
                    album_ids.push(item.id);
                }
            }
        }
    }
    if album_ids.is_empty() {
        return Err(format!("artist {artist_id} has no top tracks and no studio releases"));
    }
    let mut queue: Vec<QueueTrack> = Vec::new();
    for aid in &album_ids {
        match fetch_album_queue(runtime, aid).await {
            Ok(tracks) => queue.extend(tracks),
            Err(e) => log::warn!("[qbz-qt] artist-play: album {aid} skipped: {e}"),
        }
    }
    if queue.is_empty() {
        return Err(format!("artist {artist_id} studio discography produced no playable tracks"));
    }
    log::info!("[qbz-qt] artist-play {artist_id}: discography fallback, {} tracks", queue.len());
    let first_id = queue[0].id;
    // The discography queue arrives pre-stamped per ALBUM (fetch_album_queue);
    // the artist origin is what the user launched, so it wins here — the
    // explicit context overrides the per-album stamp for the whole queue.
    for track in queue.iter_mut() {
        track.context_kind = None;
        track.context_id = None;
    }
    set_queue_stamped(runtime, queue, Some(0), PlayContext::artist(artist_id)).await;
    publish_queue(runtime).await;
    runtime
        .core()
        .play_track_resolved(first_id, current_quality(), None, None, 0)
        .await
        .map_err(|e| format!("play_track {first_id} failed: {e}"))?;
    refresh_now_playing(runtime).await;
    Ok(())
}

/// "Play next" / "Add to queue" for an album (AlbumCard ⋯ menu): resolve
/// the album's tracks and insert them after the current track (mode
/// "next") or append them (mode "later").
pub async fn enqueue_album(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
    mode: &str,
) -> Result<(), String> {
    let tracks = stamped(
        fetch_album_queue(runtime, album_id).await?,
        PlayContext::album(album_id),
    );
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
    play_album_from(runtime, album_id, 0).await
}

/// Play an album starting at track `start_index` (AlbumView row play —
/// Slint's play_album_from semantics: the queue keeps the whole album,
/// the cursor starts at the clicked track).
pub async fn play_album_from(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
    start_index: usize,
) -> Result<(), String> {
    log::info!("[qbz-qt] play_album: resolving {album_id} (start {start_index})");
    let tracks = fetch_album_queue(runtime, album_id).await?;
    let start = start_index.min(tracks.len() - 1);
    let first_id = tracks[start].id;
    let count = tracks.len();
    set_queue_stamped(runtime, tracks, Some(start), PlayContext::album(album_id)).await;
    publish_queue(runtime).await;
    log::info!("[qbz-qt] play_album: queue set ({count} tracks), playing track {first_id}");
    runtime
        .core()
        .play_track_resolved(first_id, current_quality(), None, None, 0)
        .await
        .map_err(|e| format!("play_track {first_id} failed: {e}"))?;
    log::info!("[qbz-qt] play_album: play_track_resolved started for {first_id}");
    refresh_now_playing(runtime).await;
    Ok(())
}

/// AlbumView row play: play the album starting AT the clicked track
/// (Slint play_album_from: the queue keeps the whole album).
pub async fn play_album_from_track(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
    track_id: u64,
) -> Result<(), String> {
    let tracks = fetch_album_queue(runtime, album_id).await?;
    let start = tracks.iter().position(|t| t.id == track_id).unwrap_or(0);
    let first_id = tracks[start].id;
    set_queue_stamped(runtime, tracks, Some(start), PlayContext::album(album_id)).await;
    publish_queue(runtime).await;
    runtime
        .core()
        .play_track_resolved(first_id, current_quality(), None, None, 0)
        .await
        .map_err(|e| format!("play_track {first_id} failed: {e}"))?;
    refresh_now_playing(runtime).await;
    Ok(())
}

/// Play a pre-built queue starting at `start` (ArtistView Popular Tracks:
/// the visible list becomes the queue, anchored at the clicked track).
///
/// The origin is DERIVED from the queue when the caller has none (see
/// `stamp_context`), so this path can never publish a contextless track — the
/// artist page's Popular Tracks span many albums but one artist and resolve to
/// ("artist", artist_id) without the caller lifting a finger.
pub async fn play_track_list(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    tracks: Vec<QueueTrack>,
    start: usize,
    shuffle: bool,
) -> Result<(), String> {
    play_track_list_in(runtime, tracks, start, shuffle, None).await
}

/// `play_track_list` with an EXPLICIT origin — preferred whenever the caller
/// knows it (artist page, playlist, label), because it survives a queue whose
/// tracks share nothing derivable.
pub async fn play_track_list_in(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    tracks: Vec<QueueTrack>,
    start: usize,
    shuffle: bool,
    context: Option<PlayContext>,
) -> Result<(), String> {
    if tracks.is_empty() {
        return Err("empty track list".to_string());
    }
    if shuffle {
        runtime.core().set_shuffle(true).await;
        crate::now_playing::set_shuffle(true);
    }
    let start = start.min(tracks.len() - 1);
    let first_id = tracks[start].id;
    set_queue_stamped(runtime, tracks, Some(start), context).await;
    publish_queue(runtime).await;
    runtime
        .core()
        .play_track_resolved(first_id, current_quality(), None, None, 0)
        .await
        .map_err(|e| format!("play_track {first_id} failed: {e}"))?;
    refresh_now_playing(runtime).await;
    Ok(())
}

/// Append a pre-built queue to the current one ("Add all to queue").
pub async fn enqueue_track_list(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    tracks: Vec<QueueTrack>,
) -> Result<(), String> {
    if tracks.is_empty() {
        return Err("empty track list".to_string());
    }
    runtime.core().add_tracks(stamped(tracks, None)).await;
    publish_queue(runtime).await;
    Ok(())
}

/// One track from an album context into the queue ("Play next" / "Add to
/// queue" on an AlbumView row).
pub async fn enqueue_album_track(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
    track_id: u64,
    mode: &str,
) -> Result<(), String> {
    let tracks = fetch_album_queue(runtime, album_id).await?;
    let Some(track) = tracks.into_iter().find(|t| t.id == track_id) else {
        return Err(format!("track {track_id} not in album {album_id}"));
    };
    if mode == "next" {
        runtime.core().add_track_next(track).await;
    } else {
        runtime.core().add_track(track).await;
    }
    publish_queue(runtime).await;
    Ok(())
}

/// Shuffle-play an album (AlbumView header Shuffle): enable shuffle, then
/// play — the core queue owns the shuffled order.
pub async fn play_album_shuffled(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
) -> Result<(), String> {
    runtime.core().set_shuffle(true).await;
    crate::now_playing::set_shuffle(true);
    play_album(runtime, album_id).await
}

/// Build the queue meta for a Library-feed track (shared by
/// play_single_track / enqueue_single_track).
fn feed_queue_track(track_id: u64) -> Result<QueueTrack, String> {
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
    Ok(QueueTrack {
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
    })
}

/// Play a single track as a one-element queue (Library track rows). The
/// queue meta is rebuilt from the Library feed row; the audio resolves by
/// id through the same `play_track_resolved` path.
pub async fn play_single_track(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    track_id: u64,
) -> Result<(), String> {
    let qt = feed_queue_track(track_id)?;
    // Bare single-track play: no container origin, so the derive falls to the
    // track's own album — same landing spot as the Slint fallback.
    set_queue_stamped(runtime, vec![qt], Some(0), None).await;
    publish_queue(runtime).await;
    log::info!("[qbz-qt] play_single_track: playing {track_id}");
    runtime
        .core()
        .play_track_resolved(track_id, current_quality(), None, None, 0)
        .await
        .map_err(|e| format!("play_track {track_id} failed: {e}"))?;
    refresh_now_playing(runtime).await;
    Ok(())
}

/// One Library-feed track into the EXISTING queue (the TrackCard / list-row
/// context menus): "next" -> add_track_next, "later" -> add_track_later
/// (#442 block tail), "queue" -> add_track (append).
pub async fn enqueue_single_track(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    track_id: u64,
    mode: &str,
) -> Result<(), String> {
    let qt = stamped(vec![feed_queue_track(track_id)?], None)
        .pop()
        .expect("one track in, one track out");
    match mode {
        "next" => runtime.core().add_track_next(qt).await,
        "later" => runtime.core().add_track_later(qt).await,
        _ => runtime.core().add_track(qt).await,
    }
    publish_queue(runtime).await;
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

pub(crate) async fn play_queue_track_public(runtime: &Arc<AppRuntime<LoggingAdapter>>, track_id: u64) {
    play_queue_track(runtime, track_id).await;
}

async fn play_queue_track(runtime: &Arc<AppRuntime<LoggingAdapter>>, track_id: u64) {
    // Source-aware audible step: a LOCAL file plays from disk through the
    // player's play_data seam. The Qobuz tier-walk below needs a client and
    // would fail with "No Qobuz client available" on every local advance.
    if crate::local_library_qt::play_current_if_local(runtime, track_id).await {
        refresh_now_playing(runtime).await;
        publish_queue(runtime).await;
        return;
    }
    if let Err(e) = runtime
        .core()
        .play_track_resolved(track_id, current_quality(), None, None, 0)
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
pub(crate) async fn refresh_now_playing(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let state = runtime.core().get_queue_state().await;
    let Some(track) = state.current_track else {
        crate::now_playing::clear_track();
        crate::player_bridge::ui(move |mut b| {
            b.as_mut().set_np_track_id(QString::from(""));
        });
        crate::lyrics_qt::publish_idle();
        return;
    };
    let title = match track.version.as_deref().filter(|v| !v.is_empty()) {
        Some(version) => format!("{} ({version})", track.title),
        None => track.title.clone(),
    };
    let track_id_str = track.id.to_string();
    crate::player_bridge::ui(move |mut b| {
        b.as_mut().set_np_track_id(QString::from(track_id_str.as_str()));
    });
    log::info!(
        "[qbz-qt] now playing: '{title}' — {} ({}s)",
        track.artist,
        track.duration_secs,
    );
    let (tier, label) = quality_badge(&track);
    let album_id = track.album_id.clone().unwrap_or_default();
    // Album with its release variant appended ("Octavarium (2009 Remaster)"),
    // 1:1 with the Slint `album_display` (playback.rs:1941-1949).
    let album_display = match track
        .album_version
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        Some(v) => format!("{} ({v})", track.album),
        None => track.album.clone(),
    };
    // "Playing from" origin for the song-card layers glyph, re-derived from the
    // CURRENT track's own stamp on EVERY change (never a stale global) —
    // playback.rs:1953-1965. A track with no container origin falls back to its
    // own album, exactly like the Slint.
    let (context_kind, context_id) = match (
        track.context_kind.as_deref().filter(|s| !s.is_empty()),
        track.context_id.as_deref().filter(|s| !s.is_empty()),
    ) {
        (Some(kind), Some(id)) => (kind.to_string(), id.to_string()),
        _ => ("album".to_string(), album_id.clone()),
    };
    crate::now_playing::set_track(crate::now_playing::TrackMeta {
        title,
        artist: track.artist.clone(),
        album: album_display,
        album_id,
        artist_id: track
            .artist_id
            .map(|id| id.to_string())
            .unwrap_or_default(),
        context_kind,
        context_id,
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
    // The two output LEDs (+ the volume-lock flag) are decided HERE, not when
    // the Settings page republishes: a stream is about to open and the
    // backend/mode it will use is whatever the audio settings say right now.
    // Publishing on the track edge is what makes the stamp live from the first
    // note instead of updating only when the user changes page. Cheap: one WAL
    // read + one Qt hop per track — no poll, and NOT publish_snapshot (that
    // rebuilds the entire Settings document).
    crate::output_labels::publish_current();
    // Catalog max + whether the streaming-quality pref governs this request:
    // the downgrade arrow compares DELIVERED against this (quality_state.rs).
    // Local and Plex sources are not governed — nothing downgrades them.
    let governed = !track.is_local;
    crate::now_playing::set_catalog_quality(track.bit_depth, track.sample_rate, governed);
    // Artwork through the same cache pipeline as Home (attach + background
    // download + republish — single url here).
    crate::artwork_qt::attach_now_playing(&track.artwork_url.clone().unwrap_or_default());
    // Ambient background triad (phase 14): recompute from the new track's
    // cover (no-op until the cover lands on disk — the previous palette
    // stays, like the Slint's default-until-resolved).
    crate::ambient_qt::update_for_artwork(&track.artwork_url.clone().unwrap_or_default());
    // Lyrics for the new track (loading state, then the doc).
    crate::lyrics_qt::publish_loading();
    let runtime = runtime.clone();
    let track = track.clone();
    crate::spawn(async move {
        crate::lyrics_qt::load_for_track(&runtime, &track).await;
    });
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

/// Publish the queue panel document (queue_qt.rs — the full QueueState
/// port: sections, history, pagination, search).
pub async fn publish_queue(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    crate::queue_qt::publish(runtime).await;
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
                // Integrations: scrobble now-playing + arm the delayed
                // scrobble, and refresh the Discord presence. This is the
                // DE-DUPED track edge on purpose — firing it from
                // refresh_now_playing would re-arm the timer on every republish.
                crate::integrations_qt::on_track_change_edge(&runtime);
                // Local play history — Recently Played and Most Played read the
                // same file the other frontends write; without this the Qt build
                // shows their history and never adds to it. Same de-duped edge
                // as the scrobblers, never from refresh_now_playing.
                if let Some(track) = runtime.core().get_queue_state().await.current_track {
                    crate::recently_qt::record_queue_track(&track);
                }
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

            // DELIVERED stream params -> the downgrade arrow + tooltip cause.
            // Deduped inside, so a steady stream costs no Qt-thread hop.
            crate::now_playing::set_effective_stream(
                event.sample_rate.unwrap_or(0),
                event.bit_depth.unwrap_or(0),
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
            if is_playing != was_playing {
                crate::integrations_qt::discord_push(&runtime);
            }
            was_playing = is_playing;
        }
    });
}
