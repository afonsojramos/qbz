//! Minimal playback controller — drives `QbzCore`'s EXISTING public API
//! only (qbz-audio / qbz-player are protected; no audio logic is
//! reimplemented here). Ports the *shape* of `crates/qbz/src/playback.rs`:
//! play_album (fetch -> QueueTrack vec -> set_queue -> play_track_resolved)
//! and the poll loop (PlaybackEvent -> UI state, track-change meta refresh,
//! end-of-track advance).
//!
//! OWED PORTS vs playback.rs. These are DEBT, not scope decisions: a fix that
//! lived in the frontend is not inherited and has to be replicated (TRACK-RULES
//! rule 8, whose title is "los fixes de backend deben llegar al Qt"). Each one
//! is tracked in `qbz-nix-docs/qt-frontend/2026-07-30-parity-debt/PARITY-DEBT.md`
//! — add there, not here, so a note cannot outlive its gap unnoticed. That is
//! exactly how gapless stayed broken: this header advertised it for weeks.
//!
//! - Session persist / resume-position: both prefs are persisted by settings
//!   and read by nobody.
//! - Streaming quality: seeded from ui_prefs ("streaming_quality") and
//!   live-updated by Settings > Audio (settings_qt). The #638 device-cap clamp
//!   lives in the Slint glue and is NOT ported.
//! - Offline-cache tier (offline bytes), prefetch warming, QConnect branches.
//!
//! DONE, previously listed here — kept as one line each so the list above
//! only ever names REAL gaps:
//! - Stop-after-this-song (#35) — the end-of-track arm consumes the marker and
//!   pauses AHEAD of repeat/shuffle, and the gapless pre-queue guard that was
//!   already here now defends a marker the UI can actually place.
//! - Infinite refill (#33) — `try_infinite_refill` chains a smart artist radio
//!   off the ended track when the queue runs out and the ∞ toggle is on.
//! - Gapless prefetch: the engine raises `gapless_ready` ~10s out and this
//!   module now answers it with `play_next` (both the streaming and the
//!   local/DSD arms), so transitions are sample-accurate again.
//! - Blacklist filtering of the QUEUE (#1) — `filter_blacklisted_queue` +
//!   `set_queue_stamped` / `stamped`, the two seams every play/enqueue path
//!   already went through.
//! - The bounded skip-unavailable walk (#2) — `auto_skip_unavailable`.
//! - Volume persistence (#3) — persisted on drag-end, restored at shell entry.
//! - The stream-failure toast with the Flatpak/Snap ALSA rewrite (#7) —
//!   `stream_error_text`, drained in the poll loop.
//! - The streaming seek lock (#15) — `seekable_max` from `buffer_progress`,
//!   published as `np_seekable_max`.
//! - Shuffle-play reorders the queue instead of latching the global shuffle
//!   MODE (#17) — `xorshift_shuffle` in `play_track_list_in`.
//! - Recently-played / Most-played recording — `recently_qt::record_queue_track`
//!   on the de-duped track edge, EPHEMERAL-EXEMPT: an ephemeral folder plays
//!   and leaves no row behind (owner rule, 2026-07-30). The Slint reference
//!   does NOT have that exemption; do not "restore parity" by removing it.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use cxx_qt_lib::QString;
use qbz_models::{PageArtistTrack, Quality, QueueTrack, RepeatMode};

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

    pub fn playlist(id: &str) -> Option<Self> {
        Self::new("playlist", id)
    }

    /// A label page's Popular Tracks. The reference stamps this on BOTH label
    /// play paths — the header Shuffle (`playback.rs:3149`) and a row click
    /// (`:3541`) — and it cannot be derived: a label's popular tracks share
    /// neither an album nor an artist, so `derive_context` returns None and
    /// the queue would ship contextless (PARITY-DEBT #17's reference path).
    pub fn label(id: &str) -> Option<Self> {
        Self::new("label", id)
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

    // ORDER IS DELIBERATE — album before artist. Do not flip it: a one-album
    // queue (the common case: an album play whose producer forgot to stamp)
    // legitimately wants the ALBUM origin, and an artist page's Popular Tracks
    // only reach the artist arm because they span albums. Producers that know
    // better (playlist / artist page / label) pass an EXPLICIT context and
    // never depend on this order — this is the safety net for the next
    // producer someone adds, not the primary mechanism.
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

/// THE blacklist drop predicate for a built queue row (playback.rs:2624-2632
/// `queue_track_blacklisted`). Local / Plex / no-id rows fail open — the
/// underlying `is_track_blacklisted` returns false for any source that is not
/// "qobuz", so a local file can never be dropped by an artist block.
fn queue_track_blacklisted(track: &QueueTrack) -> bool {
    let source = track.source.as_deref().unwrap_or("qobuz");
    crate::artist_blacklist::is_track_blacklisted(
        source,
        track.artist_id,
        None,
        track.album_id.as_deref(),
    )
}

/// Drop blacklisted entries from a freshly-built queue AND remap the start
/// index onto the surviving track (playback.rs:2634-2641 `filter_blacklisted_queue`).
///
/// The Slint applies this at ~18 separate builder sites. This port funnels every
/// play path through `set_queue_stamped`, so it needs exactly one — which is
/// also why its absence was total rather than partial: the store was open and
/// the manager worked, but nothing between the two ever asked.
///
/// The start remap is the part the Slint gets for free by filtering before it
/// computes the index. Here the caller has already picked an index into the
/// UNFILTERED list, so dropping rows ahead of it would silently start the wrong
/// track. The clicked row surviving is the common case; when the clicked row is
/// itself blocked, the walk lands on the next survivor, which is what the user
/// asking for "play from here" means once "here" is hidden.
fn filter_blacklisted_queue(
    tracks: Vec<QueueTrack>,
    start: Option<usize>,
) -> (Vec<QueueTrack>, Option<usize>) {
    let (kept, start) = filter_queue_with(tracks, start, queue_track_blacklisted);
    (kept, start)
}

/// The pure half, split out so the index remap is unit-testable without a bound
/// session (the blacklist store needs one). `drop` decides; everything else is
/// arithmetic.
fn filter_queue_with<T>(
    tracks: Vec<T>,
    start: Option<usize>,
    drop: impl Fn(&T) -> bool,
) -> (Vec<T>, Option<usize>) {
    let clicked = start.unwrap_or(0);
    let mut kept = Vec::with_capacity(tracks.len());
    // Surviving index for the clicked row, or for the first survivor after it.
    let mut new_start: Option<usize> = None;
    let mut dropped = 0usize;
    for (i, track) in tracks.into_iter().enumerate() {
        if drop(&track) {
            dropped += 1;
            continue;
        }
        if new_start.is_none() && i >= clicked {
            new_start = Some(kept.len());
        }
        kept.push(track);
    }
    if dropped > 0 {
        log::info!("[qbz-qt] blacklist: dropped {dropped} track(s) from the queue");
    }
    // Every survivor sits BEFORE the clicked row (the whole tail was blocked):
    // start at the last one rather than past the end.
    let start = new_start
        .or_else(|| kept.len().checked_sub(1))
        .filter(|_| !kept.is_empty());
    (kept, start)
}

/// The ONLY `core().set_queue` call in this module — every play path goes
/// through it, so the origin can never be dropped on the floor, and (since
/// PARITY-DEBT #1) neither can the blacklist.
pub(crate) async fn set_queue_stamped(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    tracks: Vec<QueueTrack>,
    start: Option<usize>,
    context: Option<PlayContext>,
) {
    // Filter BEFORE stamping: a dropped row needs no origin, and the start
    // index has to be remapped against the list the core actually receives.
    let (mut tracks, start) = filter_blacklisted_queue(tracks, start);
    stamp_context(&mut tracks, context);
    warn_dead_context(&tracks, "set_queue");
    runtime.core().set_queue(tracks, start).await;
    // QConnect (contract §6.3, Slint playback.rs:3697-3703): when WE are the
    // active renderer, push the new queue to the session immediately so a
    // controller's UI reflects the freshly-built queue — covers infinite-play
    // refills + album/radio plays. Self-gates to a no-op when not connected
    // or when a peer owns playback.
    if let Some(svc) = crate::qconnect_qt::service() {
        svc.sync_local_queue_if_changed().await;
    }
}

/// Same guarantee for the ADD paths (play-next / add-to-queue): appended tracks
/// carry their own origin, so the glyph stays right after the queue advances
/// into them.
///
/// `pub(crate)` on purpose: the local/ephemeral/bulk enqueue paths live in
/// other modules and must be able to reach the SAME stamping seam — a private
/// helper is what forced them to hand-roll their context in the first place.
pub(crate) fn stamped(tracks: Vec<QueueTrack>, context: Option<PlayContext>) -> Vec<QueueTrack> {
    // Same blacklist drop as `set_queue_stamped` — play-next / add-to-queue /
    // play-later must not be the back door that puts a blocked artist in the
    // queue. No start index to remap here: these APPEND.
    let before = tracks.len();
    let mut tracks: Vec<QueueTrack> = tracks
        .into_iter()
        .filter(|t| !queue_track_blacklisted(t))
        .collect();
    if tracks.len() < before {
        log::info!(
            "[qbz-qt] blacklist: dropped {} track(s) from an enqueue",
            before - tracks.len()
        );
    }
    stamp_context(&mut tracks, context);
    warn_dead_context(&tracks, "enqueue");
    tracks
}

// ---------------------------------------------------------------------------
// QConnect routing helpers (contract §6.3/§7 — Slint playback.rs:4082-4111
// plus the routed arms on every Qobuz-castable add path)
// ---------------------------------------------------------------------------

/// QConnect sync-on-add predicate (#442, playback.rs:4082-4096): true when
/// EVERY track of an added batch is Qobuz-castable (mirrors
/// `sync_local_queue_if_changed`'s all-or-nothing admission — `local` /
/// `plex` refused, offline `qobuz_download` eligible). A non-castable batch
/// skips the post-add push so a local/plex add never trips the refusal toast
/// on every click; the next track tick already surfaces that state.
pub(crate) fn batch_all_qconnect_castable(tracks: &[QueueTrack]) -> bool {
    tracks.iter().all(|qt| {
        qt.id > 0
            && !matches!(
                qt.source.as_deref().map(|s| s.to_ascii_lowercase()).as_deref(),
                Some("local") | Some("plex")
            )
    })
}

/// QConnect sync-on-add (#442, playback.rs:4098-4111): after a local queue
/// ADD (play-next / play-later / add-to-queue), push the updated queue to the
/// session immediately so a connected controller sees the new item AT its
/// position — previously it only re-synced on the next track transition.
/// `sync_local_queue_if_changed` self-gates (no-op when not connected, when a
/// peer owns playback, or when the queue is unchanged), so this is cheap in
/// every non-renderer situation.
pub(crate) async fn sync_qconnect_after_add(added_castable: bool) {
    if !added_castable {
        return;
    }
    if let Some(svc) = crate::qconnect_qt::service() {
        svc.sync_local_queue_if_changed().await;
    }
}

/// QConnect CONTROLLER-mode enqueue routing (contract §7 — the Slint add-path
/// arms, e.g. playback.rs:3012-3023, 4116-4128): when a PEER renderer owns
/// playback, route the add to the peer's queue instead of mutating only the
/// LOCAL queue (the peer never sees a local-only enqueue; the cloud echoes a
/// QueueUpdated that `materialize_remote_queue` applies). Mode-matched to the
/// caller's LOCAL placement: "next" inserts after the peer's current track,
/// "later" lands on the peer's manual-block tail, anything else appends.
/// Returns `true` when handled remotely — the caller MUST NOT enqueue
/// locally, and its sync tail does not run (the peer owns the queue now).
pub(crate) async fn route_enqueue_to_peer(tracks: &[QueueTrack], mode: &str) -> bool {
    let Some(svc) = crate::qconnect_qt::service() else {
        return false;
    };
    let routed: Vec<(u64, Option<String>)> = tracks
        .iter()
        .map(|qt| (qt.id, qt.source.clone()))
        .collect();
    match mode {
        "next" => svc.play_next_batch_on_peer_if_active(&routed).await,
        "later" => svc.play_later_batch_on_peer_if_active(&routed).await,
        _ => svc.add_to_queue_batch_on_peer_if_active(&routed).await,
    }
}

/// Single-track variant of [`route_enqueue_to_peer`] (Slint
/// playback.rs:4120-4128's single-track arm).
pub(crate) async fn route_track_to_peer(track: &QueueTrack, mode: &str) -> bool {
    let Some(svc) = crate::qconnect_qt::service() else {
        return false;
    };
    match mode {
        "next" => svc.play_next_on_peer_if_active(track.id, track.source.as_deref()).await,
        "later" => svc.play_later_on_peer_if_active(track.id, track.source.as_deref()).await,
        _ => svc.add_to_queue_on_peer_if_active(track.id, track.source.as_deref()).await,
    }
}

/// QConnect CONTROLLER-mode play routing (contract §7's decided arm position —
/// Slint playback.rs:638-651): when a PEER renderer owns playback, route the
/// play to the peer instead of running the local audible step. Call AFTER
/// `set_queue_stamped` (the arm pushes the CURRENT core queue — placed before
/// the funnel it would push the STALE one) and BEFORE `play_track_resolved`.
/// On handled, refreshes the bar + queue from the core cursor (the Slint
/// `after_track_change` meta refresh + sidebar) and returns `true` — the
/// caller early-returns and MUST NOT play locally. The Slint's `clear_loading`
/// after `play_on_peer_if_active` (playback.rs:643-650) maps to
/// `now_playing::clear_loading()` here: the poll loop's peer branch normally
/// clears the spinner via its position push (`has_audio = true`) on the next
/// tick, but a REFUSED routed play (mixed queue) never primes the peer — a
/// paused+idle peer would leave the spinner latched (sign-off F1).
pub(crate) async fn route_play_to_peer(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    track_id: u64,
) -> bool {
    let Some(svc) = crate::qconnect_qt::service() else {
        return false;
    };
    if !svc.play_on_peer_if_active(track_id).await {
        return false;
    }
    crate::now_playing::clear_loading();
    refresh_now_playing(runtime).await;
    publish_queue(runtime).await;
    true
}

/// Tripwire: a track with NEITHER a container origin NOR an album id publishes
/// `ctx-id == ""` (refresh_now_playing's fallback has nothing to fall back to),
/// which renders the song-card layers glyph as a dead control. Nothing in the
/// current producer set can hit this — the point is that the NEXT producer
/// someone adds says so in the log instead of shipping an inert button.
fn warn_dead_context(tracks: &[QueueTrack], site: &str) {
    let dead = tracks
        .iter()
        .filter(|t| {
            t.context_id.as_deref().unwrap_or("").is_empty()
                && t.album_id.as_deref().unwrap_or("").is_empty()
        })
        .count();
    if dead > 0 {
        log::warn!(
            "[qbz-qt] playback context: {dead}/{} track(s) reached {site} with no context AND no \
             album id — their 'playing from' glyph will be inert",
            tracks.len(),
        );
    }
}

// ---------------------------------------------------------------------------
// Play an album (album-card click on Home)
// ---------------------------------------------------------------------------

/// Fetch the album, build the queue (playback.rs `make_queue_track` port),
/// set it starting at track 1, and play through the core's resolved path.
/// `pub(crate)` so `library_bulk` can build ONE concatenated queue out of a
/// multi-album selection and hand it to `enqueue_track_list_mode` — going
/// through `enqueue_album` per id would publish the queue N times and, for
/// "later", append instead of landing in the manual block's tail.
pub(crate) async fn fetch_album_queue(
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

/// The /artist/page top-tracks -> QueueTrack mapping shared by `play_artist`
/// and the immersive search's (artist, next|queue) dispatch (contract
/// 2026-08-02-immersive-port §3.4: `enqueue_artist_top_by_id` = get_artist_page
/// -> top_tracks mapped like play_artist -> enqueue_track_list_mode).
/// make_top_track_queue semantics: /artist/page tracks carry a thinner
/// audio_info than /album/get tracks; fields fall back.
fn artist_top_queue_tracks(
    top_tracks: &[PageArtistTrack],
    artist_id: &str,
    artist_name: &str,
) -> Vec<QueueTrack> {
    top_tracks
        .iter()
        .map(|track| {
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
                    .unwrap_or_else(|| artist_name.to_string()),
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
        .collect()
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
    let top: Vec<QueueTrack> =
        artist_top_queue_tracks(page.top_tracks.as_deref().unwrap_or(&[]), artist_id, &artist_name);
    if !top.is_empty() {
        let first_id = top[0].id;
        set_queue_stamped(runtime, top, Some(0), PlayContext::artist(artist_id)).await;
        publish_queue(runtime).await;
        // QConnect CONTROLLER mode (§7): a peer owns playback — route the play
        // to it AFTER the funnel, never run the local audible step.
        if route_play_to_peer(runtime, first_id).await {
            return Ok(());
        }
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
    // QConnect CONTROLLER mode (§7): route the play to the peer (after the
    // funnel, before the local audible step).
    if route_play_to_peer(runtime, first_id).await {
        return Ok(());
    }
    runtime
        .core()
        .play_track_resolved(first_id, current_quality(), None, None, 0)
        .await
        .map_err(|e| format!("play_track {first_id} failed: {e}"))?;
    refresh_now_playing(runtime).await;
    Ok(())
}

/// LOCAL/Plex album keys must NEVER reach `fetch_album_queue` — `get_album`
/// 404s on a group key, the play returns Err and the card does nothing.
/// Mirrors the Slint guard inside `("album","play")` (qbz/src/main.rs:11871,
/// helper `is_local_album_key` at :2630-2632), which exists precisely because
/// Home "Recently played" / Pinned / Most-Played rails mix local rows into a
/// Qobuz card. Every album entry point below tests this FIRST.
fn is_local_album(album_id: &str) -> bool {
    crate::library_qt::is_local_album_key(album_id)
}

/// "Play next" / "Add to queue" for an album (AlbumCard ⋯ menu): resolve
/// the album's tracks and insert them after the current track (mode
/// "next") or append them (mode "later").
pub async fn enqueue_album(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
    mode: &str,
) -> Result<(), String> {
    if is_local_album(album_id) {
        log::info!("[qbz-qt] enqueue_album {album_id}: local/Plex group key -> local path");
        crate::local_playback::enqueue(
            runtime,
            "album".to_string(),
            album_id.to_string(),
            mode.to_string(),
        )
        .await;
        return Ok(());
    }
    let tracks = stamped(
        fetch_album_queue(runtime, album_id).await?,
        PlayContext::album(album_id),
    );
    log::info!("[qbz-qt] enqueue_album {album_id} ({mode}): {} tracks", tracks.len());
    // QConnect CONTROLLER mode (contract §7): route the add to the peer's
    // queue — early-returns when handled, so the local insert + sync tail
    // below only run in local/renderer mode. The mode is normalized to what
    // THIS fn's local match does: only "next" inserts at the cursor, every
    // other mode appends (there is no block-tail arm here).
    let routed_mode = if mode == "next" { "next" } else { "queue" };
    if route_enqueue_to_peer(&tracks, routed_mode).await {
        return Ok(());
    }
    let added_castable = batch_all_qconnect_castable(&tracks);
    if mode == "next" {
        // add_track_next inserts directly after the current track — feed
        // in reverse so the album's first track lands first.
        for track in tracks.into_iter().rev() {
            runtime.core().add_track_next(track).await;
        }
    } else {
        runtime.core().add_tracks(tracks).await;
    }
    // QConnect sync-on-add (#442): push the updated queue to the session so a
    // connected controller sees the new items; skipped silently for a
    // non-castable batch (self-gating when a peer owns playback).
    sync_qconnect_after_add(added_castable).await;
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
    if is_local_album(album_id) {
        // The local path owns its own queue building + audible step. The start
        // INDEX is not expressible there (it takes a row id) — every caller
        // that reaches here does so with 0 (`play_album`), and the row-anchored
        // variant is `play_album_from_track` below, which does pass the id.
        log::info!("[qbz-qt] play_album {album_id}: local/Plex group key -> local path");
        crate::local_playback::play_album(runtime, album_id.to_string(), None, false).await;
        return Ok(());
    }
    log::info!("[qbz-qt] play_album: resolving {album_id} (start {start_index})");
    let tracks = fetch_album_queue(runtime, album_id).await?;
    let start = start_index.min(tracks.len() - 1);
    let first_id = tracks[start].id;
    let count = tracks.len();
    set_queue_stamped(runtime, tracks, Some(start), PlayContext::album(album_id)).await;
    publish_queue(runtime).await;
    log::info!("[qbz-qt] play_album: queue set ({count} tracks), playing track {first_id}");
    // QConnect CONTROLLER mode (§7): route the play to the peer (after the
    // funnel, before the local audible step).
    if route_play_to_peer(runtime, first_id).await {
        return Ok(());
    }
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
    if is_local_album(album_id) {
        log::info!("[qbz-qt] play_album_from_track {album_id}: local/Plex group key -> local path");
        crate::local_playback::play_album(
            runtime,
            album_id.to_string(),
            Some(track_id as i64),
            false,
        )
        .await;
        return Ok(());
    }
    let tracks = fetch_album_queue(runtime, album_id).await?;
    let start = tracks.iter().position(|t| t.id == track_id).unwrap_or(0);
    let first_id = tracks[start].id;
    set_queue_stamped(runtime, tracks, Some(start), PlayContext::album(album_id)).await;
    publish_queue(runtime).await;
    // QConnect CONTROLLER mode (§7): route the play to the peer (after the
    // funnel, before the local audible step).
    if route_play_to_peer(runtime, first_id).await {
        return Ok(());
    }
    runtime
        .core()
        .play_track_resolved(first_id, current_quality(), None, None, 0)
        .await
        .map_err(|e| format!("play_track {first_id} failed: {e}"))?;
    refresh_now_playing(runtime).await;
    Ok(())
}

/// The reference's shuffle: a Fisher–Yates pass driven by an xorshift64 PRNG
/// seeded from the wall clock (`playback.rs:3131-3144`, byte-for-byte the same
/// mix as `play_album_shuffled` / `play_artist_top_shuffled`). Split from the
/// seed so the permutation is unit-testable — the seedless wrapper is what the
/// play paths call.
fn xorshift_shuffle_seeded<T>(items: &mut [T], seed: u64) {
    // The reference ORs the seed with 1 at construction: xorshift64 is stuck
    // on zero forever, and a nanosecond clock can land there.
    let mut seed = seed | 1;
    for i in (1..items.len()).rev() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let j = (seed % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

/// `xorshift_shuffle_seeded` with the reference's wall-clock seed.
///
/// `pub(crate)` so every Shuffle entry point can reorder its OWN list instead
/// of only raising the global shuffle MODE. Owner ruling 2026-08-01: "todos los
/// shuffles deben ser totalmente random"; a path that raises the flag and then
/// plays the list's #1 track is not shuffled, it is the original order with a
/// toggle lit — an artifact of a very early Tauri experiment.
pub(crate) fn xorshift_shuffle<T>(items: &mut [T]) {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1);
    xorshift_shuffle_seeded(items, seed);
}

/// The toast text for a failed audio stream (`playback.rs:5102-5113`).
/// `sandboxed` is the caller's `detect_sandbox() != Sandbox::None`: inside a
/// Flatpak/Snap sandbox direct `hw:` access is blocked by design, so an
/// ALSA-shaped failure is rewritten into the actionable sentence instead of
/// the raw engine error. Pure, so the rewrite rule is unit-tested.
fn stream_error_text(msg: &str, sandboxed: bool) -> String {
    let lower = msg.to_lowercase();
    if sandboxed && (lower.contains("alsa") || lower.contains("hw:")) {
        qbz_i18n::t(
            "Direct ALSA device access is blocked inside Flatpak/Snap — switch the audio backend to PipeWire or System Default",
        )
    } else {
        format!("{}: {}", qbz_i18n::t("Audio output error"), msg)
    }
}

/// Play a pre-built queue starting at `start` (ArtistView Popular Tracks:
/// the visible list becomes the queue, anchored at the clicked track).
///
/// The origin is DERIVED from the queue when the caller has none (see
/// `stamp_context`), so this path can never publish a contextless track — the
/// artist page's Popular Tracks span many albums but one artist and resolve to
/// ("artist", artist_id) without the caller lifting a finger.
///
/// `shuffle` REORDERS THIS QUEUE — it does not switch the player's shuffle
/// mode. See `play_track_list_in`.
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
///
/// PARITY-DEBT #17. `shuffle` means "play THIS list in a random order", which
/// in the reference is a one-shot reorder of the Vec followed by a play from
/// index 0 — the global shuffle MODE is never touched
/// (`playback.rs:3120-3145 play_label_top_shuffled`, and the identical body in
/// `play_artist_top_shuffled`:3083-3111 / `play_album_shuffled`:3945-3957).
/// This port used to answer the flag with `core().set_shuffle(true)` +
/// `now_playing::set_shuffle(true)`, so pressing Shuffle on a label or artist
/// page silently latched the bar's shuffle toggle ON for everything played
/// afterwards, AND still started on the list's #1 track. Both callers that
/// pass `true` (`label_qt::play_top`, `main::play_artist_top`) mirror a Slint
/// entry point that reorders in place, so the flag is reordering here too.
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
    // Shuffle-play starts at the top of the SHUFFLED order (`tracks[0]` after
    // the mix, exactly as the reference does), so the caller's anchor index is
    // meaningless on this path and is dropped rather than carried.
    let (tracks, start) = if shuffle {
        let mut tracks = tracks;
        xorshift_shuffle(&mut tracks);
        (tracks, 0)
    } else {
        (tracks, start)
    };
    let start = start.min(tracks.len() - 1);
    let first_id = tracks[start].id;
    set_queue_stamped(runtime, tracks, Some(start), context).await;
    publish_queue(runtime).await;
    // QConnect CONTROLLER mode (§7): route the play to the peer (after the
    // funnel, before the local audible step).
    if route_play_to_peer(runtime, first_id).await {
        return Ok(());
    }
    runtime
        .core()
        .play_track_resolved(first_id, current_quality(), None, None, 0)
        .await
        .map_err(|e| format!("play_track {first_id} failed: {e}"))?;
    refresh_now_playing(runtime).await;
    Ok(())
}

/// When the queue is exhausted and `InfiniteRadio` autoplay is on, build a
/// smart artist radio seeded by the just-finished track and start it, replacing
/// the spent queue. Returns `true` when a radio actually started.
///
/// Port of `playback.rs::try_infinite_refill`, including the reason it does not
/// mirror Tauri literally: Tauri appends the radio and retries `next()`, which
/// cannot work here because `QueueManager::next()` has already nulled
/// `current_index` at the queue edge — the retry would replay the OLD queue
/// from index 1 instead of the radio. Starting the radio fresh reaches the
/// intended behaviour and chains: each radio's end re-seeds the next one, which
/// is what makes it infinite rather than one extra radio.
///
/// `play_track_list` runs the queue through `filter_blacklisted_queue`, so a
/// radio made entirely of blocked artists cannot come back as playback.
async fn try_infinite_refill(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    seed_track_id: u64,
) -> bool {
    if !crate::queue_qt::is_infinite_play() || seed_track_id == 0 {
        return false;
    }
    let artist_id = match runtime.core().get_track(seed_track_id).await {
        Ok(track) => match track.performer.as_ref().map(|p| p.id) {
            Some(id) => id,
            None => {
                log::warn!(
                    "[qbz-qt] infinite radio: seed track {seed_track_id} has no performer"
                );
                return false;
            }
        },
        Err(e) => {
            log::warn!("[qbz-qt] infinite radio: get_track {seed_track_id} failed: {e}");
            return false;
        }
    };
    match runtime.core().create_smart_artist_radio(artist_id).await {
        Ok(tracks) if !tracks.is_empty() => {
            let queue: Vec<QueueTrack> =
                tracks.iter().map(crate::foryou_qt::to_queue_track).collect();
            match play_track_list(runtime, queue, 0, false).await {
                Ok(()) => {
                    log::info!(
                        "[qbz-qt] infinite radio: chained {} track(s) from artist {artist_id}",
                        tracks.len()
                    );
                    true
                }
                Err(e) => {
                    log::warn!("[qbz-qt] infinite radio: play failed: {e}");
                    false
                }
            }
        }
        Ok(_) => false,
        Err(e) => {
            log::warn!("[qbz-qt] infinite radio: build failed: {e}");
            false
        }
    }
}

/// Append a pre-built queue to the current one ("Add all to queue").
pub async fn enqueue_track_list(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    tracks: Vec<QueueTrack>,
) -> Result<(), String> {
    enqueue_track_list_mode(runtime, tracks, "queue").await
}

/// `enqueue_track_list` with the three insertion modes the row/card menus use,
/// mirroring `enqueue_album`: "next" inserts at the cursor (fed REVERSED so a
/// multi-track insert keeps its order), "later" appends to the manual block's
/// tail, anything else appends.
///
/// Exists because ArtistView's ⋯ menu has BOTH "Play all next" and "Add all to
/// queue" and the port wired them to the same append (the Slint arm for
/// `next-all` is `enqueue_artist_top_selected(.., next: true)`, main.rs:15301).
pub async fn enqueue_track_list_mode(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    tracks: Vec<QueueTrack>,
    mode: &str,
) -> Result<(), String> {
    if tracks.is_empty() {
        return Err("empty track list".to_string());
    }
    let tracks = stamped(tracks, None);
    // QConnect CONTROLLER mode (contract §7): route the add to the peer's
    // queue — early-returns when handled, so the local insert + sync tail
    // below only run in local/renderer mode.
    if route_enqueue_to_peer(&tracks, mode).await {
        return Ok(());
    }
    let added_castable = batch_all_qconnect_castable(&tracks);
    match mode {
        "next" => {
            for track in tracks.into_iter().rev() {
                runtime.core().add_track_next(track).await;
            }
        }
        "later" => {
            for track in tracks {
                runtime.core().add_track_later(track).await;
            }
        }
        _ => runtime.core().add_tracks(tracks).await,
    }
    // QConnect sync-on-add (#442): push the updated queue to the session;
    // skipped silently for a non-castable batch.
    sync_qconnect_after_add(added_castable).await;
    publish_queue(runtime).await;
    Ok(())
}

/// ArtistView ⋯ "Play all next" / "Add all to queue" — the whole body, so the
/// invokable in `player_bridge.rs` can call it without a `main.rs` shim (see
/// GLUE in the report).
///
/// The QConnect routed arm + sync tail live in the delegate
/// (`enqueue_track_list_mode`) — arming here too would double-fire the routed
/// send whenever a peer is active.
pub async fn enqueue_artist_top_mode(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    mode: &str,
) -> Result<(), String> {
    let (queue, _) = crate::artist_qt::top_queue(None);
    enqueue_track_list_mode(runtime, queue, mode).await
}

/// Immersive search (artist, next|queue) dispatch (contract
/// 2026-08-02-immersive-port §3.4, round-2-verified mapping): fetch the
/// artist page BY ID and enqueue its top tracks through the shared mode
/// funnel. NEW additive helper — the existing `enqueue_artist_top_mode`
/// reads the ArtistView stash (`artist_qt::top_queue`) and is BANNED from
/// immersive: there is no artist view under the overlay, so the stash would
/// be stale or empty. Unlike `play_artist` there is no discography fallback
/// — the Slint arm (`main.rs:10732-10749`) also enqueues top tracks ONLY.
pub async fn enqueue_artist_top_by_id(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    artist_id: &str,
    mode: &str,
) -> Result<(), String> {
    let id: u64 = artist_id
        .parse()
        .map_err(|_| format!("invalid artist id: {artist_id}"))?;
    let page = runtime
        .core()
        .get_artist_page(id, None)
        .await
        .map_err(|e| format!("get_artist_page {artist_id} failed: {e}"))?;
    let tracks = artist_top_queue_tracks(
        page.top_tracks.as_deref().unwrap_or(&[]),
        artist_id,
        &page.name.display,
    );
    if tracks.is_empty() {
        return Err(format!("artist {artist_id} has no top tracks"));
    }
    enqueue_track_list_mode(runtime, tracks, mode).await
}

/// One track from an album context into the queue ("Play next" / "Add to
/// queue" on an AlbumView row).
pub async fn enqueue_album_track(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
    track_id: u64,
    mode: &str,
) -> Result<(), String> {
    if is_local_album(album_id) {
        // One LOCAL row: the local enqueue resolves it by row id (the cached
        // Tracks page / open detail / library.db), so the group key is only
        // used here to decide the route.
        log::info!("[qbz-qt] enqueue_album_track {album_id}/{track_id}: local/Plex -> local path");
        crate::local_playback::enqueue(
            runtime,
            "track".to_string(),
            track_id.to_string(),
            mode.to_string(),
        )
        .await;
        return Ok(());
    }
    let tracks = fetch_album_queue(runtime, album_id).await?;
    let Some(track) = tracks.into_iter().find(|t| t.id == track_id) else {
        return Err(format!("track {track_id} not in album {album_id}"));
    };
    // QConnect CONTROLLER mode (contract §7): route the add to the peer's
    // queue — early-returns when handled. The mode is normalized to what THIS
    // fn's local match does: only "next" inserts at the cursor, every other
    // mode appends (no block-tail arm here).
    let routed_mode = if mode == "next" { "next" } else { "queue" };
    if route_track_to_peer(&track, routed_mode).await {
        return Ok(());
    }
    let added_castable = batch_all_qconnect_castable(std::slice::from_ref(&track));
    if mode == "next" {
        runtime.core().add_track_next(track).await;
    } else {
        runtime.core().add_track(track).await;
    }
    // QConnect sync-on-add (#442): push the updated queue to the session;
    // skipped silently for a non-castable add.
    sync_qconnect_after_add(added_castable).await;
    publish_queue(runtime).await;
    Ok(())
}

/// Shuffle-play an album (AlbumView header Shuffle): mix the album's own list
/// and start at the top of the mixed order.
///
/// The MODE alone was not a shuffle. `set_shuffle(true)` only randomises what
/// comes NEXT, so `play_album` still started on track 1 — pressing Shuffle on
/// an album played its opener every single time. Owner ruling 2026-08-01:
/// every shuffle must be genuinely random; the flag-and-play-from-the-top
/// shape is an artifact of a very early Tauri experiment. The mode is still
/// raised so playback stays shuffled past this album.
///
/// A LOCAL album keeps going through the local path, which owns its own queue
/// building and does the same mix inside `local_playback::play_rows`.
pub async fn play_album_shuffled(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
) -> Result<(), String> {
    runtime.core().set_shuffle(true).await;
    crate::now_playing::set_shuffle(true);
    if is_local_album(album_id) {
        crate::local_playback::play_album(runtime, album_id.to_string(), None, true).await;
        return Ok(());
    }
    let mut tracks = fetch_album_queue(runtime, album_id).await?;
    xorshift_shuffle(&mut tracks);
    play_track_list_in(runtime, tracks, 0, false, PlayContext::album(album_id)).await
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
        // playback.rs `make_queue_track` (:2426): the CATALOG max travels with
        // the queue track. `None` here zeroed `quality_state`'s `TRACK_MAX_*`
        // seed, so a Library-feed track play drew a bare tier on the NPB
        // AudioStamp with no "24-bit / 96 kHz" line. `library_qt::FeedItem`
        // carries the raw numbers alongside the formatted `quality_detail`.
        hires: item.quality_tier == "hires",
        bit_depth: item.bit_depth,
        sample_rate: item.sample_rate,
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

/// Catalog fallback for a track the Library feed does NOT hold — Search rows
/// and the search hero, Home's "Recently played tracks" rail, anything reached
/// by id from a view that keeps no full `Track`. This is the Qt port of the
/// Slint's last resort, `playback.rs::play_track_now` (:3242-3283): fetch the
/// track and build its one-row queue from the response. Context is left
/// unstamped exactly like the Slint — a bare track play falls back to its own
/// album in `refresh_now_playing`.
async fn catalog_queue_track(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    track_id: u64,
) -> Result<QueueTrack, String> {
    let track = runtime
        .core()
        .get_track(track_id)
        .await
        .map_err(|e| format!("get_track {track_id} failed: {e}"))?;
    let (album_id, album_title, album_artwork) = match track.album.as_ref() {
        Some(album) => (
            album.id.clone(),
            album.title.clone(),
            album.image.best().cloned().unwrap_or_default(),
        ),
        None => (String::new(), String::new(), String::new()),
    };
    let album_key = if album_id.is_empty() {
        None
    } else {
        Some(album_id.clone())
    };
    Ok(QueueTrack {
        id: track.id,
        title: track.title.clone(),
        version: track.version.clone(),
        artist: track
            .performer
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_default(),
        album: album_title,
        album_version: None,
        duration_secs: track.duration as u64,
        artwork_url: if album_artwork.is_empty() {
            None
        } else {
            Some(album_artwork)
        },
        hires: track.hires,
        bit_depth: track.maximum_bit_depth,
        sample_rate: track.maximum_sampling_rate,
        is_local: false,
        album_id: album_key.clone(),
        artist_id: track.performer.as_ref().map(|p| p.id),
        streamable: track.streamable,
        source: Some("qobuz".to_string()),
        parental_warning: track.parental_warning,
        source_item_id_hint: album_key,
        context_kind: None,
        context_id: None,
    })
}

/// One queue row for a bare track id: the Library feed first (free, already
/// resolved), the catalog second. Before the fallback existed, every entry
/// point outside the Library feed — search rows, the search hero, Home's
/// Recently-Played-Tracks rail — returned Err here and rendered a control that
/// did nothing.
async fn queue_track_for(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    track_id: u64,
) -> Result<QueueTrack, String> {
    match feed_queue_track(track_id) {
        Ok(qt) => Ok(qt),
        Err(feed_miss) => {
            log::debug!("[qbz-qt] {feed_miss}; resolving from the catalog");
            catalog_queue_track(runtime, track_id).await
        }
    }
}

/// Play a single track as a one-element queue (Library track rows, Search
/// rows/hero, Home's Recently-Played-Tracks rail). The queue meta comes from
/// the Library feed row when it has one and from `get_track` otherwise; the
/// audio resolves by id through the same `play_track_resolved` path.
pub async fn play_single_track(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    track_id: u64,
) -> Result<(), String> {
    let qt = queue_track_for(runtime, track_id).await?;
    // Bare single-track play: no container origin, so the derive falls to the
    // track's own album — same landing spot as the Slint fallback.
    set_queue_stamped(runtime, vec![qt], Some(0), None).await;
    publish_queue(runtime).await;
    log::info!("[qbz-qt] play_single_track: playing {track_id}");
    // QConnect CONTROLLER mode (§7): route the play to the peer (after the
    // funnel, before the local audible step).
    if route_play_to_peer(runtime, track_id).await {
        return Ok(());
    }
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
    let qt = stamped(vec![queue_track_for(runtime, track_id).await?], None)
        .pop()
        .expect("one track in, one track out");
    // QConnect CONTROLLER mode (contract §7): route the add to the peer's
    // queue — early-returns when handled, so the local insert + sync tail
    // below only run in local/renderer mode.
    if route_track_to_peer(&qt, mode).await {
        return Ok(());
    }
    let added_castable = batch_all_qconnect_castable(std::slice::from_ref(&qt));
    match mode {
        "next" => runtime.core().add_track_next(qt).await,
        "later" => runtime.core().add_track_later(qt).await,
        _ => runtime.core().add_track(qt).await,
    }
    // QConnect sync-on-add (#442): push the updated queue to the session;
    // skipped silently for a non-castable add.
    sync_qconnect_after_add(added_castable).await;
    publish_queue(runtime).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

pub async fn toggle_play(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    // A connected renderer owns transport — the local player is stopped.
    match crate::cast_qt::service().toggle_play_if_cast().await {
        Ok(true) => return,
        Ok(false) => {}
        Err(e) => {
            log::warn!("[qbz-qt][Cast] toggle-play failed: {e}");
            return;
        }
    }
    // QConnect CONTROLLER mode (dispatch priority cast → QConnect → local,
    // Slint main.rs:14238-14247): a peer renderer owns playback — toggle IT.
    // `true` = handled remotely; the local path MUST NOT run. On error there
    // is no local fallback (a peer owns audio; a local resume would
    // double-play).
    if let Some(svc) = crate::qconnect_qt::service() {
        match svc.toggle_remote_renderer_playback_if_active().await {
            Ok(true) => return,
            Ok(false) => {}
            Err(e) => {
                log::warn!("[qbz-qt][QConnect] toggle-play handoff: {e}");
                return;
            }
        }
    }
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
    // QConnect CONTROLLER mode (Slint main.rs:14268-14277): a peer renderer
    // owns playback — skip on IT and return; the local cursor stays put (the
    // cloud echo re-materializes the queue). NOTE: no cast branch here, 1:1
    // with the reference (main.rs:14261-14267) — while casting, the local
    // next() flow runs and the play arm casts the new current track.
    if let Some(svc) = crate::qconnect_qt::service() {
        match svc.skip_next_if_remote().await {
            Ok(true) => return,
            Ok(false) => {}
            Err(e) => {
                log::warn!("[qbz-qt][QConnect] next handoff: {e}");
                return;
            }
        }
    }
    if let Some(track) = runtime.core().next_track().await {
        play_queue_track(runtime, track.id).await;
    }
}

pub async fn previous(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    // QConnect CONTROLLER mode (Slint main.rs:14293-14302) — see next() above.
    if let Some(svc) = crate::qconnect_qt::service() {
        match svc.skip_previous_if_remote().await {
            Ok(true) => return,
            Ok(false) => {}
            Err(e) => {
                log::warn!("[qbz-qt][QConnect] previous handoff: {e}");
                return;
            }
        }
    }
    if let Some(track) = runtime.core().previous_track().await {
        play_queue_track(runtime, track.id).await;
    }
}

pub(crate) async fn play_queue_track_public(runtime: &Arc<AppRuntime<LoggingAdapter>>, track_id: u64) {
    play_queue_track(runtime, track_id).await;
}

async fn play_queue_track(runtime: &Arc<AppRuntime<LoggingAdapter>>, track_id: u64) {
    // CAST owns playback while a renderer is connected: route the new track to
    // it instead of starting the local backend, or both would play.
    if crate::cast_qt::play_current_if_cast(runtime).await {
        refresh_now_playing(runtime).await;
        publish_queue(runtime).await;
        return;
    }
    // QConnect CONTROLLER mode (Slint playback.rs:638-651, AFTER the cast
    // gate, BEFORE the local arms): a peer renderer owns playback — route the
    // play to it (it pushes the current core queue, which the caller's cursor
    // move already updated) and never start the local backend.
    if route_play_to_peer(runtime, track_id).await {
        return;
    }
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
        // The bar must not keep showing the PREVIOUS track while the cursor has
        // already moved. The old early `return` skipped both refreshes, which is
        // how a failed advance left a stale card over a silent player.
        refresh_now_playing(runtime).await;
        publish_queue(runtime).await;
        auto_skip_unavailable(runtime, e).await;
        return;
    }
    // A track actually started: the consecutive-skip budget is spent per RUN of
    // bad tracks, not per session (playback.rs UNAVAILABLE_SKIPS reset).
    UNAVAILABLE_SKIPS.store(0, Ordering::SeqCst);
    refresh_now_playing(runtime).await;
    publish_queue(runtime).await;
}

/// True when a stringified play error means the track cannot play now or ever
/// at any quality, as opposed to a transient network/server failure (those are
/// already retried with backoff inside the client and must NOT cost the user a
/// good track). Verbatim from playback.rs:327-332 — the `ApiError` Display texts
/// survive the `Result<(), String>` flattening, so a substring match is the same
/// pragmatic contract the Tauri frontend used.
fn is_terminal_unavailable(e: &str) -> bool {
    e.contains("no longer available") // ApiError::TrackUnavailable
        || e.contains("not streamable") // ApiError::NonStreamable
        || e.contains("No valid quality available") // ApiError::NoQualityAvailable
}

/// A Qobuz 403 / the client-side 403 back-off (issue #637). NOT the track's
/// fault — every track 403s the same way — so it must not count as an
/// "unavailable" skip. Pre-fix in the Slint this burned through five good
/// tracks while showing a misleading "no longer available"
/// (playback.rs:334-343).
fn is_forbidden_backoff(e: &str) -> bool {
    e.contains("Access forbidden by Qobuz") // ApiError::Forbidden
        || e.contains("backing off after repeated 403s") // ApiError::ForbiddenCircuitOpen
}

/// Consecutive auto-skips over tracks whose play failed TERMINALLY (Tauri #467
/// parity: `MAX_CONSECUTIVE_SKIPS = 5`). Reset the moment any track starts.
static UNAVAILABLE_SKIPS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
const MAX_UNAVAILABLE_SKIPS: u32 = 5;

/// One unavailable track must not wedge the whole queue (PARITY-DEBT #2 —
/// Tauri #467, re-introduced by this port). Port of `auto_skip_unavailable`
/// (playback.rs:766-818).
///
/// The signature RETURNS a boxed `dyn Future + Send` rather than being an
/// `async fn`, for the reason the reference documents: the recursion makes the
/// future's Send-ness self-referential and an inferred `impl Future` type sends
/// the compiler into a query cycle. Declaring the concrete boxed type is what
/// cuts it.
fn auto_skip_unavailable<'a>(
    runtime: &'a Arc<AppRuntime<LoggingAdapter>>,
    error: String,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        // A 403 / back-off is not this track's fault: stop cleanly and say so
        // instead of eating five good tracks looking for one that works.
        if is_forbidden_backoff(&error) {
            crate::toast_qt::error(qbz_i18n::t("Access forbidden by Qobuz"));
            crate::now_playing::set_playing(false);
            return;
        }
        // Anything that is not TERMINALLY unavailable is transient — the client
        // already retried it. Do not skip a good track over a network blip.
        if !is_terminal_unavailable(&error) {
            return;
        }
        crate::toast_qt::warning(qbz_i18n::t("This track is no longer available"));

        let skips = UNAVAILABLE_SKIPS.fetch_add(1, Ordering::SeqCst) + 1;
        if skips > MAX_UNAVAILABLE_SKIPS {
            log::warn!(
                "[qbz-qt] playback: {MAX_UNAVAILABLE_SKIPS} consecutive unavailable tracks — stopping the skip walk"
            );
            crate::toast_qt::warning(qbz_i18n::t("No available tracks to play"));
            crate::now_playing::set_playing(false);
            UNAVAILABLE_SKIPS.store(0, Ordering::SeqCst);
            return;
        }
        let Some(track) = runtime.core().next_track().await else {
            // Queue edge reached while skipping — stop quietly, the warning
            // above already told the user why.
            crate::now_playing::set_playing(false);
            UNAVAILABLE_SKIPS.store(0, Ordering::SeqCst);
            return;
        };
        log::info!(
            "[qbz-qt] advance: skipping unavailable track, trying {} ({skips}/{MAX_UNAVAILABLE_SKIPS})",
            track.id
        );
        play_queue_track(runtime, track.id).await;
    })
}

pub async fn seek_frac(runtime: &Arc<AppRuntime<LoggingAdapter>>, frac: f32) {
    match crate::cast_qt::service().seek_fraction_if_cast(frac as f64).await {
        Ok(true) => return,
        Ok(false) => {}
        Err(e) => {
            log::warn!("[qbz-qt][Cast] seek failed: {e}");
            return;
        }
    }
    // QConnect CONTROLLER mode (Slint main.rs:14334-14351): seek the REMOTE
    // renderer. The remote API wants absolute position in MILLISECONDS; the
    // bar gives a 0..1 fraction — derive ms from the locally-known duration
    // (SECONDS here, hence the ×1000).
    if let Some(svc) = crate::qconnect_qt::service() {
        let fraction = frac.clamp(0.0, 1.0);
        let duration_secs = runtime.core().get_playback_state().duration;
        let position_ms =
            (fraction as f64 * duration_secs as f64 * 1000.0).round() as i64;
        match svc.set_position_if_remote(position_ms).await {
            Ok(true) => return,
            Ok(false) => {}
            Err(e) => {
                log::warn!("[qbz-qt][QConnect] seek handoff: {e}");
                return;
            }
        }
    }
    let event = runtime.core().player().get_playback_event();
    if event.duration == 0 {
        return;
    }
    let target = (frac.clamp(0.0, 1.0) * event.duration as f32) as u64;
    if let Err(e) = runtime.core().seek(target) {
        log::warn!("[qbz-qt] seek failed: {e}");
        return;
    }
    // Tell the desktop where we jumped to. Without this the MPRIS position is
    // only ever refreshed on the play/pause edge, so a widget extrapolates
    // from a base that is now wrong by the whole seek distance — the owner
    // measured ~40 s of lag on 2026-08-04 after seeking forward.
    //
    // The shared crate exposes no `Seeked` emitter (`types.rs`), and
    // `set_playback` already carries a position, so the honest fix inside the
    // current API is to re-assert the state at the new position. The SHIPPING
    // Slint build has the same gap and the same cure would apply there — that
    // is a shared-crate conversation, not this one.
    crate::media_controls_qt::push_playback_state(
        runtime.core().player().get_playback_event().is_playing,
        target,
    );
}

pub async fn set_volume(runtime: &Arc<AppRuntime<LoggingAdapter>>, volume: f32) {
    if matches!(crate::cast_qt::service().set_volume_if_cast(volume).await, Ok(true)) {
        return;
    }
    // QConnect CONTROLLER mode (Slint main.rs:14378-14389): set the REMOTE
    // renderer's volume. The remote API wants 0..100; the bar gives a 0..1
    // fraction. TRAP (§12.30): a volume-DISALLOWING peer returns Ok(true) —
    // handled NO-OP — so a drag on a volume-locked peer never falls through
    // to change the LOCAL volume.
    if let Some(svc) = crate::qconnect_qt::service() {
        let remote_volume = (volume.clamp(0.0, 1.0) * 100.0).round() as i32;
        match svc.set_volume_if_remote(remote_volume).await {
            Ok(true) => return,
            Ok(false) => {}
            Err(e) => {
                log::warn!("[qbz-qt][QConnect] set-volume handoff: {e}");
                return;
            }
        }
    }
    let _ = runtime.core().set_volume(volume.clamp(0.0, 1.0));
    MUTED.store(volume <= 0.0 && PREMUTE_VOLUME.load(Ordering::Relaxed) != 0, Ordering::Relaxed);
}

pub async fn toggle_mute(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    // QConnect FIRST (Slint main.rs:14455-14478 — the ONE dispatch that checks
    // QConnect before cast; the reference has NO cast arm for mute). The
    // remote API wants the target value; send the negation of the
    // authoritative local MUTED flag.
    if let Some(svc) = crate::qconnect_qt::service() {
        let target = !MUTED.load(Ordering::Relaxed);
        match svc.mute_if_remote(target).await {
            Ok(true) => return,
            Ok(false) => {}
            Err(e) => {
                log::warn!("[qbz-qt][QConnect] toggle-mute handoff: {e}");
                return;
            }
        }
    }
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
    // QConnect (Slint main.rs:14492-14501 — no cast arm in the reference):
    // the CLOUD owns shuffle order while connected (gated on
    // transport_connected inside the service — §12.14, no extra gate here);
    // QBZ applies only the echo. `true` = handled remotely — a local reorder
    // would diverge from the cloud's order.
    if let Some(svc) = crate::qconnect_qt::service() {
        match svc.toggle_shuffle_if_remote().await {
            Ok(true) => return,
            Ok(false) => {}
            Err(e) => {
                log::warn!("[qbz-qt][QConnect] toggle-shuffle handoff: {e}");
                return;
            }
        }
    }
    let enabled = runtime.core().toggle_shuffle().await;
    crate::now_playing::set_shuffle(enabled);
    publish_queue(runtime).await;
}

pub async fn cycle_repeat(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    // QConnect (Slint main.rs:14517-14526 — no cast arm in the reference):
    // gated on transport_connected inside the service (§12.14).
    if let Some(svc) = crate::qconnect_qt::service() {
        match svc.cycle_repeat_if_remote().await {
            Ok(true) => return,
            Ok(false) => {}
            Err(e) => {
                log::warn!("[qbz-qt][QConnect] cycle-repeat handoff: {e}");
                return;
            }
        }
    }
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
        // Tray tooltip: the no-track arm. 1:1 with the reference, where
        // `clear_track()` (crates/qbz/src/playback.rs:1918) and `set_track(…)`
        // (`:2160`) are the two arms of ONE publisher,
        // `refresh_now_playing_meta` (`:1895`) — this function is its Qt
        // analogue, and BOTH pushes live here. Silent no-op with no tray.
        // Clearing also drops `is_playing`, so the middle-click hint cannot go
        // stale.
        if let Some(t) = crate::tray_qt::handle() {
            t.clear_track();
        }
        // OS media controls: the no-track arm, a SIBLING of the tray clear
        // above and in the reference's order — dedupe reset, tray, MPRIS
        // (crates/qbz/src/playback.rs:1913-1922). Tells the Plasma/GNOME widget
        // we stopped and resets the metadata dedupe, so replaying the SAME
        // track after a stop pushes its metadata again. Silent no-op when the
        // integration never started.
        crate::media_controls_qt::clear_now_playing();
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
    // Tray tooltip: the has-track arm (§5.6). Edge-triggered — this publisher
    // runs on a track change, never per tick (§7-M12).
    //
    // The three strings are exactly the reference's
    // (crates/qbz/src/playback.rs:2163-2165): the VERSIONED title, the artist,
    // and the CLEAN album — `track.album`, NOT the release-variant
    // `album_display` built below, which `playback.rs:1937` / `:1941-1948`
    // deliberately keeps as a separate value.
    //
    // It goes in THIS function and not in the poll loop's de-duped track edge:
    // `refresh_now_playing` has 23 call sites across nine modules, so binding
    // the tooltip to the poll loop would leave it stale on every QConnect
    // handoff, cast event and click-to-play (§15 trap 34).
    if let Some(t) = crate::tray_qt::handle() {
        t.set_track(title.clone(), track.artist.clone(), track.album.clone());
    }
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
    // OS media controls: the has-track arm, the second SIBLING of the tray push
    // above (crates/qbz/src/playback.rs:2208-2223). This is what the KDE Plasma
    // now-playing widget, GNOME's media section and playerctl read.
    //
    // It sits HERE, after `album_display` is built and BEFORE `title` is moved
    // into the NowPlayingModel meta below, because MPRIS wants the pair the
    // reference sends: the VERSIONED title and the release-variant album — not
    // the clean `track.album` the tray tooltip takes (`:2213` vs `:2164`).
    //
    // Metadata is de-duped on (track id, resolved art) INSIDE the callee: this
    // publisher re-runs on resume/seek/republish from 23 call sites, so an
    // unconditional push would re-emit PropertiesChanged for identical values.
    // The playback push is unconditional and optimistic (Playing at 0) exactly
    // as in the reference — the poll loop's play/pause edge corrects it if the
    // stream never opens.
    crate::media_controls_qt::push_now_playing(&track, &title, &album_display);
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

/// Quality tier + exact label from a queue track's bit depth / sample rate.
///
/// 1:1 with `playback.rs:2025-2042` (`refresh_now_playing_meta`), which this
/// had forked from in three ways, each of them visible on the NPB AudioStamp:
///
///  - **DSD.** `bit_depth == Some(1)` marks a DSD stream (1-bit, `sample_rate`
///    = the DSD bit rate). The reference tiers it "hires" and labels it
///    through `dsd_multiple_label` ("DSD64"); the forked version tiered it
///    "cd" and printed "1-bit / 2822.4 kHz".
///  - **Hz vs kHz.** `quality_state::detail` normalizes either unit (the
///    `>= 1000` guard) exactly like the track rows do, so the stamp and the
///    row can never disagree. The forked `format!` printed a raw Hz value as
///    "44100 kHz".
///  - **The empty-detail gate.** The reference blanks the line only when the
///    TIER is unknown or BOTH params are missing; the fork blanked it whenever
///    either one was missing, so a track with a known rate and no depth lost
///    its whole detail line instead of taking the formatter's depth default.
fn quality_badge(track: &QueueTrack) -> (String, String) {
    let tier = match track.bit_depth {
        Some(1) => "hires",
        Some(d) if d >= 24 => "hires",
        Some(_) => "cd",
        None if track.hires => "hires",
        None => "",
    }
    .to_string();
    let label = if tier.is_empty() || (track.bit_depth.is_none() && track.sample_rate.is_none()) {
        String::new()
    } else if track.bit_depth == Some(1) {
        crate::quality_state::dsd_multiple_label(track.sample_rate)
    } else {
        crate::quality_state::detail(track.bit_depth, track.sample_rate)
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

/// Wall-clock now in ms — the peer-position extrapolation clock (the Slint
/// `now_ms`, playback.rs:5162).
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

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
        // Track id we have already fired a gapless prefetch for, so this poll
        // does not re-request it on every tick while the download is in flight
        // (playback.rs:5049-5051).
        let mut gapless_requested_for: u64 = 0;

        // QConnect renderer-report throttle (contract §6.2): the official
        // client reports RndrSrvrStateUpdated ~every 2s while playing PLUS
        // immediately on a transition (track / play-state change). The Slint
        // ticks at 450ms and reports every 4 ticks ≈ 1.8s
        // (playback.rs:5053-5059); this loop ticks at 1 Hz, so the wall-clock
        // equivalent is every 2 ticks — tick-count throttles do NOT translate
        // 1:1, translate by wall time.
        let mut last_reported_track_id: u64 = 0;
        let mut last_reported_playing = false;
        let mut report_tick: u64 = 0;
        const QCONNECT_REPORT_EVERY_N_TICKS: u64 = 2;

        // QConnect CONTROLLER mode (contract §6.1): the peer's last-seen
        // current track id. When the peer advances a track on its own, this
        // edge-detects the change so the bar/queue meta refresh from the core
        // cursor (which the sink already aligned to the peer's track). Reset
        // to 0 when the peer branch is not taken so re-entering controller
        // mode refreshes meta (playback.rs:5061-5066).
        let mut last_peer_track_id: u64 = 0;
        // Dirty-guard for the per-tick peer UI pushes (playback.rs:5194-5204):
        // (track_id, position_ms, duration_secs, playing, volume f32 bits,
        // shuffle, repeat_mode). Re-pushing identical values every second
        // dirties bindings and repaints even when fully idle. `track_id` is
        // load-bearing: it guarantees the push after a peer track change (the
        // meta refresh just reset the bar's position to 0). Reset to None
        // whenever another owner (local/cast poll) may have written the bar.
        let mut last_remote_ui_push: Option<(u64, u64, u64, bool, u32, bool, i32)> = None;

        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(1));
        loop {
            ticker.tick().await;

            // --- Stream-failure toast (PARITY-DEBT #7) -------------------
            // Surface audio-stream failures as a toast, 1:1 with
            // `playback.rs:5093-5115` (#508/#534/#500). The player records a
            // user-readable message when a stream fails to open; this port
            // drained the slot and only logged it, so an ALSA Direct /
            // exclusive-mode failure left the bar frozen at 0:00 with no
            // explanation — the support issue the Slint fix was written for.
            // take_stream_error_message() drains exactly once per recorded
            // error, so each failed attempt toasts once.
            if let Some(msg) = runtime.core().player().state.take_stream_error_message() {
                log::error!("[qbz-qt] audio output error: {msg}");
                let sandboxed = !matches!(
                    qbz_audio::health::detect_sandbox(),
                    qbz_audio::health::Sandbox::None
                );
                crate::toast_qt::error(stream_error_text(&msg, sandboxed));
            }

            // --- QConnect CONTROLLER mode: peer-state reflection ----------
            // (contract §6.1 — Slint playback.rs:5117-5244, inserted BEFORE
            // the cast branch exactly like the reference.) When QBZ is
            // CONTROLLING a peer renderer, the event sink stops the LOCAL
            // player, so `get_playback_event()` reports track_id == 0 / not
            // playing and the seek bar would freeze. While a peer owns
            // playback, drive the bar from the peer's renderer snapshot
            // instead: title / artist / art come from the materialized local
            // core queue (the sink aligns the core cursor to the peer's
            // track), only position / playing / duration come from the peer.
            // Returns None in every NON-controller situation (disconnected,
            // renderer mode where active == local, no active renderer), so
            // the local path below runs byte-unchanged.
            //
            // UNITS (contract §6): `RemoteNowPlaying.position_ms` /
            // `updated_at_ms` are MILLISECONDS; `now_playing::set_position`
            // takes SECONDS. Convert at the publish boundary.
            if let Some(remote) = match crate::qconnect_qt::service() {
                Some(svc) => svc.remote_now_playing().await,
                None => None,
            } {
                // The peer changed track on its own → refresh the bar/queue
                // meta from the core cursor (the event sink already aligned
                // it to the peer's track via sync_active_renderer_projection).
                // This resets position to 0, which is correct on a track
                // change; the per-tick position push below immediately
                // re-applies the peer's real position. Done BEFORE the
                // per-tick push so peer values win.
                if remote.track_id != last_peer_track_id {
                    refresh_now_playing(&runtime).await;
                    publish_queue(&runtime).await;
                    // Discord follows the peer (§11.2): the peer track edge
                    // pushes Discord but NEVER scrobbles — 1:1 with the
                    // Slint, where this edge reaches the UNGUARDED push
                    // inside refresh_now_playing_meta (playback.rs:5137-5141
                    // -> :2236-2239). The scrobble half of the LOCAL edge is
                    // peer-guarded inside `on_track_change_edge` instead.
                    crate::integrations_qt::discord_push(&runtime);
                    last_peer_track_id = remote.track_id;
                }
                // Lyrics follow the peer (Q7, §11.1): publish the RAW
                // renderer anchor on EVERY peer-branch tick — the lyrics
                // sync invokables extrapolate between poll ticks exactly
                // like the position extrapolation below
                // (playback.rs:5142-5149). Outside the track edge AND the
                // dirty guard on purpose: re-anchoring a peer pause promptly
                // beats a skipped UI push.
                crate::lyrics_qt::publish_remote_anchor(
                    remote.position_ms,
                    remote.updated_at_ms,
                    remote.playing,
                );
                // Duration from the core queue's current track (aligned to the
                // peer's track by the sink). Zero when unknown — the clamp is
                // skipped then; without it the seekbar is wrong.
                let duration_secs = runtime
                    .core()
                    .current_track()
                    .await
                    .map(|track| track.duration_secs)
                    .unwrap_or(0);
                let duration_ms = duration_secs.saturating_mul(1000);
                // Extrapolate position while playing; clamp to the track length.
                let mut position_ms = remote.position_ms;
                if remote.playing && remote.updated_at_ms > 0 {
                    position_ms = position_ms
                        .saturating_add(now_ms().saturating_sub(remote.updated_at_ms));
                }
                if duration_ms > 0 && position_ms > duration_ms {
                    position_ms = duration_ms;
                }
                // ms → s at the publish boundary (see UNITS above).
                let position_secs = position_ms / 1000;
                let playing = remote.playing;
                // Reflect the PEER's actual volume on the bar so a drag starts
                // from a safe level (never QBZ's local 100). When the peer
                // hasn't reported a volume, clamp to 50% — the AVR-nuke
                // safety default (playback.rs:5174-5180, §12.13).
                let remote_volume = remote
                    .volume
                    .map(|v| (v as f32 / 100.0).clamp(0.0, 1.0))
                    .unwrap_or(0.5);
                // Reflect the PEER's shuffle/repeat state on the bar buttons.
                // Pure UI reflection of the cloud's reported state — no local
                // order is generated (WS-authoritative for shuffle order).
                let shuffle_on = remote.shuffle_mode;
                let repeat_mode = remote.repeat_mode;
                // Skip the UI hop when nothing the push depends on changed
                // since the last push (see the dirty-guard comment at the
                // trackers). While playing, `position_ms` extrapolates every
                // tick, so pushes proceed; paused, the bar stops repainting.
                let remote_snapshot = (
                    remote.track_id,
                    position_ms,
                    duration_secs,
                    playing,
                    remote_volume.to_bits(),
                    shuffle_on,
                    repeat_mode,
                );
                if last_remote_ui_push != Some(remote_snapshot) {
                    last_remote_ui_push = Some(remote_snapshot);
                    // Keep the visualizer producer in step with the same flag
                    // the drain gate reads (a paused PEER parks it too;
                    // playback.rs:5205-5209).
                    crate::viz_qt::set_paused(!playing);
                    // The peer branch `continue`s past the local body that
                    // re-sets the seek lock every tick — force it open here
                    // (playback.rs:5219) or a mid-stream takeover would latch
                    // the last buffer fraction and refuse routed seeks.
                    crate::now_playing::set_seekable_max(1.0);
                    // `has_audio = true` clears the fetch spinner: a peer owns
                    // audio, so there is no local fetch wait and the bar's
                    // spinner must never linger here (the Slint force-clears
                    // PENDING_PLAY_ID on the peer edge, playback.rs:5228-5233
                    // — Qt has no PENDING_PLAY_ID/clear_loading; this push is
                    // guaranteed on the edge because track_id is in the
                    // dirty-guard tuple). Cache overlay 0.0 = "not streaming",
                    // same value the cast poll publishes (cast_qt.rs:1000).
                    crate::now_playing::set_position(
                        position_secs as i32,
                        duration_secs as i32,
                        playing,
                        0.0,
                        true,
                    );
                    crate::now_playing::set_volume(remote_volume);
                    crate::now_playing::set_shuffle(shuffle_on);
                    crate::now_playing::set_repeat_mode(repeat_mode);
                }
                // Reset the LOCAL edge trackers so when control returns to QBZ
                // the end-of-track / gapless / transition logic re-detects
                // from a clean slate (the local player was stopped while
                // peer-active).
                last_track_id = 0;
                was_playing = false;
                seen_position = 0;
                continue;
            }

            // CAST: the cast service runs its own 1 s poll and the local player is
    // stopped, so this path would push position 0 / not-playing every
    // second and fight it.
    if crate::cast_qt::is_casting().await {
        last_track_id = 0;
        was_playing = false;
        seen_position = 0;
        // Same invariant for the PEER edge trackers (playback.rs:5268-5269):
        // the remote branch above did not run, so without this reset a
        // re-taken peer whose snapshot happens to match the last controller
        // push would be skipped, leaving the cast renderer's values on the
        // bar (mirrors the fall-through reset below).
        last_peer_track_id = 0;
        last_remote_ui_push = None;
        // A cast target owns playback and there is no local download to
        // outrun: seek is unlocked while casting, the way the Slint cast
        // publish forces it (`cast_service.rs:1118`). Without this the last
        // local stream's fraction would stay latched on the bar.
        crate::now_playing::set_seekable_max(1.0);
        continue;
    }
    // Not in controller mode (no peer / returned to local): reset the
    // peer-track edge var so re-entering the peer state refreshes meta
    // (playback.rs:5274-5277).
    last_peer_track_id = 0;
    last_remote_ui_push = None;
    // §11.1: deliberately NO lyrics anchor clear here (the Slint clears
    // per-tick at playback.rs:5279) — the Qt anchor clears from the single
    // `now_playing::set_remote(false)` choke point, which unlike this
    // fallthrough also runs while the cast branch owns the loop. See the
    // note in `now_playing::set_remote`.
    let event = runtime.core().player().get_playback_event();
            let track_id = event.track_id;
            let position = event.position;
            let duration = event.duration;
            let is_playing = event.is_playing;
            let cache = event.buffer_progress.unwrap_or(0.0);
            // Seek lock (PARITY-DEBT #15, playback.rs:5304): while streaming
            // (`buffer_progress` is Some) the user can only seek up to what
            // has downloaded; a fully-available track (None) seeks freely.
            // `cache` above is the DECORATIVE overlay of the same fill — this
            // is the value the seek bars must enforce.
            let seekable_max = event.buffer_progress.map(|p| p.clamp(0.0, 1.0)).unwrap_or(1.0);

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
                //
                // EPHEMERAL (owner rule): an ephemeral folder "solo se reproduce
                // y ya" and "no deja rastros en las bibliotecas" — it may be
                // added to the queue and to nothing else, so a play of it must
                // NOT land in Recently Played / Most Played. The player carries
                // the synthetic id verbatim (`local_ephemeral::play_file` hands
                // `row_id` to the engine), so the poll's own `track_id` is the
                // cheapest place to decide, before the queue-state fetch.
                // `record_queue_track` re-checks the same predicate, so the rule
                // holds even if another call site appears.
                if !crate::local_ephemeral::is_ephemeral_id(track_id as i64) {
                    if let Some(track) = runtime.core().get_queue_state().await.current_track {
                        crate::recently_qt::record_queue_track(&track);
                    }
                }
                last_track_id = track_id;
                // The engine may have reached this track through a GAPLESS
                // hand-off, in which case the arming guard still names the
                // track that just ended. Clear it so the NEW current track can
                // arm its own prefetch (playback.rs:5353).
                gapless_requested_for = 0;
            }

            if track_id != 0 {
                log::debug!("[qbz-qt] poll: id={track_id} pos={position}/{duration} playing={is_playing} cache={cache:.2}");
            }

            // --- Position / state push ------------------------------------
            // Seek lock first, so the bar never renders a position push whose
            // limit still belongs to the previous tick (the setter dedupes,
            // so a fully-available track costs no extra hop).
            crate::now_playing::set_seekable_max(seekable_max);
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

            // --- Gapless prefetch trigger (playback.rs:5362-5476) ---------
            // The audio engine ASKS for the next track ~10s before the end of
            // the current one ("Gapless: approaching end of track", player/
            // mod.rs:3927) by raising `gapless_ready`. Nothing answered it in
            // this port, so every advance went through the end-of-track edge
            // below instead: fetch AFTER silence, ~1s of nothing plus the
            // initial-buffer wait. The engine, the setting and the CMAF path
            // were all working — this seam simply was not wired.
            //
            // Answering means handing the engine the next track's BYTES while
            // the current one still plays, via `Player::play_next`, which
            // appends to the same rodio sink and is what makes the transition
            // sample-accurate.
            // The stop-after guard is last and short-circuited. It is LIVE now
            // (the queue row menu can place the marker): without it the engine
            // would hand the next track's bytes over seamlessly and the marker
            // would never fire, which is exactly what playback.rs:5372-5377
            // warns about.
            if event.gapless_ready
                && event.gapless_next_track_id == 0
                && track_id != 0
                && gapless_requested_for != track_id
                && runtime.core().get_stop_after().await != Some(track_id)
            {
                let upcoming = runtime.core().peek_upcoming(1).await;
                if let Some(next) = upcoming.into_iter().next() {
                    // Never queue the current track as its own successor.
                    if next.id != track_id && !next.is_local {
                        gapless_requested_for = track_id;
                        let runtime = runtime.clone();
                        let next_id = next.id;
                        tokio::spawn(async move {
                            // L1/L2 player cache -> network. The offline tier is
                            // `None` here for the same reason every other play
                            // path in this module passes `None`: this port opens
                            // no offline-cache index.
                            if let Some(data) = runtime
                                .core()
                                .fetch_for_gapless_resolved(next_id, current_quality(), None, None)
                                .await
                            {
                                let player = runtime.core().player();
                                match player.play_next(data, next_id) {
                                    Ok(()) => log::info!(
                                        "[qbz-qt] [GAPLESS] queued track {next_id} for gapless"
                                    ),
                                    Err(e) => log::warn!(
                                        "[qbz-qt] [GAPLESS] play_next {next_id} failed: {e}"
                                    ),
                                }
                            }
                        });
                    } else if next.id != track_id && next.is_local {
                        // LOCAL gapless: resolve the file path and hand it to
                        // the engine's gapless queue — DSD through
                        // `play_next_dsd` (DoP append when a DoP stream is
                        // live, else an in-memory converted WAV), everything
                        // else as raw bytes. CUE virtual tracks are skipped:
                        // they share one file and the engine would append the
                        // whole album image.
                        gapless_requested_for = track_id;
                        let runtime = runtime.clone();
                        let next_id = next.id;
                        tokio::spawn(async move {
                            let info = if crate::local_ephemeral::is_ephemeral_id(next_id as i64) {
                                crate::local_ephemeral::get_track(next_id as i64)
                                    .map(|t| (t.file_path, t.cue_start_secs))
                            } else {
                                tokio::task::spawn_blocking(move || {
                                    crate::local_state::with_db(|db| db.get_track(next_id as i64))
                                        .flatten()
                                        .map(|t| (t.file_path, t.cue_start_secs))
                                })
                                .await
                                .ok()
                                .flatten()
                            };
                            let Some((path, cue)) = info else { return };
                            if cue.is_some() {
                                return;
                            }
                            let res = tokio::task::spawn_blocking(move || {
                                let p = std::path::PathBuf::from(&path);
                                let player = runtime.core().player();
                                // The reference calls `qbz_dsd::is_dsd_path`,
                                // but `qbz-dsd` is not a dependency of this
                                // crate and `local_playback.rs:139` already
                                // makes the same decision from the extension.
                                // Same two extensions, no new dependency.
                                let lower = p.to_string_lossy().to_lowercase();
                                if lower.ends_with(".dsf") || lower.ends_with(".dff") {
                                    player.play_next_dsd(p, next_id)
                                } else {
                                    let bytes = std::fs::read(&p).map_err(|e| e.to_string())?;
                                    player.play_next(bytes, next_id)
                                }
                            })
                            .await;
                            match res {
                                Ok(Ok(())) => log::info!(
                                    "[qbz-qt] [GAPLESS] queued local track {next_id} for gapless"
                                ),
                                Ok(Err(e)) => log::info!(
                                    "[qbz-qt] [GAPLESS] local pre-queue {next_id} skipped: {e}"
                                ),
                                Err(e) => {
                                    log::warn!("[qbz-qt] [GAPLESS] local pre-queue task failed: {e}")
                                }
                            }
                        });
                    }
                }
            }

            // --- QConnect: outbound renderer state report -----------------
            // (contract §6.2 — Slint playback.rs:5676-5710, placed after the
            // position push / before the end-of-track edge exactly like the
            // reference.) When QBZ is the ACTIVE LOCAL renderer (controlled by
            // a remote controller like the iOS app), report our playback state
            // so the controller's seek bar + current-track follow. Mirrors the
            // official client: position/duration in MILLISECONDS (the Qt poll
            // position is SECONDS — ×1000 at the call), ~2s periodic while
            // playing + an immediate report on every transition. The service
            // self-gates on is_local_renderer_active (no-op when we're a peer
            // controller or not connected) and resolves the queue_item_ids —
            // no extra gates.
            report_tick = report_tick.wrapping_add(1);
            if track_id != 0 {
                let transition =
                    track_id != last_reported_track_id || is_playing != last_reported_playing;
                let periodic = is_playing && report_tick % QCONNECT_REPORT_EVERY_N_TICKS == 0;
                if transition || periodic {
                    if let Some(svc) = crate::qconnect_qt::service() {
                        let playing_state = if is_playing {
                            qconnect_app::renderer::PLAYING_STATE_PLAYING
                        } else {
                            qconnect_app::renderer::PLAYING_STATE_PAUSED
                        };
                        let position_ms = (position as i64) * 1000;
                        let duration_ms = (duration as i64) * 1000;
                        svc.report_playback_state(playing_state, position_ms, duration_ms, track_id)
                            .await;
                        // On a track change, also reconcile the session queue:
                        // if the user started a new album/playlist on QBZ,
                        // push it so the controller follows. Self-gates +
                        // echo-suppresses.
                        if transition {
                            svc.sync_local_queue_if_changed().await;
                        }
                    }
                    last_reported_track_id = track_id;
                    last_reported_playing = is_playing;
                }
            }

            // --- End-of-track edge (playback.rs condition, verbatim) ------
            let track_ended = was_playing
                && !is_playing
                && last_track_id != 0
                && (track_id == 0 || track_id == last_track_id)
                && duration > 0
                && seen_position + 2 >= duration;
            if track_ended {
                // Seed for InfiniteRadio, read BEFORE anything moves the
                // cursor: the track that just ended is still the current one.
                let ended_track_id = runtime
                    .core()
                    .current_track()
                    .await
                    .map(|t| t.id)
                    .unwrap_or(0);
                // Stop-after-this-song: if the track that just ended carries
                // the marker, HALT here — do not advance, do not refill. The
                // queue stays intact and the finished track stays parked in
                // now-playing, so pressing play resumes from where the user
                // left off. `consume_stop_after_if` is one-shot (it clears the
                // marker), and this runs AHEAD of repeat/shuffle exactly as
                // the reference does (playback.rs:5749-5772) — behind them the
                // marker would lose to a repeat-one and never fire.
                if ended_track_id != 0
                    && runtime.core().consume_stop_after_if(ended_track_id).await
                {
                    if let Err(e) = runtime.core().pause() {
                        log::warn!("[qbz-qt] stop-after: pause failed: {e}");
                    }
                    last_track_id = 0;
                    seen_position = 0;
                    gapless_requested_for = 0;
                    crate::now_playing::set_playing(false);
                    // Republish so the row drops its CircleStop in the same
                    // frame the marker is consumed.
                    crate::queue_qt::publish(&runtime).await;
                    // The tail-of-loop bookkeeping, inlined because this arm
                    // returns to the top early and must NOT run
                    // `seen_position = position` (the reset above is the
                    // point). Dropping the Discord push here would leave the
                    // presence saying "playing" after the timer stopped us. The
                    // tray tooltip's middle-click hint has the identical
                    // problem — it would keep saying "Middle-click to pause"
                    // after the stop-after timer fired — so its push is a
                    // sibling here, exactly as in the reference, where tray
                    // (playback.rs:5719-5722) and Discord (`:5734`) share one
                    // `if`. THIS IS ONE OF TWO `is_playing != was_playing`
                    // BLOCKS; a push in only one is the silent half-port
                    // (§5.6, AC-27).
                    if is_playing != was_playing {
                        if let Some(t) = crate::tray_qt::handle() {
                            t.set_playing(is_playing);
                        }
                        // OS media controls, inserted between the tray and
                        // Discord exactly as in the reference, whose single
                        // block is tray (playback.rs:5720-5722), MPRIS
                        // (`:5723-5730`), Discord (`:5734`). Carries the live
                        // position, so this transition is also the only thing
                        // that moves the widget's Position — nothing per tick.
                        // SIBLING OF THE BLOCK AT THE LOOP TAIL: a push in only
                        // one of the two is the silent half-port.
                        crate::media_controls_qt::push_playback_state(is_playing, position);
                        crate::integrations_qt::discord_push(&runtime);
                    }
                    was_playing = is_playing;
                    continue;
                }
                last_track_id = 0;
                // The skip-unavailable walk is ported: a failed play inside
                // `play_queue_track` hands off to `auto_skip_unavailable`
                // (PARITY-DEBT #2).
                if let Some(track) = runtime.core().next_track().await {
                    let next_id = track.id;
                    log::info!("[qbz-qt] poll: advancing to {next_id}");
                    play_queue_track(&runtime, next_id).await;
                } else if try_infinite_refill(&runtime, ended_track_id).await {
                    // InfiniteRadio started a fresh smart radio; it replaced
                    // the queue and published on its way in.
                } else {
                    log::info!("[qbz-qt] poll: queue finished");
                    crate::now_playing::set_playing(false);
                }
            }

            seen_position = position;
            // Reflect play/pause into the tray tooltip on the TRANSITION only,
            // so the "Middle-click to pause/play" hint stays correct without
            // spamming the updater channel every tick (§7-M12). The second of
            // the two `is_playing != was_playing` blocks — see the sibling in
            // the stop-after arm above.
            if is_playing != was_playing {
                if let Some(t) = crate::tray_qt::handle() {
                    t.set_playing(is_playing);
                }
                // OS media controls — the SECOND of the two blocks, see the
                // sibling in the stop-after arm above. Same order as the
                // reference: tray, MPRIS, Discord.
                crate::media_controls_qt::push_playback_state(is_playing, position);
                crate::integrations_qt::discord_push(&runtime);
            }
            was_playing = is_playing;
        }
    });
}

// ---------------------------------------------------------------------------
// Tests (pure functions only — no Qt, no engine, no session)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{filter_queue_with, stream_error_text, xorshift_shuffle_seeded};

    // --- PARITY-DEBT #17: the shuffle flag reorders, it does not switch mode
    //
    // The permutation itself is part of the contract being ported: the
    // reference walks Fisher–Yates DOWNWARD (`for i in (1..len).rev()`) over
    // an xorshift64 stream seeded once. A different loop direction or a
    // different mix order is still "random" and still passes a smoke test, so
    // it is pinned here against an independently-computed expectation.

    /// Reference permutation for seed 0x2545F4914F6CDD1D over 0..8, computed
    /// from the algorithm at `playback.rs:3131-3144`.
    #[test]
    fn xorshift_shuffle_matches_the_reference_permutation() {
        let mut items: Vec<u32> = (0..8).collect();
        xorshift_shuffle_seeded(&mut items, 0x2545_F491_4F6C_DD1D);
        assert_eq!(items, vec![1, 0, 3, 2, 6, 5, 4, 7]);
    }

    /// A second seed, so the test pins the PRNG and not one lucky constant.
    #[test]
    fn xorshift_shuffle_is_seed_driven() {
        let mut items: Vec<u32> = (0..5).collect();
        xorshift_shuffle_seeded(&mut items, 12345);
        assert_eq!(items, vec![3, 0, 2, 1, 4]);
    }

    /// The reference's loop never runs for 0 or 1 elements — a one-track label
    /// Shuffle must not panic on the `1..len` range.
    #[test]
    fn xorshift_shuffle_is_a_no_op_below_two_elements() {
        let mut one = vec![7u32];
        xorshift_shuffle_seeded(&mut one, 42);
        assert_eq!(one, vec![7]);
        let mut none: Vec<u32> = vec![];
        xorshift_shuffle_seeded(&mut none, 42);
        assert!(none.is_empty());
    }

    /// The seed is OR-ed with 1 the way the reference does, so a zero clock
    /// reading cannot park xorshift64 on its fixed point (which would leave
    /// the list in its original order — a "shuffle" that never shuffles).
    #[test]
    fn a_zero_seed_still_shuffles() {
        let mut items: Vec<u32> = (0..8).collect();
        xorshift_shuffle_seeded(&mut items, 0);
        assert_ne!(items, (0..8).collect::<Vec<u32>>());
    }

    /// Whatever the seed, shuffling is a permutation: nothing is lost and
    /// nothing is duplicated (the queue the user gets is the queue they asked
    /// for, reordered).
    #[test]
    fn xorshift_shuffle_is_a_permutation() {
        for seed in [1u64, 2, 3, 999, u64::MAX] {
            let mut items: Vec<u32> = (0..64).collect();
            xorshift_shuffle_seeded(&mut items, seed);
            items.sort_unstable();
            assert_eq!(items, (0..64).collect::<Vec<u32>>());
        }
    }

    // --- PARITY-DEBT #7: the stream-error toast text ----------------------
    //
    // Tests run with the default (English) catalog, so `qbz_i18n::t` returns
    // the msgid — which is exactly the string the reference builds.

    /// Outside a sandbox the raw engine error is shown, prefixed.
    #[test]
    fn stream_error_outside_a_sandbox_keeps_the_raw_message() {
        assert_eq!(
            stream_error_text("ALSA: device or resource busy", false),
            "Audio output error: ALSA: device or resource busy"
        );
    }

    /// Inside a sandbox an ALSA-shaped failure is rewritten into the
    /// actionable sentence (`playback.rs:5105-5111`).
    #[test]
    fn stream_error_inside_a_sandbox_rewrites_alsa_failures() {
        let expected = "Direct ALSA device access is blocked inside Flatpak/Snap — switch the audio backend to PipeWire or System Default";
        assert_eq!(stream_error_text("ALSA open failed", true), expected);
        // The match is case-insensitive and also fires on the device string.
        assert_eq!(stream_error_text("cannot open hw:1,0", true), expected);
        assert_eq!(stream_error_text("alsa: no such device", true), expected);
    }

    /// A sandboxed failure that is NOT ALSA-shaped keeps the raw message —
    /// the rewrite must not swallow an unrelated cause.
    #[test]
    fn stream_error_inside_a_sandbox_keeps_non_alsa_failures() {
        assert_eq!(
            stream_error_text("HTTP 403 from the CDN", true),
            "Audio output error: HTTP 403 from the CDN"
        );
    }

    /// Drop the odd numbers; `start` must follow the CLICKED item, not its old
    /// index. Without the remap the core would start on a different track than
    /// the one the user clicked — silently, and only when something was blocked.
    #[test]
    fn start_index_follows_the_clicked_track() {
        let (kept, start) = filter_queue_with(vec![0, 1, 2, 3, 4], Some(2), |n| n % 2 == 1);
        assert_eq!(kept, vec![0, 2, 4]);
        assert_eq!(start, Some(1)); // the 2 moved from index 2 to index 1
    }

    /// Clicking a row that is itself blocked lands on the next survivor.
    #[test]
    fn a_blocked_clicked_row_falls_forward() {
        let (kept, start) = filter_queue_with(vec![0, 1, 2, 3], Some(1), |n| n % 2 == 1);
        assert_eq!(kept, vec![0, 2]);
        assert_eq!(start, Some(1)); // the 1 was dropped, so start on the 2
    }

    /// The whole tail blocked: start on the LAST survivor, never past the end
    /// (an out-of-range index is what makes the core start nothing at all).
    #[test]
    fn a_fully_blocked_tail_clamps_to_the_last_survivor() {
        let (kept, start) = filter_queue_with(vec![0, 1, 3, 5], Some(2), |n| n % 2 == 1);
        assert_eq!(kept, vec![0]);
        assert_eq!(start, Some(0));
    }

    /// Everything blocked: an empty queue carries NO start index.
    #[test]
    fn everything_blocked_yields_no_start() {
        let (kept, start) = filter_queue_with(vec![1, 3, 5], Some(1), |n| n % 2 == 1);
        assert!(kept.is_empty());
        assert_eq!(start, None);
    }

    /// Nothing blocked: the queue and the index are untouched — the path every
    /// user without a blacklist takes.
    #[test]
    fn nothing_blocked_is_identity() {
        let (kept, start) = filter_queue_with(vec![0, 2, 4], Some(2), |_| false);
        assert_eq!(kept, vec![0, 2, 4]);
        assert_eq!(start, Some(2));
    }
}
