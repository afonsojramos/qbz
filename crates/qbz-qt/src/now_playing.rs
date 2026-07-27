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
    pub artwork_path: String,
    pub elapsed_secs: i32,
    pub duration_secs: i32,
    pub playing: bool,
    pub loading: bool,
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
            artwork_path: String::new(),
            elapsed_secs: 0,
            duration_secs: 0,
            playing: false,
            loading: false,
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
    artwork_path: String::new(),
    elapsed_secs: 0,
    duration_secs: 0,
    playing: false,
    loading: false,
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

pub fn toggle_mute() {
    mutate(|m| m.muted = !m.muted);
}

pub fn toggle_shuffle() {
    mutate(|m| m.shuffle = !m.shuffle);
}

pub fn cycle_repeat() {
    mutate(|m| m.repeat_mode = (m.repeat_mode + 1) % 3);
}

// --- Transport (log-and-noop until phase 4) ------------------------------

pub fn toggle_play() {
    // POC-NOTE: no player wired — phase 4 replaces this with the core
    // playback command through AppRuntime.
    log::info!("[qbz-qt] transport toggle-play (no-op until phase 4)");
}

pub fn next() {
    log::info!("[qbz-qt] transport next (no-op until phase 4)");
}

pub fn previous() {
    log::info!("[qbz-qt] transport previous (no-op until phase 4)");
}

pub fn seek(frac: f32) {
    log::info!("[qbz-qt] transport seek({frac:.3}) (no-op until phase 4)");
}
