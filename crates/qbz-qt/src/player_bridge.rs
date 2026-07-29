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
        // --- Quality stamp (Slint NowPlayingState quality-* block) -------
        // CATALOG max of the track: "hires" | "mp3" | "lossless" | "cd" | "".
        #[qproperty(QString, np_quality_tier)]
        // Exact detail line of the catalog max, e.g. "24-bit / 96 kHz".
        #[qproperty(QString, np_quality_detail)]
        // Legacy alias of np_quality_detail (the pre-contract Qt name) —
        // published from the same value so existing QML keeps working.
        #[qproperty(QString, np_quality_label)]
        // DELIVERED-vs-catalog state (#590/#638). While `downgraded` is on,
        // the stamp's main line reports the delivered tier/detail and the
        // catalog max moves to the tooltip's "Source" line.
        #[qproperty(bool, np_quality_downgraded)]
        // Delivered detail line while downgraded ("16-bit / 44.1 kHz",
        // "DSD64"); empty when the engine has not reported params yet.
        #[qproperty(QString, np_quality_true_detail)]
        // Delivered tier while downgraded ("hires"|"cd"|"mp3"), else "".
        #[qproperty(QString, np_quality_effective_tier)]
        // WHY the stream is below the catalog max (qbz_models::QualityLimit):
        // 0 none · 1 streaming-quality setting · 2 local output device cap ·
        // 3 cast renderer cap · 4 Qobuz offered nothing higher.
        #[qproperty(i32, np_quality_limit_cause)]

        // --- Output LEDs (settings.rs `output_labels`) -------------------
        // PIPEWIRE|ALSA|JACK|PULS|SYST|AUTO — *_active lights the LED.
        #[qproperty(QString, np_output_backend_label)]
        #[qproperty(bool, np_output_backend_active)]
        // DACPASS|BITPERF|EXCL|DIRECT|LOCKED|ROUTED|SHARED|DEFAULT.
        #[qproperty(QString, np_output_mode_label)]
        #[qproperty(bool, np_output_mode_active)]
        // The app's software volume is INERT on this route — the ALSA Direct
        // engine's `set_volume` is a no-op unless the DAC's own mixer is
        // driven (`alsa_hardware_volume`). Derived read-only in
        // `output_labels::volume_locked`; the bars render the volume slider
        // disabled with a tooltip instead of pretending it works. No audio
        // behaviour hangs off this flag — it is a UI mirror only.
        #[qproperty(bool, np_volume_locked)]

        // --- Cast / remote (Qobuz Connect + Chromecast/DLNA) -------------
        // A peer Qobuz Connect renderer owns playback (transport is remote).
        #[qproperty(bool, np_is_remote)]
        // Name of the active renderer / cast target; empty when local.
        #[qproperty(QString, np_cast_target)]
        // A Chromecast/DLNA session is connected — while casting the two
        // output LEDs fuse into a single cast chip.
        #[qproperty(bool, np_cast_active)]
        // "cast" | "dlna".
        #[qproperty(QString, np_cast_protocol)]
        // Now-playing track id (playing-row indicator in track lists).
        #[qproperty(QString, np_track_id)]
        #[qproperty(QString, np_album)]
        #[qproperty(QString, np_album_id)]
        #[qproperty(QString, np_artist_id)]
        // "Playing from" ORIGIN of the current track (NowPlayingState
        // context-kind / context-id): the container the queue was launched
        // from, republished on every track change. kind is "album" | "artist"
        // | "playlist" | "label"; the song-card layers glyph navigates there.
        #[qproperty(QString, np_context_kind)]
        #[qproperty(QString, np_context_id)]
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
    np_quality_detail: QString,
    np_quality_label: QString,
    np_quality_downgraded: bool,
    np_quality_true_detail: QString,
    np_quality_effective_tier: QString,
    np_quality_limit_cause: i32,
    np_output_backend_label: QString,
    np_output_backend_active: bool,
    np_output_mode_label: QString,
    np_output_mode_active: bool,
    np_volume_locked: bool,
    np_is_remote: bool,
    np_cast_target: QString,
    np_cast_active: bool,
    np_cast_protocol: QString,
    np_track_id: QString,
    np_album: QString,
    np_album_id: QString,
    np_artist_id: QString,
    np_context_kind: QString,
    np_context_id: QString,
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
            np_quality_detail: QString::default(),
            np_quality_label: QString::default(),
            np_quality_downgraded: false,
            np_quality_true_detail: QString::default(),
            np_quality_effective_tier: QString::default(),
            np_quality_limit_cause: 0,
            // The Slint NowPlayingState defaults (state.slint) — an unlit
            // "SYST / DEFAULT" pair until the first settings snapshot.
            np_output_backend_label: QString::from("SYST"),
            np_output_backend_active: false,
            np_output_mode_label: QString::from("DEFAULT"),
            np_output_mode_active: false,
            // Optimistic default: the slider stays live until the first
            // settings read proves the route is an inert ALSA-direct one.
            np_volume_locked: false,
            np_is_remote: false,
            np_cast_target: QString::default(),
            np_cast_active: false,
            np_cast_protocol: QString::default(),
            np_track_id: QString::default(),
            np_album: QString::default(),
            np_album_id: QString::default(),
            np_artist_id: QString::default(),
            np_context_kind: QString::default(),
            np_context_id: QString::default(),
            // Seed only — this runs when the QML engine constructs the
            // singleton, which can be BEFORE the playback-preferences store is
            // open (then it reads false). `now_playing::publish_show_context_icon`
            // re-publishes it on shell entry and on the Settings toggle.
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
