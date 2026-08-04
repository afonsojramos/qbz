//! System media controls — MPRIS on Linux (what makes the KDE Plasma
//! now-playing widget, GNOME Shell's media section and the hardware media keys
//! see QBZ at all), SMTC / MediaRemote on Windows / macOS.
//!
//! The backend is the frontend-agnostic `qbz-media-controls` crate (ADR-006),
//! consumed VERBATIM — this module is only the Qt-side glue: it owns the
//! process-global handle, the metadata de-dupe, the artwork resolver MPRIS
//! needs, and it bridges inbound control events onto the transport verbs.
//! Playback metadata/state is PUSHED from `playback_qt.rs`, mirroring the tray
//! (`src/tray_qt.rs`).
//!
//! Ported from `crates/qbz/src/media_controls.rs`; where the Slint glue and
//! the daemon's (`crates/qbzd/src/mpris.rs`) disagree, the DAEMON is the
//! precedent, because qbz-qt is likewise not a Slint app:
//!
//!  - **No captured runtime.** The Slint closure captures a strong
//!    `Arc<AppRuntime>` plus a `slint::Weak<AppWindow>` and a tokio `Handle`
//!    (`media_controls.rs:29-35`); the daemon deliberately holds only a
//!    `Weak` so the integration can never pin the runtime (#521 audio-release
//!    ordering, `qbzd/src/mpris.rs:15-18`). Qt needs NEITHER: `crate::app()`
//!    (`src/main.rs:312`) and `crate::spawn()` (`:317`) are process-global
//!    `OnceLock`s, so the callback captures nothing at all and the inbound
//!    `fn` is a plain function item. Same property `tray_qt.rs:224-228` cites
//!    for the ksni thread.
//!  - **Sync core commands run on the D-Bus thread.** The callback runs on the
//!    mpris-server thread — NOT a tokio worker, NOT the Qt thread
//!    (`qbzd/src/mpris.rs:148-151`). `core().stop()` and
//!    `core().get_playback_state()` are synchronous, so they are called
//!    directly like the daemon does, and everything async is handed to the
//!    `crate::transport_*` verbs, which do their own `app()` + `spawn()`.
//!
//! ## Threading — do NOT "simplify" any of this
//!
//! OUTBOUND (`set_metadata` / `set_playback`) is free from any thread: the
//! Linux handle's methods are non-blocking sends onto the server's channel.
//! INBOUND must never block: the D-Bus thread services every method call, so
//! nothing here waits on the network — the transport verbs return immediately
//! after spawning.
//!
//! This process ALREADY publishes on D-Bus (the ksni StatusNotifierItem on its
//! own thread, plus whatever libQt6DBus opens). MPRIS adds a THIRD connection
//! on its own thread. They coexist only because zbus 4 (mpris-server) and
//! zbus 5 (ksni / ashpd / rfd) unify features per-major: **never enable a
//! `tokio` feature on any D-Bus dependency** or the async-io threads panic
//! with "there is no reactor running" (see the ksni comment in Cargo.toml).
//!
//! ## No teardown
//!
//! Like the tray (`tray_qt.rs:230-234`) and the reference (`grep
//! 'media_controls' crates/qbz/src/auth.rs` → 0 hits) the integration is
//! process-scoped: `init` is idempotent, logout does not touch it, and there
//! is no `shutdown`. Do not add one.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use qbz_media_controls::{MediaEvent, MediaIntegration};

/// The live integration, or `None` when it never started (no session bus, an
/// unsupported platform, or the name was already owned). Boxed behind a
/// `OnceLock` exactly like `tray_qt::TRAY` (`src/tray_qt.rs:159`).
static CONTROLS: OnceLock<Box<dyn MediaIntegration>> = OnceLock::new();

/// Last `(track id, resolved art URL)` pushed as metadata.
/// `refresh_now_playing` re-runs on resume / seek / republish — it has 23 call
/// sites across nine modules (`src/playback_qt.rs:1777-1779`) — so metadata is
/// only re-pushed when this key actually changes. 1:1 with the reference's
/// `MPRIS_LAST_META` (`crates/qbz/src/playback.rs:1723-1724`), including the
/// art field: the URL a widget can load changes when the cover resolves
/// differently (offline), which the track id alone cannot see.
/// `None` = nothing pushed yet / cleared.
static MPRIS_LAST_META: Mutex<Option<(u64, Option<String>)>> = Mutex::new(None);

/// The live integration, if it started. Playback pushes metadata/state through
/// it. Mirrors `tray_qt::handle()` (`src/tray_qt.rs:165`).
pub(crate) fn handle() -> Option<&'static dyn MediaIntegration> {
    CONTROLS.get().map(|b| b.as_ref())
}

/// Start the OS media-controls integration once. No-op if already started or
/// unavailable on this platform.
///
/// NOT platform-gated, because the shared crate is not: `qbz_media_controls::
/// spawn` routes to the MPRIS backend on Linux, souvlaki on macOS/Windows and
/// returns `None` everywhere else (`crates/qbz-media-controls/src/lib.rs:36-53`),
/// so both consumers declare the dependency plain (`crates/qbz/Cargo.toml:80`,
/// `crates/qbzd/Cargo.toml:22`). Linux is the platform this port CLAIMS: under
/// Qt the souvlaki callbacks would have to be serviced by a Qt-owned run loop,
/// which nothing in this tree establishes.
///
/// A non-`None` handle does NOT mean the bus name was acquired — registration
/// happens asynchronously on the server thread and only logs on failure. The
/// smoke check is the `[mpris] registered org.mpris.MediaPlayer2.com.blitzfc.qbz`
/// line, not this one.
pub(crate) fn init() {
    if CONTROLS.get().is_some() {
        return;
    }
    match qbz_media_controls::spawn(dispatch) {
        Some(c) => {
            let _ = CONTROLS.set(c);
            log::info!("[media-controls] integration started");
        }
        None => log::info!("[media-controls] no integration on this platform"),
    }
}

// ---------------------------------------------------------------------------
// Outbound — pushed from playback_qt (siblings of the tray pushes)
// ---------------------------------------------------------------------------

/// The has-track arm: metadata (de-duped) plus the OPTIMISTIC `Playing` at
/// position 0. 1:1 with `crates/qbz/src/playback.rs:2208-2223`, where
/// `set_playback` is deliberately unconditional — a stream is about to open,
/// and the poll loop's play/pause edge corrects it if it does not.
///
/// `title` is the VERSIONED title and `album` the release-variant
/// `album_display`, NOT the clean `track.album` the tray tooltip uses
/// (`playback.rs:2213` vs `:2164`) — the two are not interchangeable.
///
/// Pushing `set_playback` on every call is also what keeps the sleep inhibitor
/// alive: the Linux backend hangs #522's inhibit on the playback update, and
/// the inhibitor type is private, so there is no other way to reach it.
pub(crate) fn push_now_playing(track: &qbz_models::QueueTrack, title: &str, album: &str) {
    let Some(mc) = handle() else {
        return;
    };
    let art = art_url_for(track);
    if meta_changed(&(track.id, art.clone())) {
        // `qbz_media_controls::TrackMeta` — NOT `crate::now_playing::TrackMeta`
        // (`src/now_playing.rs:261`), a different 13-field struct. Fully
        // qualified at every use in this module for that reason.
        mc.set_metadata(&qbz_media_controls::TrackMeta {
            title: title.to_string(),
            artist: track.artist.clone(),
            album: album.to_string(),
            duration: (track.duration_secs > 0)
                .then(|| Duration::from_secs(track.duration_secs)),
            art_url: art,
            // `xesam:url` (#658): the public Qobuz link for catalog rows, None
            // for local/plex (`qbz-media-controls/src/types.rs:30-36`).
            url: qbz_media_controls::xesam_url_for(track.source.as_deref(), track.id),
        });
        // One line per real track edge. It exists because "Plasma sees
        // nothing" has two very different causes — the player never
        // registered, or it registered and no metadata was ever pushed — and
        // without this the log cannot tell them apart. A desktop applet
        // generally hides a player that is Stopped with empty metadata, so
        // the FIRST push is the interesting event, not the hundredth.
        log::info!(
            "[mpris] metadata pushed: {} — {} ({}s)",
            title,
            track.artist,
            track.duration_secs
        );
    }
    mc.set_playback(
        qbz_media_controls::PlaybackStatus::Playing,
        Some(Duration::ZERO),
    );
}

/// The no-track arm: reset the de-dupe (so replaying the SAME track after a
/// stop re-pushes metadata) and tell the widget we stopped.
/// `crates/qbz/src/playback.rs:1913-1922`, in that order.
pub(crate) fn clear_now_playing() {
    if let Ok(mut last) = MPRIS_LAST_META.lock() {
        *last = None;
    }
    if let Some(mc) = handle() {
        mc.set_playback(qbz_media_controls::PlaybackStatus::Stopped, None);
    }
}

/// The play/pause TRANSITION arm, carrying the live position in seconds.
/// `crates/qbz/src/playback.rs:5723-5730`.
///
/// Edge-triggered on purpose: `set_playback` emits a D-Bus `PropertiesChanged`,
/// so a per-tick push would spam every listening widget once a second. Between
/// transitions the MPRIS `Position` getter answers from the last value pushed
/// here — the same behaviour as the Slint build.
pub(crate) fn push_playback_state(is_playing: bool, position_secs: u64) {
    let Some(mc) = handle() else {
        return;
    };
    let status = if is_playing {
        qbz_media_controls::PlaybackStatus::Playing
    } else {
        qbz_media_controls::PlaybackStatus::Paused
    };
    mc.set_playback(status, Some(Duration::from_secs(position_secs)));
}

/// Keep the reported position current, once per playback tick.
///
/// MPRIS `Position` is READ ON DEMAND — the spec keeps it out of
/// `PropertiesChanged` — so the backend serves whatever it last stored, and
/// before this it only stored one at a track start and at a play/pause edge.
/// A desktop widget therefore showed a value that could be minutes stale:
/// measured 2026-08-04 with QBZ at 4:03 and Plasma's widget stuck at 0:14.
///
/// `set_position` stores and emits nothing, so this is a channel send per
/// second and never a bus signal — it does not become the per-tick work
/// §7-M12 refuses.
pub(crate) fn push_position(position_secs: u64) {
    if let Some(mc) = handle() {
        mc.set_position(Duration::from_secs(position_secs));
    }
}

/// Report a user SEEK — a discontinuous jump, not the ordinary advance.
///
/// `push_position` fixes what a client sees when it OPENS; this is what
/// reaches one that is ALREADY open. The owner's observation is what made the
/// distinction concrete: *"el contador del seekbar del widget solo empieza a
/// contar hasta que lo abro"* — the widget extrapolates from the value it read
/// at open and never looks again, so only the `Seeked` signal can tell it the
/// ground moved.
pub(crate) fn push_seeked(position_secs: u64) {
    if let Some(mc) = handle() {
        mc.seeked(Duration::from_secs(position_secs));
    }
}

/// Compare-and-record the metadata de-dupe key. `true` when `key` differs from
/// the last pushed value (→ caller pushes now), recording it as the new
/// last-pushed value. A poisoned lock falls back to pushing.
/// `crates/qbz/src/playback.rs:1881-1893`.
fn meta_changed(key: &(u64, Option<String>)) -> bool {
    match MPRIS_LAST_META.lock() {
        Ok(mut last) => {
            if last.as_ref() == Some(key) {
                false
            } else {
                *last = Some(key.clone());
                true
            }
        }
        Err(_) => true,
    }
}

// ---------------------------------------------------------------------------
// The art resolver — `mpris:artUrl`
// ---------------------------------------------------------------------------

/// The cover URL for `mpris:artUrl`, or `None` when there is nothing a widget
/// could load (the property is then simply omitted — no placeholder).
///
/// This is NOT `artwork_qt::cached_path`. That returns Qt's `file_url`
/// (`src/artwork_qt.rs:184-199`), which escapes only `%`, `#` and `?` and
/// leaves spaces raw — good enough for `QUrl`, not a valid URI for a media
/// widget. The URI comes from `qbz_models::ArtworkRef::to_mpris_url`
/// (`crates/qbz-models/src/source.rs:161-181`), whose LocalFile arm goes
/// through `url::Url::from_file_path` (full percent-encoding), so both
/// frontends hand the desktop identical URLs.
///
/// Source-aware exactly like the reference (`crates/qbz/src/playback.rs:2000`):
/// the ref is built with the CURRENT Plex credentials at `size: None` — MPRIS
/// art wants the large image — so a Plex track yields a fetchable tokenized
/// URL instead of a `file://`-poisoned miss. Local / offline-download covers
/// are already on disk and pass through as a `file://` URI.
///
/// The offline branch is the B11 rule (`playback.rs:2168-2198`): ONLINE a
/// remote Qobuz cover keeps its `https://` URL untouched — widgets fetch it
/// themselves, and the disk cache's extension-less `<md5>.img` copy is
/// REJECTED by the freedesktop mime database (`application/vnd.efi.img`),
/// which is what made every push carry a dead URL. OFFLINE nothing can fetch
/// https, so a cache hit becomes the `file://` copy (better than nothing for
/// widgets that sniff content) and a miss becomes no art at all. The cache is
/// therefore only consulted while offline — one fewer SQLite touch per track
/// on the normal path, with an identical result.
pub(crate) fn art_url_for(track: &qbz_models::QueueTrack) -> Option<String> {
    let plex = crate::local_plex::settings();
    let artwork = track.artwork_ref_with_plex(&plex.base_url, &plex.token, None);
    let mut art = artwork.to_mpris_url();
    if crate::offline_fwd::engine().is_offline() {
        if let qbz_models::ArtworkRef::Remote(url) = &artwork {
            art = crate::artwork_qt::cached_raw_path(url).and_then(|path| {
                qbz_models::ArtworkRef::LocalFile(path.to_string_lossy().into_owned())
                    .to_mpris_url()
            });
        }
    }
    art
}

// ---------------------------------------------------------------------------
// Inbound — media keys, the Plasma/GNOME widget, playerctl
// ---------------------------------------------------------------------------

/// Map one inbound `MediaEvent` onto a transport verb. Runs on the mpris-server
/// (D-Bus) thread — not a tokio worker, not the Qt thread — so nothing here
/// blocks: the sync core reads are cheap and every async command goes through
/// `crate::transport_*`, which spawns onto the process-global runtime.
/// Time values are MICROSECONDS (`qbz-media-controls/src/types.rs:47`).
fn dispatch(ev: MediaEvent) {
    match ev {
        // The OS only sends Play when paused and Pause when playing (it reads
        // PlaybackStatus), so routing all three through the toggle is correct
        // and keeps the cast → QConnect → local ladder the tray verbs ride
        // (`src/tray_qt.rs:487-500`).
        MediaEvent::Play | MediaEvent::Pause | MediaEvent::Toggle => {
            crate::tray_qt::dispatch_play_pause()
        }
        MediaEvent::Next => crate::tray_qt::dispatch_next(),
        MediaEvent::Previous => crate::tray_qt::dispatch_previous(),
        // The one command with no transport verb — direct on the core, like the
        // daemon (`qbzd/src/mpris.rs:90-92`) and like the two existing Qt call
        // sites (`src/cast_qt.rs:309`, `src/qconnect_engine_qt.rs:92`).
        // `QbzCore::stop` is synchronous (`crates/qbz-core/src/core.rs:1060`).
        MediaEvent::Stop => {
            // Bound to a local first: `AppRuntime::core()` returns a BORROW
            // (`crates/qbz-app/src/shell.rs:144`) and `app()` hands back an
            // owned Arc, so the runtime must outlive the call — the same shape
            // `tray_qt::dispatch_volume_delta` uses (`src/tray_qt.rs:525`).
            let runtime = crate::app();
            if let Err(e) = runtime.core().stop() {
                log::warn!("[media-controls] stop failed: {e}");
            }
        }
        // Present, not show_window: with the miniplayer open a forced main-
        // window show reads as a duplicate instance (#559). `present()` was
        // left in exactly this shape for this caller and needs no event-loop
        // hop — it reads an AtomicBool mirror (`src/tray_qt.rs:442-462`).
        MediaEvent::Raise => crate::tray_qt::present(),
        MediaEvent::Quit => crate::tray_qt::quit(),
        // Local model first, then the engine — `transport_set_volume` does both
        // (`src/main.rs:1706-1711`) and its Qt hop is safe from any thread.
        MediaEvent::SetVolume(v) => crate::transport_set_volume((v as f32).clamp(0.0, 1.0)),
        MediaEvent::SetPosition(micros) => seek_to_micros(micros),
        MediaEvent::SeekBy(delta_micros) => seek_by_micros(delta_micros),
    }
}

/// Absolute seek (MPRIS `SetPosition`).
///
/// Micros → a 0..1 FRACTION, because that is this app's seek primitive
/// (`crate::transport_seek`, `src/main.rs:1701`), which alone carries the full
/// cast → QConnect → local ladder (`src/playback_qt.rs:1586-1621`). The Slint
/// glue converts the same way (`crates/qbz/src/media_controls.rs:72-95`); the
/// daemon converts to SECONDS instead only because `core.seek(secs)` is its
/// primitive.
fn seek_to_micros(micros: i64) {
    let runtime = crate::app();
    let state = runtime.core().get_playback_state();
    if let Some(fraction) = fraction_for(micros, state.duration) {
        crate::transport_seek(fraction);
    }
}

/// Relative seek (MPRIS `Seek`), signed micros.
fn seek_by_micros(delta_micros: i64) {
    // `position` / `duration` are SECONDS (`qbz-player`'s PlaybackState,
    // `crates/qbz-player/src/player/mod.rs:5447-5453`).
    let runtime = crate::app();
    let state = runtime.core().get_playback_state();
    let target = (state.position as i64) * 1_000_000 + delta_micros;
    if let Some(fraction) = fraction_for(target, state.duration) {
        crate::transport_seek(fraction);
    }
}

/// A 0..1 seek fraction for an absolute target in micros, or `None` when the
/// duration is unknown (a zero-duration seek would divide by zero — the
/// reference's `dur == 0` early return).
fn fraction_for(target_micros: i64, duration_secs: u64) -> Option<f32> {
    if duration_secs == 0 {
        return None;
    }
    let target = target_micros.max(0);
    Some((target as f64 / 1_000_000.0 / duration_secs as f64).clamp(0.0, 1.0) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_for_needs_a_known_duration() {
        assert_eq!(fraction_for(5_000_000, 0), None);
    }

    #[test]
    fn fraction_for_maps_micros_onto_the_track() {
        assert_eq!(fraction_for(0, 200), Some(0.0));
        assert_eq!(fraction_for(100_000_000, 200), Some(0.5));
        assert_eq!(fraction_for(200_000_000, 200), Some(1.0));
    }

    #[test]
    fn fraction_for_clamps_both_ends() {
        // A relative seek back past the start, and past the end.
        assert_eq!(fraction_for(-30_000_000, 200), Some(0.0));
        assert_eq!(fraction_for(999_000_000, 200), Some(1.0));
    }
}
