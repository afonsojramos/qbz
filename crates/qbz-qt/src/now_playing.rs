//! POC now-playing model — the data source behind the bar's `np*` bridge
//! properties.
//!
//! POC-NOTE: playback is NOT wired (phase 4). This static model carries the
//! EMPTY state (no track) plus the pure-UI toggles (volume / mute / shuffle
//! / repeat) so the bar renders and behaves exactly like the Slint bar at
//! idle; `toggle_play` / `next` / `previous` / `seek` are log-and-noop
//! until the player poll replaces this model.

use std::sync::Mutex;

use cxx_qt_lib::QString;

#[derive(Clone)]
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
    /// "hires" | "mp3" | "lossless" | "cd".
    pub quality_tier: String,
    /// e.g. "24-bit / 96 kHz".
    pub quality_label: String,
}

impl Default for NowPlayingModel {
    fn default() -> Self {
        Self {
            has_track: false,
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            album_id: String::new(),
            artist_id: String::new(),
            artwork_path: String::new(),
            elapsed_secs: 0,
            duration_secs: 0,
            playing: false,
            loading: false,
            cache: 0.0,
            volume: 1.0,
            muted: false,
            shuffle: false,
            repeat_mode: 0,
            quality_tier: "cd".to_string(),
            quality_label: String::new(),
        }
    }
}

static MODEL: Mutex<NowPlayingModel> = Mutex::new(NowPlayingModel {
    has_track: false,
    title: String::new(),
    artist: String::new(),
    album: String::new(),
    album_id: String::new(),
    artist_id: String::new(),
    artwork_path: String::new(),
    elapsed_secs: 0,
    duration_secs: 0,
    playing: false,
    loading: false,
    cache: 0.0,
    volume: 1.0,
    muted: false,
    shuffle: false,
    repeat_mode: 0,
    quality_tier: String::new(),
    quality_label: String::new(),
});

fn mutate(f: impl FnOnce(&mut NowPlayingModel)) {
    let mut guard = MODEL.lock().unwrap();
    f(&mut guard);
    publish(&guard.clone());
}

/// Push the full model onto the bridge (Qt thread hop).
fn publish(m: &NowPlayingModel) {
    let m = m.clone();
    crate::ui(move |mut b| {
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
        // No streaming cache in the POC model — cache tracks progress.
        b.as_mut().set_np_cache_progress(progress);
        b.as_mut().set_np_playing(m.playing);
        b.as_mut().set_np_loading(m.loading);
        b.as_mut().set_np_volume(m.volume);
        b.as_mut().set_np_muted(m.muted);
        b.as_mut().set_np_shuffle(m.shuffle);
        b.as_mut().set_np_repeat_mode(m.repeat_mode);
        b.as_mut()
            .set_np_quality_tier(QString::from(m.quality_tier.as_str()));
        b.as_mut()
            .set_np_quality_label(QString::from(m.quality_label.as_str()));
    });
}

/// Seed the bridge properties from the model at shell entry (the model is
/// static, so a logout/login round-trip keeps the last UI toggles — same
/// as the Slint app, where NowPlayingState survives).
pub fn publish_current() {
    let guard = MODEL.lock().unwrap();
    publish(&guard.clone());
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
    MODEL.lock().unwrap().repeat_mode
}

pub fn set_playing(playing: bool) {
    mutate(|m| m.playing = playing);
}

// --- Track meta + position (phase 4: fed by the poll pump) ---------------

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
        m.quality_tier = meta.quality_tier;
        m.quality_label = meta.quality_label;
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

/// No current track (queue cleared / finished): back to the idle bar.
pub fn clear_track() {
    mutate(|m| {
        *m = NowPlayingModel {
            volume: m.volume,
            muted: m.muted,
            ..NowPlayingModel::default()
        };
    });
}

/// The artwork url of the current track, stashed for the artwork pipeline.
static ARTWORK_URL: Mutex<String> = Mutex::new(String::new());

/// Attach a resolved artwork path to the current track (artwork pipeline).
pub fn set_artwork_path(path: String) {
    mutate(|m| m.artwork_path = path);
}

/// 1 Hz position push from the poll pump. `has_audio` = the engine
/// surfaced a track id (clears the loading spinner).
pub fn set_position(elapsed_secs: i32, duration_secs: i32, playing: bool, cache: f32, has_audio: bool) {
    let mut guard = MODEL.lock().unwrap();
    {
        let m = &mut *guard;
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
    }
    let m = guard.clone();
    drop(guard);
    publish_with_cache(&m);
}

/// Same as `publish` but honors the poll-fed cache value instead of
/// mirroring progress.
fn publish_with_cache(m: &NowPlayingModel) {
    let m = m.clone();
    crate::ui(move |mut b| {
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
        b.as_mut()
            .set_np_quality_tier(QString::from(m.quality_tier.as_str()));
        b.as_mut()
            .set_np_quality_label(QString::from(m.quality_label.as_str()));
    });
}
