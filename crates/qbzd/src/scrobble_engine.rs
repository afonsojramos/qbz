// crates/qbzd/src/scrobble_engine.rs — scrobble-on-play (CONSOLE ext).
//
// A daemon background task subscribing the DaemonAdapter CoreEvent bus. On
// `TrackStarted` it sends "now playing" to every ACTIVE provider; when the
// track crosses the scrobble threshold (`qbz_app::scrobble_timing::
// scrobble_delay_secs` — Last.fm's played-half-or-4-min rule) it scrobbles
// ONCE. Credentials are re-read from the canonical `ScrobblerSettingsStore` on
// each track start, so `qbzd scrobble …` changes take effect on the next track
// with no reload signal. Best-effort + logged.
//
// Providers: Last.fm (LastFmClient::update_now_playing / scrobble) and
// ListenBrainz (submit_playing_now / submit_listen). Both backends are
// qbz-integrations (Slint-free). A persistent ListenBrainz offline queue
// (ListenBrainzCache) is a follow-up; this slice submits best-effort.
use std::time::{SystemTime, UNIX_EPOCH};

use qbz_app::settings::scrobblers::{ScrobblerSettings, ScrobblerSettingsStore};
use qbz_integrations::lastfm::LastFmClient;
use qbz_integrations::listenbrainz::{ListenBrainzClient, ListenBrainzConfig};
use qbz_models::{CoreEvent, QueueTrack};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;

use crate::paths::ProfileRoots;

/// The track currently being timed for a scrobble.
struct Playing {
    track: QueueTrack,
    /// Unix seconds when it started — Last.fm's scrobble timestamp.
    started_at: u64,
    /// Seconds into the track at which it becomes scrobble-eligible; `None`
    /// means "too short to scrobble" (`scrobble_delay_secs` returned None).
    threshold: Option<u64>,
    scrobbled: bool,
}

/// Spawn the scrobble-on-play task. Holds NO `Arc<AppRuntime>` (only the roots,
/// its own store, and the bus receiver), so it is outside the §8.2 audio
/// clock-release ordering — the caller aborts it for a clean shutdown.
pub fn spawn(roots: ProfileRoots, mut rx: broadcast::Receiver<CoreEvent>) -> JoinHandle<()> {
    use broadcast::error::RecvError;
    tokio::spawn(async move {
        let store = match ScrobblerSettingsStore::new_at(&roots.data) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("[scrobbler] store open failed; scrobbling disabled: {e}");
                return;
            }
        };
        let mut playing: Option<Playing> = None;

        loop {
            match rx.recv().await {
                Ok(CoreEvent::TrackStarted { track, .. }) => {
                    let settings = store.get_settings().unwrap_or_default();
                    if !settings.enabled {
                        playing = None;
                        continue;
                    }
                    now_playing(&settings, &track).await;
                    playing = Some(Playing {
                        threshold: qbz_app::scrobble_timing::scrobble_delay_secs(track.duration_secs),
                        started_at: now_unix(),
                        track,
                        scrobbled: false,
                    });
                }
                Ok(CoreEvent::PositionUpdated { position_secs, .. }) => {
                    if let Some(p) = playing.as_mut() {
                        if due(position_secs, p.threshold, p.scrobbled) {
                            let settings = store.get_settings().unwrap_or_default();
                            scrobble(&settings, &p.track, p.started_at).await;
                            p.scrobbled = true;
                        }
                    }
                }
                Ok(_) => {}
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return,
            }
        }
    })
}

// ============================ internals ============================

/// Whether the current track is due to scrobble now: it has a threshold, has
/// been played to it, and hasn't been scrobbled yet. Pure — unit-tested.
fn due(position_secs: u64, threshold: Option<u64>, scrobbled: bool) -> bool {
    !scrobbled && threshold.is_some_and(|t| position_secs >= t)
}

async fn now_playing(s: &ScrobblerSettings, t: &QueueTrack) {
    let album = album_opt(t);
    if s.lastfm_active() {
        let c = LastFmClient::with_session_key(s.lastfm_session_key.clone());
        if let Err(e) = c.update_now_playing(&t.artist, &t.title, album).await {
            log::debug!("[scrobbler] last.fm now-playing failed: {e}");
        }
    }
    if s.listenbrainz_active() {
        let c = lb_client(s);
        if let Err(e) = c.submit_playing_now(&t.artist, &t.title, album, None).await {
            log::debug!("[scrobbler] listenbrainz now-playing failed: {e}");
        }
    }
}

async fn scrobble(s: &ScrobblerSettings, t: &QueueTrack, started_at: u64) {
    let album = album_opt(t);
    if s.lastfm_active() {
        let c = LastFmClient::with_session_key(s.lastfm_session_key.clone());
        match c.scrobble(&t.artist, &t.title, album, started_at).await {
            Ok(()) => log::info!("[scrobbler] last.fm scrobbled: {} — {}", t.artist, t.title),
            Err(e) => log::warn!("[scrobbler] last.fm scrobble failed: {e}"),
        }
    }
    if s.listenbrainz_active() {
        let c = lb_client(s);
        match c.submit_listen(&t.artist, &t.title, album, started_at as i64, None).await {
            Ok(()) => log::info!("[scrobbler] listenbrainz submitted: {} — {}", t.artist, t.title),
            Err(e) => log::warn!("[scrobbler] listenbrainz submit failed: {e}"),
        }
    }
}

/// A ListenBrainz client bound to the stored token, with its own enabled flag
/// ON (submit_* early-returns if the client config is disabled — our gate is
/// `ScrobblerSettings::listenbrainz_active`, checked before calling).
fn lb_client(s: &ScrobblerSettings) -> ListenBrainzClient {
    ListenBrainzClient::with_config(ListenBrainzConfig {
        enabled: true,
        token: Some(s.listenbrainz_token.clone()),
        user_name: Some(s.listenbrainz_username.clone()),
    })
}

/// The album name, unless it's empty or the queue-track "Unknown Album"
/// placeholder (both scrobble better as "no album" than a fake one).
fn album_opt(t: &QueueTrack) -> Option<&str> {
    if t.album.is_empty() || t.album == "Unknown Album" {
        None
    } else {
        Some(&t.album)
    }
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn due_only_when_past_threshold_and_not_yet_scrobbled() {
        assert!(!due(10, Some(120), false)); // not there yet
        assert!(due(120, Some(120), false)); // exactly at threshold
        assert!(due(200, Some(120), false)); // past it
        assert!(!due(200, Some(120), true)); // already scrobbled
        assert!(!due(999, None, false)); // too short to scrobble (no threshold)
    }

    fn qt(album: &str) -> QueueTrack {
        QueueTrack {
            id: 1,
            title: "Spain".into(),
            version: None,
            artist: "Chick Corea".into(),
            album: album.into(),
            album_version: None,
            duration_secs: 300,
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
            context_kind: None,
            context_id: None,
        }
    }

    #[test]
    fn album_opt_drops_empty_and_unknown() {
        assert_eq!(album_opt(&qt("Light as a Feather")), Some("Light as a Feather"));
        assert_eq!(album_opt(&qt("")), None);
        assert_eq!(album_opt(&qt("Unknown Album")), None);
    }
}
