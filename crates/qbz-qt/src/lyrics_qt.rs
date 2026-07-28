//! Lyrics side panel data layer — drives the `qbz-lyrics` crate
//! (LyricsService: Qobuz primary + LRCLIB/OVH fallback chain + per-user
//! SQLite cache, all self-contained) from the playback track-change hook.
//!
//! Lifecycle (mirrors `lyrics::init_for_user`): the service is built
//! LAZILY on first use from the core's QobuzClient clone, and the per-user
//! cache (`~/.cache/qbz/users/<uid>/lyrics`) binds on every session
//! activation. Fetches dedupe in-flight per track inside the crate.
//!
//! POC-NOTEs:
//! - Word-level karaoke fill (the wsync word engine), translations
//!   (Qobuz v10 language targets + the translate toggle), the display
//!   settings flyout (size/dimming/uppercase/font/color prefs — the
//!   Slint defaults are hardcoded), MusicBrainz/AI providers: out of scope.
//! - Highlight cadence is the 1 Hz playback poll (vs the Slint engine's
//!   ~30 Hz) — line-level highlight only.

use std::sync::{Arc, OnceLock};

use qbz_app::shell::AppRuntime;
use qbz_app::user_data::UserDataPaths;
use qbz_core::LoggingAdapter;
use qbz_lyrics::service::{LyricsOutcome, LyricsRequest, LyricsService};
use qbz_models::QueueTrack;
use cxx_qt_lib::QString;
use serde::Serialize;

/// LyricsState.status semantics (LyricsSidebar renders by these).
#[derive(Clone, Serialize)]
pub struct LyricsLine {
    #[serde(rename = "timeMs")]
    pub time_ms: Option<i64>,
    #[serde(rename = "endMs")]
    pub end_ms: Option<i64>,
    pub text: String,
}

#[derive(Default, Serialize)]
pub struct LyricsDoc {
    /// 0 idle / 1 loading / 2 ready / 3 not-found / 4 error / 5 offline-miss.
    pub status: i32,
    pub lines: Vec<LyricsLine>,
    pub synced: bool,
    pub provider: String,
    pub error: String,
}

static SERVICE: OnceLock<Arc<LyricsService>> = OnceLock::new();
/// The user id the cache is currently bound for ("" = unbound).
static CACHE_USER: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Build (once) the service from the core's client clone, and bind the
/// per-user cache on first use / user change.
async fn service(runtime: &Arc<AppRuntime<LoggingAdapter>>) -> Option<Arc<LyricsService>> {
    let service = match SERVICE.get() {
        Some(s) => Arc::clone(s),
        None => {
            let client_lock = runtime.core().client();
            let client = {
                let guard = client_lock.read().await;
                guard.as_ref().cloned()
            }?;
            let service = Arc::new(LyricsService::with_qobuz_client(Arc::new(client)));
            let _ = SERVICE.set(Arc::clone(&service));
            service
        }
    };
    // Bind the per-user cache (idempotent per user). The std MutexGuard
    // must NOT be held across the await (it would make the future !Send).
    let needs_init = UserDataPaths::load_last_user_id()
        .map(|uid| {
            let bound = CACHE_USER.lock().unwrap();
            *bound != uid.to_string()
        })
        .unwrap_or(false);
    if needs_init {
        if let Some(uid) = UserDataPaths::load_last_user_id() {
            if let Some(dir) = dirs::cache_dir().map(|d| d.join("qbz").join("users").join(uid.to_string())) {
                match service.init_at(&dir).await {
                    Ok(()) => {
                        log::info!("[qbz-qt] lyrics cache bound for user {uid}");
                        *CACHE_USER.lock().unwrap() = uid.to_string();
                    }
                    Err(e) => log::error!("[qbz-qt] lyrics cache init failed: {e}"),
                }
            }
        }
    }
    Some(service)
}

fn publish(doc: LyricsDoc) {
    let json = serde_json::to_string(&doc).unwrap_or_else(|_| "{}".into());
    crate::ui(move |mut b| {
        b.as_mut().set_lyrics_json(QString::from(json.as_str()));
    });
}

/// Publish the loading state (status 1) — called on every track change.
pub fn publish_loading() {
    publish(LyricsDoc {
        status: 1,
        ..Default::default()
    });
}

/// Publish the idle/empty state (track cleared).
pub fn publish_idle() {
    publish(LyricsDoc::default());
}

/// Fetch lyrics for the new current track (called from
/// `refresh_now_playing` on every track change; the panel re-requests on
/// open via the same path — a cached doc returns immediately).
pub async fn load_for_track(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    track: &QueueTrack,
) {
    publish_loading();
    let Some(service) = service(runtime).await else {
        publish(LyricsDoc {
            status: 4,
            error: "lyrics service unavailable".to_string(),
            ..Default::default()
        });
        return;
    };
    let request = LyricsRequest {
        track_id: Some(track.id),
        source: qbz_lyrics::service::LyricsSourceKind::Qobuz,
        title: track.title.clone(),
        artist: track.artist.clone(),
        album: Some(track.album.clone()),
        duration_secs: Some(track.duration_secs),
        offline: crate::offline_fwd::engine().is_offline(),
        language: None,
    };
    let track_id = track.id;
    match service.get(request).await {
        Ok(response) => {
            // Stale-response guard (F2 parity): a slow fetch for a track
            // that is no longer current is discarded by the caller's own
            // generation check — see playback_qt (it awaits sequentially
            // per track change, so this mostly covers rapid skips).
            let _ = track_id;
            match response.outcome {
                LyricsOutcome::Found(result) => {
                    let doc = result.doc;
                    let n = doc.lines.len();
                    let provider = doc.provider.as_str().to_string();
                    let synced = doc.synced;
                    let lines: Vec<LyricsLine> = doc
                        .lines
                        .iter()
                        .map(|l| LyricsLine {
                            time_ms: l.time_ms,
                            end_ms: l.end_ms,
                            text: l.text.clone(),
                        })
                        .collect();
                    publish(LyricsDoc {
                        status: if lines.is_empty() { 3 } else { 2 },
                        synced,
                        provider: provider.clone(),
                        lines,
                        ..Default::default()
                    });
                    log::info!("[qbz-qt] lyrics: {n} lines ({provider}, synced={synced}) for track {track_id}");
                }
                LyricsOutcome::NotFound => publish(LyricsDoc {
                    status: 3,
                    ..Default::default()
                }),
                LyricsOutcome::NotAvailableOffline => publish(LyricsDoc {
                    status: 5,
                    ..Default::default()
                }),
            }
        }
        Err(e) => publish(LyricsDoc {
            status: 4,
            error: e,
            ..Default::default()
        }),
    }
}
