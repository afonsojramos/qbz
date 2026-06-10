//! Slint-side glue for the shared lyrics engine (`crates/qbz-lyrics`).
//!
//! The engine (Qobuz-first chain, providers, per-user SQLite cache) is
//! frontend-agnostic (ADR-006); this module owns only the process global and
//! the bindings, following the `offline_mode.rs` / `fav_cache.rs` template:
//!
//! - [`init_for_user`] binds the per-user cache on every session activation
//!   (login, restore, offline entry — next to `offline_mode::init_for_user`);
//!   [`teardown`] drops it on logout. The cache file is the SAME
//!   `<user cache dir>/lyrics/lyrics.db` Tauri uses.
//! - [`on_track_changed`] is the fetch rider on the `NOTIFY_LAST_TRACK`
//!   de-duped track-change edge in `playback::refresh_now_playing_meta`
//!   (the scrobble/notification seam). Tauri prefetches on EVERY track change
//!   regardless of panel visibility (`lyricsStore.ts:545-565` — "Always
//!   prefetch lyrics when a new track starts"); same here. Deliberately NO
//!   skip-if-remote: lyrics follow the QConnect peer's track (Q7).
//! - [`on_track_cleared`] resets `LyricsState` when the queue empties
//!   (track -> null resets the store, `lyricsStore.ts:560-562`).
//! - The sidebar's `init` fires `LyricsState.panel-opened()` (conditional
//!   mount, ADR-010) -> main.rs re-runs the same fetch path for the current
//!   track; the duplicate-fetch guard makes that a no-op while still loaded
//!   (Tauri `lastFetchedTrackId` guard, `lyricsStore.ts:352-354`).
//!
//! Stale-response guard (F2): every spawned fetch captures its request
//! identity; the service echoes it back (`request_track_id`/`request_key`)
//! and the response is DROPPED unless it still matches the latest requested
//! track — a late response can never overwrite the current track's lyrics
//! (the documented Tauri race, review §1.6).
//!
//! The parsed [`LyricsDoc`] (native Qobuz word stamps included) is held
//! Rust-side in [`CURRENT_DOC`]; the UI model only carries the line list +
//! a `has-words` flag. The S4 sync engine consumes the doc for the
//! word-anchored karaoke fill.

use std::sync::{Arc, Mutex, OnceLock};

use slint::{ComponentHandle, ModelRc, VecModel};

use qbz_lyrics::{
    build_cache_key, LyricsData, LyricsDoc, LyricsOutcome, LyricsProvider, LyricsProviders,
    LyricsRequest, LyricsResponse, LyricsService, LyricsSourceKind,
};
use qbz_models::QueueTrack;
use qbz_qobuz::{QobuzClient, QobuzLyricsDocument};

use crate::{AppWindow, LyricsLineItem, LyricsState};

// `LyricsState.status` values (keep in sync with `ui/state.slint`).
// `READY` is pub(crate): the sync engine (`lyrics_sync`) gates on it.
const STATUS_IDLE: i32 = 0;
const STATUS_LOADING: i32 = 1;
pub(crate) const STATUS_READY: i32 = 2;
const STATUS_NOT_FOUND: i32 = 3;
const STATUS_ERROR: i32 = 4;
const STATUS_OFFLINE: i32 = 5;

/// Process-global lyrics service. Installed on the first session activation;
/// the per-user cache handle re-binds via `init_at` on every activation.
static SERVICE: OnceLock<Arc<LyricsService>> = OnceLock::new();

/// Identity of the latest requested track + whether a Found result has been
/// committed for it. `key` doubles as the stale guard (F2) and the
/// duplicate-fetch guard (`loaded` mirrors Tauri's `status === 'loaded'`).
struct CurrentLyrics {
    key: String,
    loaded: bool,
}

static CURRENT: Mutex<CurrentLyrics> = Mutex::new(CurrentLyrics {
    key: String::new(),
    loaded: false,
});

/// The parsed document for the current track, held Rust-side so the native
/// Qobuz word timestamps survive for the S4 sync engine (the UI model only
/// carries text + line bounds + a has-words flag).
static CURRENT_DOC: Mutex<Option<LyricsDoc>> = Mutex::new(None);

/// Read access to the current parsed document for the sync engine: the
/// closure runs under the lock (keep it short — it executes on the UI
/// thread at the engine's tick rate). A poisoned lock degrades to `None`.
pub(crate) fn with_current_doc<R>(f: impl FnOnce(Option<&LyricsDoc>) -> R) -> R {
    match CURRENT_DOC.lock() {
        Ok(guard) => f(guard.as_ref()),
        Err(_) => f(None),
    }
}

/// Production providers over the shared core client lock. The lock is read
/// at CALL time (never cached), so re-inits of the Qobuz client are always
/// picked up; a missing client (pre-login, offline boot) errors out, which
/// the chain treats as silent degradation to the external fallbacks.
struct SharedClientProviders {
    client: Arc<tokio::sync::RwLock<Option<QobuzClient>>>,
}

#[async_trait::async_trait]
impl LyricsProviders for SharedClientProviders {
    async fn qobuz(&self, track_id: u64) -> Result<Option<QobuzLyricsDocument>, String> {
        let guard = self.client.read().await;
        let client = guard
            .as_ref()
            .ok_or_else(|| "Qobuz client not initialized".to_string())?;
        client.get_lyrics(track_id).await.map_err(|e| e.to_string())
    }

    async fn lrclib(
        &self,
        title: &str,
        artist: &str,
        duration_secs: Option<u64>,
    ) -> Result<Option<LyricsData>, String> {
        qbz_lyrics::providers::fetch_lrclib(title, artist, duration_secs).await
    }

    async fn ovh(&self, title: &str, artist: &str) -> Option<LyricsData> {
        qbz_lyrics::providers::fetch_lyrics_ovh(title, artist).await
    }
}

/// Bind the per-user lyrics cache — the SAME `lyrics/lyrics.db` under the
/// per-user CACHE dir that Tauri's `session_lifecycle.rs:229` uses, so both
/// frontends share one cache. Called on every session activation; the first
/// call installs the process-global service over the core's client lock
/// (one lock per process, so later calls reuse the installed providers).
/// Best-effort: failures are logged, never block entry. Must run within the
/// tokio runtime context (the bind is spawned).
pub fn init_for_user(client: Arc<tokio::sync::RwLock<Option<QobuzClient>>>, user_id: u64) {
    let service = SERVICE
        .get_or_init(move || {
            Arc::new(LyricsService::new(Arc::new(SharedClientProviders {
                client,
            })))
        })
        .clone();
    let Some(cache_dir) = dirs::cache_dir().map(|d| {
        d.join("qbz")
            .join("users")
            .join(user_id.to_string())
    }) else {
        log::error!("[qbz-slint] lyrics cache init: cache directory unavailable");
        return;
    };
    tokio::spawn(async move {
        match service.init_at(&cache_dir).await {
            Ok(()) => log::info!("[qbz-slint] lyrics cache bound for user {user_id}"),
            Err(e) => log::error!("[qbz-slint] lyrics cache init failed: {e}"),
        }
    });
}

/// Drop the per-user cache handle + the in-memory track state on logout.
/// Clearing `CURRENT` also invalidates any in-flight fetch (its stale guard
/// no longer matches), so a late response from the previous session never
/// lands in the next one.
pub fn teardown() {
    if let Ok(mut current) = CURRENT.lock() {
        current.key.clear();
        current.loaded = false;
    }
    if let Ok(mut doc) = CURRENT_DOC.lock() {
        *doc = None;
    }
    if let Some(service) = SERVICE.get().cloned() {
        tokio::spawn(async move {
            service.teardown().await;
        });
    }
}

/// Qobuz vs non-Qobuz from the queue track's source tag. `qobuz_download`
/// rows carry the REAL Qobuz catalog id (`local_queue_track`), so the Qobuz
/// primary applies to them too; local user files / ephemeral folders / Plex
/// have synthetic ids and go straight to the metadata-keyed fallback chain.
fn source_kind(track: &QueueTrack) -> LyricsSourceKind {
    match track.source.as_deref() {
        Some("local") | Some("ephemeral") | Some("plex") => LyricsSourceKind::NonQobuz,
        // None | "qobuz" | "qobuz_download"
        _ => LyricsSourceKind::Qobuz,
    }
}

/// One identity string for a request/response pair: the track id when the
/// Qobuz primary applies, else the metadata cache key. Matches the echo the
/// service returns (`request_track_id` / `request_key`).
fn request_identity(track_id: Option<u64>, cache_key: &str) -> String {
    match track_id {
        Some(id) => format!("id:{id}"),
        None => format!("key:{cache_key}"),
    }
}

fn provider_label(provider: LyricsProvider) -> &'static str {
    match provider {
        LyricsProvider::Lrclib => "LRCLIB",
        LyricsProvider::Ovh => "lyrics.ovh",
        // First-party — no attribution needed (spec §3.5).
        LyricsProvider::Qobuz => "",
    }
}

/// The fetch rider — called inside the `NOTIFY_LAST_TRACK` guard of
/// `refresh_now_playing_meta` on every real track-change edge, and from the
/// panel-open path for the current track. Fire-and-forget: pushes the
/// loading state immediately, resolves through the engine off-loop, and
/// commits the response only if the track is still current (F2).
pub fn on_track_changed(weak: slint::Weak<AppWindow>, track: &QueueTrack) {
    let Some(service) = SERVICE.get().cloned() else {
        log::debug!("[qbz-slint] lyrics fetch skipped: service not installed");
        return;
    };

    let source = source_kind(track);
    // F4 contract, explicit: the RAW title goes to the engine (fallback
    // providers match on the unversioned title; Qobuz looks up by id). The
    // header meta shows the version-enriched display title like the bar.
    let display_title = match track.version.as_deref().filter(|v| !v.is_empty()) {
        Some(version) => format!("{} ({version})", track.title),
        None => track.title.clone(),
    };
    let artist = track.artist.clone();
    let request = LyricsRequest {
        track_id: (source == LyricsSourceKind::Qobuz).then_some(track.id),
        source,
        title: track.title.clone(),
        artist: artist.clone(),
        album: (!track.album.is_empty()).then(|| track.album.clone()),
        duration_secs: (track.duration_secs > 0).then_some(track.duration_secs),
        // Offline as data, not lookup (spec §2.2.4): the engine verdict is
        // read here and travels with the request.
        offline: crate::offline_mode::engine().is_offline(),
    };
    let key = request_identity(
        request.track_id,
        &build_cache_key(
            request.title.trim(),
            request.artist.trim(),
            request.duration_secs,
        ),
    );

    // Duplicate-fetch guard (Tauri parity, lyricsStore.ts:352-354): skip only
    // when the SAME track is already loaded; not-found/error states re-fetch
    // on the next trigger (e.g. panel re-open).
    {
        let mut current = CURRENT.lock().expect("lyrics CURRENT lock poisoned");
        if current.key == key && current.loaded {
            return;
        }
        current.key = key.clone();
        current.loaded = false;
    }

    // Loading state + header meta, immediately (the spinner shows even for a
    // fast cache hit — Tauri does the same).
    {
        let display_title = display_title.clone();
        let artist = artist.clone();
        let _ = weak.clone().upgrade_in_event_loop(move |w| {
            let state = w.global::<LyricsState>();
            state.set_status(STATUS_LOADING);
            state.set_track_title(display_title.into());
            state.set_track_artist(artist.into());
            state.set_lines(ModelRc::new(VecModel::default()));
            state.set_synced(false);
            state.set_active_index(-1);
            state.set_line_progress(0.0);
            state.set_fill_anim_ms(0);
            state.set_provider("".into());
            state.set_provider_label("".into());
            state.set_error("".into());
        });
    }

    tokio::spawn(async move {
        let result = service.get(request).await;
        // Stale guard (F2): match the response echo against the LATEST
        // requested identity; a superseded response is dropped whole.
        let response_key = match &result {
            Ok(response) => {
                request_identity(response.request_track_id, &response.request_key)
            }
            Err(_) => key.clone(),
        };
        {
            let mut current = CURRENT.lock().expect("lyrics CURRENT lock poisoned");
            if current.key != response_key {
                return;
            }
            if matches!(
                result.as_ref().map(|r| &r.outcome),
                Ok(LyricsOutcome::Found(_))
            ) {
                current.loaded = true;
            }
        }
        apply_result(weak, result);
    });
}

/// Reset everything when the queue empties (track -> null), mirroring the
/// Tauri store reset (`lyricsStore.ts:560-562`).
pub fn on_track_cleared(weak: slint::Weak<AppWindow>) {
    if let Ok(mut current) = CURRENT.lock() {
        current.key.clear();
        current.loaded = false;
    }
    if let Ok(mut doc) = CURRENT_DOC.lock() {
        *doc = None;
    }
    let _ = weak.upgrade_in_event_loop(|w| {
        let state = w.global::<LyricsState>();
        state.set_status(STATUS_IDLE);
        state.set_lines(ModelRc::new(VecModel::default()));
        state.set_synced(false);
        state.set_active_index(-1);
        state.set_line_progress(0.0);
        state.set_fill_anim_ms(0);
        state.set_track_title("".into());
        state.set_track_artist("".into());
        state.set_provider("".into());
        state.set_provider_label("".into());
        state.set_error("".into());
    });
}

/// Map the engine response into `LyricsState` (UI thread push).
fn apply_result(weak: slint::Weak<AppWindow>, result: Result<LyricsResponse, String>) {
    let (status, items, synced, provider, label, error) = match result {
        Ok(response) => match response.outcome {
            LyricsOutcome::Found(found) => {
                let items: Vec<LyricsLineItem> = found
                    .doc
                    .lines
                    .iter()
                    .map(|line| LyricsLineItem {
                        text: line.text.clone().into(),
                        time_ms: line.time_ms.map(|v| v as i32).unwrap_or(-1),
                        end_ms: line.end_ms.map(|v| v as i32).unwrap_or(-1),
                        has_words: line.words.is_some(),
                    })
                    .collect();
                let synced = found.doc.synced;
                let provider = found.doc.provider;
                if let Ok(mut doc) = CURRENT_DOC.lock() {
                    *doc = Some(found.doc);
                }
                (
                    STATUS_READY,
                    items,
                    synced,
                    provider.as_str(),
                    provider_label(provider),
                    String::new(),
                )
            }
            LyricsOutcome::NotFound => {
                if let Ok(mut doc) = CURRENT_DOC.lock() {
                    *doc = None;
                }
                (STATUS_NOT_FOUND, Vec::new(), false, "", "", String::new())
            }
            // Typed offline miss (F3) — the view maps it to a translated
            // string, never a hardcoded message.
            LyricsOutcome::NotAvailableOffline => {
                if let Ok(mut doc) = CURRENT_DOC.lock() {
                    *doc = None;
                }
                (STATUS_OFFLINE, Vec::new(), false, "", "", String::new())
            }
        },
        Err(e) => {
            if let Ok(mut doc) = CURRENT_DOC.lock() {
                *doc = None;
            }
            log::warn!("[qbz-slint] lyrics fetch failed: {e}");
            (STATUS_ERROR, Vec::new(), false, "", "", e)
        }
    };
    let _ = weak.upgrade_in_event_loop(move |w| {
        let state = w.global::<LyricsState>();
        state.set_lines(ModelRc::new(VecModel::from(items)));
        state.set_synced(synced);
        state.set_provider(provider.into());
        state.set_provider_label(label.into());
        state.set_error(error.into());
        state.set_active_index(-1);
        state.set_line_progress(0.0);
        state.set_fill_anim_ms(0);
        state.set_status(status);
        // One immediate engine pass so a freshly committed doc lands on the
        // correct line right away — even while PAUSED (Tauri computes once on
        // load, lyricsStore.ts:386-389); continuous ticking stays gated on
        // playing.
        crate::lyrics_sync::kick();
    });
}
