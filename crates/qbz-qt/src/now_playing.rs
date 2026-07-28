//! Now-playing model — the single data source behind the bar's `np*` bridge
//! properties (Slint `NowPlayingState`).
//!
//! Holds the track meta + transport, the QUALITY STAMP state (catalog tier /
//! detail, the delivered-vs-catalog downgrade block) and the cast/remote
//! flags. Every field is published through ONE `publish()` — a second
//! near-copy of it was the reason the cache overlay and the quality fields
//! could disagree, so there is exactly one now.
//!
//! The arithmetic behind the downgrade block lives in `quality_state.rs`
//! (pure, unit-tested); the two output LEDs are derived from AudioSettings in
//! `output_labels.rs`. They follow the SETTINGS, not the track — but they are
//! DECIDED when a stream opens, so besides `settings_qt::publish_snapshot`
//! they are re-derived on shell entry (`publish_current`), on the track edge
//! (`playback_qt::refresh_now_playing`) and on the stream edge
//! (`set_effective_stream`, only when the delivered params actually move).
//! Without those edges the stamp only refreshed when the user changed page.
//!
//! POC-NOTE: playback transport is wired (playback_qt poll pump). Cast /
//! Qobuz Connect are NOT in the POC build (no qbz-cast / qconnect deps), so
//! `set_cast_session` / `set_remote` exist as the wiring seam and the flags
//! stay at their idle defaults until a session service calls them.

use std::sync::Mutex;

use cxx_qt_lib::QString;

use crate::quality_state::Delivered;

#[derive(Clone, Default)]
pub struct NowPlayingModel {
    pub has_track: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_id: String,
    pub artist_id: String,
    pub artwork_path: String,
    pub elapsed_secs: i32,
    pub duration_secs: i32,
    pub playing: bool,
    pub loading: bool,
    /// Streaming buffer fill 0..1 (seekbar cache overlay); 0 = not streaming.
    pub cache: f32,
    pub volume: f32,
    pub muted: bool,
    pub shuffle: bool,
    /// 0 off / 1 all / 2 one.
    pub repeat_mode: i32,
    /// CATALOG tier: "hires" | "mp3" | "lossless" | "cd" | "".
    pub quality_tier: String,
    /// CATALOG detail, e.g. "24-bit / 96 kHz".
    pub quality_detail: String,
    /// Delivered-vs-catalog block (downgrade arrow + tooltip cause).
    pub delivered: Delivered,
    /// Last delivered stream params seen from the engine, so a poll tick that
    /// changes nothing costs no Qt-thread hop.
    pub eff_rate_hz: u32,
    pub eff_bits: u32,
    /// A peer Qobuz Connect renderer owns playback.
    pub is_remote: bool,
    /// Active renderer / cast target name; empty when local.
    pub cast_target: String,
    /// A Chromecast/DLNA session is connected.
    pub cast_active: bool,
    /// "cast" | "dlna".
    pub cast_protocol: String,
}

/// The idle bar: no track, full volume, no badge.
fn idle() -> NowPlayingModel {
    NowPlayingModel {
        volume: 1.0,
        ..NowPlayingModel::default()
    }
}

static MODEL: Mutex<Option<NowPlayingModel>> = Mutex::new(None);

fn with_model<T>(f: impl FnOnce(&mut NowPlayingModel) -> T) -> (T, NowPlayingModel) {
    let mut guard = MODEL.lock().unwrap();
    let m = guard.get_or_insert_with(idle);
    let out = f(m);
    (out, m.clone())
}

fn mutate(f: impl FnOnce(&mut NowPlayingModel)) {
    let (_, snapshot) = with_model(f);
    publish(&snapshot);
}

/// Push the full model onto the bridge (Qt thread hop). The ONLY publisher —
/// every setter funnels through here so no field can go stale behind another.
fn publish(m: &NowPlayingModel) {
    // Mirror the transport onto the visualizer tap: a paused player parks the
    // FFT producer instead of re-analyzing a stale ring buffer (the Slint
    // playback.rs does the same). Cheap atomics, safe off the Qt thread.
    crate::viz_qt::set_paused(!m.playing);
    let m = m.clone();
    crate::player_bridge::ui(move |mut b| {
        let progress = if m.duration_secs > 0 {
            (m.elapsed_secs as f32 / m.duration_secs as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        b.as_mut().set_np_has_track(m.has_track);
        b.as_mut().set_np_title(QString::from(m.title.as_str()));
        b.as_mut().set_np_artist(QString::from(m.artist.as_str()));
        b.as_mut().set_np_album(QString::from(m.album.as_str()));
        b.as_mut()
            .set_np_album_id(QString::from(m.album_id.as_str()));
        b.as_mut()
            .set_np_artist_id(QString::from(m.artist_id.as_str()));
        b.as_mut()
            .set_np_artwork_path(QString::from(m.artwork_path.as_str()));
        b.as_mut().set_np_elapsed_secs(m.elapsed_secs);
        b.as_mut().set_np_duration_secs(m.duration_secs);
        b.as_mut().set_np_progress(progress);
        b.as_mut().set_np_cache_progress(m.cache);
        b.as_mut().set_np_playing(m.playing);
        b.as_mut().set_np_loading(m.loading);
        b.as_mut().set_np_volume(m.volume);
        b.as_mut().set_np_muted(m.muted);
        b.as_mut().set_np_shuffle(m.shuffle);
        b.as_mut().set_np_repeat_mode(m.repeat_mode);
        // --- Quality stamp ------------------------------------------------
        b.as_mut()
            .set_np_quality_tier(QString::from(m.quality_tier.as_str()));
        let detail = QString::from(m.quality_detail.as_str());
        b.as_mut().set_np_quality_detail(detail.clone());
        // Legacy alias of the same value (pre-contract Qt name).
        b.as_mut().set_np_quality_label(detail);
        b.as_mut().set_np_quality_downgraded(m.delivered.downgraded);
        b.as_mut()
            .set_np_quality_true_detail(QString::from(m.delivered.true_detail.as_str()));
        b.as_mut()
            .set_np_quality_effective_tier(QString::from(m.delivered.effective_tier.as_str()));
        b.as_mut()
            .set_np_quality_limit_cause(m.delivered.limit_cause);
        // --- Cast / remote ------------------------------------------------
        b.as_mut().set_np_is_remote(m.is_remote);
        b.as_mut()
            .set_np_cast_target(QString::from(m.cast_target.as_str()));
        b.as_mut().set_np_cast_active(m.cast_active);
        b.as_mut()
            .set_np_cast_protocol(QString::from(m.cast_protocol.as_str()));
    });
}

/// Seed the bridge properties from the model at shell entry (the model is
/// static, so a logout/login round-trip keeps the last UI toggles — same
/// as the Slint app, where NowPlayingState survives).
pub fn publish_current() {
    let (_, snapshot) = with_model(|_| ());
    publish(&snapshot);
    // Seed the two output LEDs + the volume-lock flag at shell entry too, so
    // the stamp is correct before the first track ever plays (the bridge
    // defaults are the unlit SYST/DEFAULT pair).
    crate::output_labels::publish_current();
}

// --- Pure-UI toggles (mutate + republish) --------------------------------

pub fn set_volume(volume: f32) {
    mutate(|m| {
        m.volume = volume.clamp(0.0, 1.0);
        if m.volume > 0.0 {
            m.muted = false;
        }
    });
}

pub fn set_muted(muted: bool) {
    mutate(|m| m.muted = muted);
}

pub fn set_shuffle(shuffle: bool) {
    mutate(|m| m.shuffle = shuffle);
}

pub fn set_repeat_mode(mode: i32) {
    mutate(|m| m.repeat_mode = mode);
}

pub fn repeat_mode() -> i32 {
    with_model(|m| m.repeat_mode).0
}

pub fn set_playing(playing: bool) {
    mutate(|m| m.playing = playing);
}

// --- Track meta + position (fed by the poll pump) ------------------------

/// Current-track metadata as published by the playback controller.
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_id: String,
    pub artist_id: String,
    pub duration_secs: i32,
    pub quality_tier: String,
    pub quality_label: String,
    pub artwork_url: String,
    pub shuffle: bool,
    pub repeat_mode: i32,
}

/// A new current track: full meta swap (art path attaches separately via
/// the artwork pipeline — see `artwork_qt::attach_now_playing`).
///
/// Resets the DELIVERED block: the engine re-derives it once the new stream
/// opens, and the previous track's downgrade state must never linger.
pub fn set_track(meta: TrackMeta) {
    mutate(|m| {
        m.has_track = true;
        m.title = meta.title;
        m.artist = meta.artist;
        m.album = meta.album;
        m.album_id = meta.album_id;
        m.artist_id = meta.artist_id;
        m.duration_secs = meta.duration_secs;
        m.elapsed_secs = 0;
        m.cache = 0.0;
        m.quality_tier = meta.quality_tier;
        m.quality_detail = meta.quality_label;
        m.delivered = Delivered::default();
        m.eff_rate_hz = 0;
        m.eff_bits = 0;
        m.artwork_path = String::new();
        m.loading = true;
        m.playing = true;
        m.shuffle = meta.shuffle;
        m.repeat_mode = meta.repeat_mode;
    });
    // Stash the url out-of-band so the artwork pipeline can resolve it
    // without a second full publish.
    *ARTWORK_URL.lock().unwrap() = meta.artwork_url;
}

/// Seed the CATALOG maximum + the requested tier for the new current track
/// (playback.rs `refresh_now_playing_meta`: the TRACK_MAX / REQUESTED stores).
/// Resolved ONCE per track change — never per poll tick, because the request
/// tier is a preferences read.
///
/// `governed` = the streaming-quality preference shapes this track's request
/// (Qobuz-sourced, not local/Plex); ungoverned tracks keep the cause line off.
/// Publishes nothing on its own: the seed only changes what the NEXT
/// `set_effective_stream` computes.
#[allow(dead_code)] // wired by the poll pump / a cast service (see GLUE)
pub fn set_catalog_quality(bit_depth: Option<u32>, sample_rate: Option<f64>, governed: bool) {
    crate::quality_state::seed_track(bit_depth, sample_rate, governed);
}

/// The engine's DELIVERED stream params, from the poll tick's PlaybackEvent
/// (`sample_rate` / `bit_depth`; 0 = not reported yet). Re-evaluates the
/// downgrade block and republishes ONLY when the params actually moved, so a
/// steady stream costs one atomic compare per tick and no Qt hop.
#[allow(dead_code)] // wired by the poll pump / a cast service (see GLUE)
pub fn set_effective_stream(eff_rate_hz: u32, eff_bits: u32) {
    let (changed, snapshot) = with_model(|m| {
        if m.eff_rate_hz == eff_rate_hz && m.eff_bits == eff_bits {
            return false;
        }
        m.eff_rate_hz = eff_rate_hz;
        m.eff_bits = eff_bits;
        m.delivered = crate::quality_state::evaluate(eff_rate_hz, eff_bits);
        true
    });
    if changed {
        publish(&snapshot);
        // STREAM edge: the engine has just reported real params, i.e. the
        // output stream is open and the backend/mode are now facts. Re-derive
        // the two LEDs so they are right even if the audio settings moved
        // between the track edge and the stream actually opening. Gated by
        // `changed`, so a steady stream costs nothing — this is not a poll.
        crate::output_labels::publish_current();
    }
}

/// No current track (queue cleared / finished): back to the idle bar. Keeps
/// the user's volume/mute and the cast/remote session (a session outlives the
/// track).
pub fn clear_track() {
    mutate(|m| {
        let kept = NowPlayingModel {
            volume: m.volume,
            muted: m.muted,
            is_remote: m.is_remote,
            cast_target: m.cast_target.clone(),
            cast_active: m.cast_active,
            cast_protocol: m.cast_protocol.clone(),
            ..NowPlayingModel::default()
        };
        *m = kept;
    });
    crate::quality_state::seed_track(None, None, false);
}

// --- Cast / remote (wiring seam — no cast service in the POC build) -------

/// A Chromecast/DLNA session connected or dropped (`cast_service.rs`
/// `push_connection_state`). `protocol` is "cast" | "dlna".
#[allow(dead_code)] // wired by the poll pump / a cast service (see GLUE)
pub fn set_cast_session(active: bool, protocol: &str, target: &str) {
    mutate(|m| {
        m.cast_active = active;
        m.cast_protocol = if active { protocol.to_string() } else { String::new() };
        if active {
            m.cast_target = target.to_string();
        } else if !m.is_remote {
            m.cast_target.clear();
        }
    });
}

/// A peer Qobuz Connect renderer took over (or handed back) the transport
/// (`qconnect_event_sink.rs` `refresh_transport_modes`). `target` is the
/// renderer's friendly name, empty when local.
#[allow(dead_code)] // wired by the poll pump / a cast service (see GLUE)
pub fn set_remote(is_remote: bool, target: &str) {
    mutate(|m| {
        m.is_remote = is_remote;
        if is_remote {
            m.cast_target = target.to_string();
        } else if !m.cast_active {
            m.cast_target.clear();
        }
    });
}

/// The DELIVERED quality measured by a cast session, which the local poll
/// cannot see (the local player is stopped while casting —
/// `cast_service.rs` `publish_delivered_quality`). `limit_cause` is a
/// `qbz_models::QualityLimit` discriminant already classified by the caller.
#[allow(dead_code)] // wired by the poll pump / a cast service (see GLUE)
pub fn set_cast_delivered(delivered: Delivered) {
    mutate(|m| m.delivered = delivered);
}

/// The artwork url of the current track, stashed for the artwork pipeline.
static ARTWORK_URL: Mutex<String> = Mutex::new(String::new());

/// Attach a resolved artwork path to the current track (artwork pipeline).
pub fn set_artwork_path(path: String) {
    mutate(|m| m.artwork_path = path);
}

/// 1 Hz position push from the poll pump. `has_audio` = the engine
/// surfaced a track id (clears the loading spinner).
pub fn set_position(
    elapsed_secs: i32,
    duration_secs: i32,
    playing: bool,
    cache: f32,
    has_audio: bool,
) {
    mutate(|m| {
        m.elapsed_secs = elapsed_secs;
        if duration_secs > 0 {
            m.duration_secs = duration_secs;
        }
        m.playing = playing;
        if has_audio {
            m.loading = false;
        }
        // progress is derived in publish() from elapsed/duration; cache is
        // stored raw for the seekbar's buffer overlay.
        m.cache = cache;
    });
}
