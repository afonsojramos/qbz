//! QbzPlayer — now-playing/transport domain bridge (phase 23 split of the
//! QbzBridge God-object; the pattern is documented in main.rs). Props: the
//! NowPlayingState mirror (np_ prefix) + the playing-row indicator ids +
//! show-context-icon. Invokables: transport + the play/enqueue entry points
//! (album / artist / track / playlist) — one-line forwards into the crate
//! handlers.

use std::pin::Pin;
use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

#[cxx_qt::bridge]
pub mod qbz_player {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // --- Now playing (Slint NowPlayingState; np_ prefix) ------------
        // POC: fed by a static NowPlayingModel (src/now_playing.rs) with
        // empty-state defaults; phase 4 swaps the data source for the real
        // player poll.
        #[qproperty(bool, np_has_track)]
        #[qproperty(QString, np_title)]
        #[qproperty(QString, np_artist)]
        #[qproperty(QString, np_artwork_path)]
        #[qproperty(i32, np_elapsed_secs)]
        #[qproperty(i32, np_duration_secs)]
        #[qproperty(f32, np_progress)]
        #[qproperty(f32, np_cache_progress)]
        #[qproperty(bool, np_playing)]
        #[qproperty(bool, np_loading)]
        #[qproperty(f32, np_volume)]
        #[qproperty(bool, np_muted)]
        #[qproperty(bool, np_shuffle)]
        // 0 off / 1 all / 2 one.
        #[qproperty(i32, np_repeat_mode)]
        // "hires" | "mp3" | "lossless" | "cd" (AudioStamp tier mapping).
        #[qproperty(QString, np_quality_tier)]
        // e.g. "24-bit / 96 kHz" (AudioStamp detail line).
        #[qproperty(QString, np_quality_label)]
        // Now-playing track id (playing-row indicator in track lists).
        #[qproperty(QString, np_track_id)]
        #[qproperty(QString, np_album)]
        #[qproperty(QString, np_album_id)]
        #[qproperty(QString, np_artist_id)]
        // "Show track playing context" pref (Playback settings) — feeds the
        // SongCard layers icon.
        #[qproperty(bool, show_context_icon)]

        type QbzPlayer = super::QbzPlayerRust;

        /// Registers this object's Qt-thread hop (Main.qml boots EVERY
        /// domain singleton; only QbzSession.boot also fires crate::on_boot).
        #[qinvokable]
        fn boot(self: Pin<&mut QbzPlayer>);

        // --- Transport (phase 4 wires the player; POC log-and-noop, except
        // the pure-UI toggles which mutate the NowPlayingModel) -----------
        #[qinvokable]
        fn toggle_play(self: Pin<&mut QbzPlayer>);
        #[qinvokable]
        fn next(self: Pin<&mut QbzPlayer>);
        #[qinvokable]
        fn previous(self: Pin<&mut QbzPlayer>);
        #[qinvokable]
        fn seek(self: Pin<&mut QbzPlayer>, frac: f32);
        #[qinvokable]
        fn set_volume(self: Pin<&mut QbzPlayer>, volume: f32);
        #[qinvokable]
        fn toggle_mute(self: Pin<&mut QbzPlayer>);
        #[qinvokable]
        fn toggle_shuffle(self: Pin<&mut QbzPlayer>);
        #[qinvokable]
        fn cycle_repeat(self: Pin<&mut QbzPlayer>);

        /// Album-card click on Home: resolve the album's tracks, enqueue,
        /// and play through the core's resolved path.
        #[qinvokable]
        fn play_album(self: Pin<&mut QbzPlayer>, album_id: QString);

        /// AlbumView header Shuffle.
        #[qinvokable]
        fn play_album_shuffled(self: Pin<&mut QbzPlayer>, album_id: QString);
        /// AlbumView row play: the album starting AT this track.
        #[qinvokable]
        fn play_album_from(self: Pin<&mut QbzPlayer>, album_id: QString, track_id: QString);
        /// AlbumView row "Play next" ("next") / "Add to queue" ("later").
        #[qinvokable]
        fn enqueue_album_track(self: Pin<&mut QbzPlayer>, album_id: QString, track_id: QString, mode: QString);
        /// AlbumCard ⋯ menu: Play next ("next") / Add to queue ("later").
        #[qinvokable]
        fn enqueue_album(self: Pin<&mut QbzPlayer>, album_id: QString, mode: QString);
        /// ArtistView Popular Tracks row play (whole list as the queue).
        #[qinvokable]
        fn play_artist_track(self: Pin<&mut QbzPlayer>, track_id: QString);
        /// ArtistView "Play all" (shuffle=false) / "Shuffle all" (true).
        #[qinvokable]
        fn play_artist_top(self: Pin<&mut QbzPlayer>, shuffle: bool);
        /// ArtistView ⋯ "Add all to queue" — appends the top-tracks queue.
        #[qinvokable]
        fn enqueue_artist_top(self: Pin<&mut QbzPlayer>);
        /// Artist-card overlay play (ArtistGridCard): Popular tracks,
        /// falling back to the studio discography (playback.rs play_artist).
        #[qinvokable]
        fn play_artist_card(self: Pin<&mut QbzPlayer>, artist_id: QString);

        /// Track-row click (Library): play the track as a 1-element queue.
        #[qinvokable]
        fn play_track(self: Pin<&mut QbzPlayer>, track_id: QString);
        /// Library track context menus: Play next ("next") / Play later
        /// ("later") / Add to queue ("queue") on a single feed track.
        #[qinvokable]
        fn enqueue_track(self: Pin<&mut QbzPlayer>, track_id: QString, mode: QString);

        /// Card-level playlist actions (LibPlaylistCard overlay + menu).
        #[qinvokable]
        fn play_playlist_by_id(self: Pin<&mut QbzPlayer>, playlist_id: QString);
        #[qinvokable]
        fn enqueue_playlist_by_id(self: Pin<&mut QbzPlayer>, playlist_id: QString, mode: QString);
    }

    impl cxx_qt::Threading for QbzPlayer {}
}

use qbz_player::QbzPlayer;

/// Rust side of the player bridge (plain storage, phase-1 pattern).
pub struct QbzPlayerRust {
    np_has_track: bool,
    np_title: QString,
    np_artist: QString,
    np_artwork_path: QString,
    np_elapsed_secs: i32,
    np_duration_secs: i32,
    np_progress: f32,
    np_cache_progress: f32,
    np_playing: bool,
    np_loading: bool,
    np_volume: f32,
    np_muted: bool,
    np_shuffle: bool,
    np_repeat_mode: i32,
    np_quality_tier: QString,
    np_quality_label: QString,
    np_track_id: QString,
    np_album: QString,
    np_album_id: QString,
    np_artist_id: QString,
    show_context_icon: bool,
}

impl Default for QbzPlayerRust {
    fn default() -> Self {
        Self {
            // A derive would zero these — the model's sane defaults instead.
            np_volume: 1.0,
            np_quality_tier: QString::from("cd"),
            np_has_track: false,
            np_title: QString::default(),
            np_artist: QString::default(),
            np_artwork_path: QString::default(),
            np_elapsed_secs: 0,
            np_duration_secs: 0,
            np_progress: 0.0,
            np_cache_progress: 0.0,
            np_playing: false,
            np_loading: false,
            np_muted: false,
            np_shuffle: false,
            np_repeat_mode: 0,
            np_quality_label: QString::default(),
            np_track_id: QString::default(),
            np_album: QString::default(),
            np_album_id: QString::default(),
            np_artist_id: QString::default(),
            show_context_icon: crate::settings_qt::show_context_icon(),
        }
    }
}

// ---------------------------------------------------------------------------
// The UI hop (bridges/mod.rs pattern)
// ---------------------------------------------------------------------------

static QT_THREAD: OnceLock<CxxQtThread<QbzPlayer>> = OnceLock::new();

/// Queue a player-bridge mutation onto the Qt event loop (no-op before
/// boot registers the thread).
pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzPlayer>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}

impl qbz_player::QbzPlayer {
    pub fn boot(self: Pin<&mut Self>) {
        if QT_THREAD.set(self.qt_thread()).is_err() {
            log::warn!("[qbz-qt] player Qt thread already registered");
        }
    }

    pub fn toggle_play(self: Pin<&mut Self>) {
        crate::transport_toggle_play();
    }

    pub fn next(self: Pin<&mut Self>) {
        crate::transport_next();
    }

    pub fn previous(self: Pin<&mut Self>) {
        crate::transport_previous();
    }

    pub fn seek(self: Pin<&mut Self>, frac: f32) {
        crate::transport_seek(frac);
    }

    pub fn set_volume(self: Pin<&mut Self>, volume: f32) {
        crate::transport_set_volume(volume);
    }

    pub fn toggle_mute(self: Pin<&mut Self>) {
        crate::transport_toggle_mute();
    }

    pub fn toggle_shuffle(self: Pin<&mut Self>) {
        crate::transport_toggle_shuffle();
    }

    pub fn cycle_repeat(self: Pin<&mut Self>) {
        crate::transport_cycle_repeat();
    }

    pub fn play_album(self: Pin<&mut Self>, album_id: QString) {
        crate::play_album(album_id.to_string());
    }

    pub fn play_album_shuffled(self: Pin<&mut Self>, album_id: QString) {
        crate::play_album_shuffled(album_id.to_string());
    }

    pub fn play_album_from(self: Pin<&mut Self>, album_id: QString, track_id: QString) {
        if let Ok(tid) = track_id.to_string().parse::<u64>() {
            crate::play_album_from_track(album_id.to_string(), tid);
        }
    }

    pub fn enqueue_album_track(self: Pin<&mut Self>, album_id: QString, track_id: QString, mode: QString) {
        if let Ok(tid) = track_id.to_string().parse::<u64>() {
            crate::enqueue_album_track(album_id.to_string(), tid, mode.to_string());
        }
    }

    pub fn enqueue_album(self: Pin<&mut Self>, album_id: QString, mode: QString) {
        crate::enqueue_album(album_id.to_string(), mode.to_string());
    }

    pub fn play_artist_track(self: Pin<&mut Self>, track_id: QString) {
        if let Ok(tid) = track_id.to_string().parse::<u64>() {
            crate::play_artist_track(tid);
        }
    }

    pub fn play_artist_top(self: Pin<&mut Self>, shuffle: bool) {
        crate::play_artist_top(shuffle);
    }

    pub fn enqueue_artist_top(self: Pin<&mut Self>) {
        crate::enqueue_artist_top();
    }

    pub fn play_artist_card(self: Pin<&mut Self>, artist_id: QString) {
        crate::play_artist_card(artist_id.to_string());
    }

    pub fn play_track(self: Pin<&mut Self>, track_id: QString) {
        if let Ok(id) = track_id.to_string().parse::<u64>() {
            crate::play_track(id);
        }
    }

    pub fn enqueue_track(self: Pin<&mut Self>, track_id: QString, mode: QString) {
        if let Ok(id) = track_id.to_string().parse::<u64>() {
            crate::enqueue_track(id, mode.to_string());
        }
    }

    pub fn play_playlist_by_id(self: Pin<&mut Self>, playlist_id: QString) {
        if let Ok(pid) = playlist_id.to_string().parse::<u64>() {
            crate::play_playlist_by_id(pid);
        }
    }

    pub fn enqueue_playlist_by_id(self: Pin<&mut Self>, playlist_id: QString, mode: QString) {
        if let Ok(pid) = playlist_id.to_string().parse::<u64>() {
            crate::enqueue_playlist_by_id(pid, mode.to_string());
        }
    }
}
