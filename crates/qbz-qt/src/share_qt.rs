//! Share links + system clipboard — the Qt port of `crates/qbz/src/share.rs`
//! (lines 1-70: the URL builders and the clipboard). Driven by the artist
//! header's ⋯ → Share (`qml/views/ArtistView.qml`, the overflow menu).
//!
//! **Only the first half of the reference is ported.** `share.rs:72-184`
//! (`share_http_client`, `deezer_lookup`, `songlink_for_track`,
//! `albumlink_for_album`, `songlink_url`) resolves Song.link / Album.link for
//! the TRACK and ALBUM context menus. Artists have no such path anywhere in
//! the Slint tree — `main.rs:12749` ("artist","share") is two statements, copy
//! + toast — and this port has no Song.link call site at all, so porting those
//! resolvers now would be dead async HTTP. The module header of the reference
//! naming "Song.link" is about track/album only; do not read it as an artist
//! feature.
//!
//! **The `open.` vs `play.` split is deliberate, not a typo.** Track and album
//! moved to `open.qobuz.com` for #514 (`share.rs:4`, `:14-16`, which records
//! that Tauri used `play.qobuz.com/album/{id}` and that the change is
//! intentional); playlist / artist / label stay on the web player at
//! `play.qobuz.com`. Normalising all five onto one host breaks parity.
//!
//! **Why the clipboard lives in Rust here.** cxx-qt-lib 0.7 exposes no
//! QClipboard binding, so QML's own idiom (an inactive `Loader` holding an
//! invisible `TextEdit`, `selectAll()` + `copy()` — `qml/rows/TrackRow.qml`)
//! is the only QML route; but only Rust can publish `QbzShell.toastJson`
//! (`toast_qt.rs`), and the reference arm raises a toast. Splitting the two
//! halves across the boundary would need a second invokable for no gain, so
//! the whole action is one Rust seam. The four existing QML clipboard sites
//! keep their idiom — this does not replace them.

// The five builders are ported as a set on purpose: they are the reference's
// complete URL surface and the album / track / label / playlist Share arms
// will reuse them verbatim when those menus are wired. Keeping the set intact
// (rather than growing it one host at a time) is what stops a later arm from
// re-deriving a URL and reintroducing the `open.`/`play.` mix-up that
// `qml/views/LabelView.qml` already shipped once. Same allowance the toast
// publisher takes for its unused kinds (`toast_qt.rs:27`).
#![allow(dead_code)]

/// Canonical Qobuz track URL — the `open.qobuz.com` share form (#514).
/// `share.rs:4-7`.
pub(crate) fn qobuz_track_url(track_id: &str) -> String {
    format!("https://open.qobuz.com/track/{track_id}")
}

/// Qobuz web-player playlist URL (matches Tauri's share-playlist link).
/// `share.rs:9-12`.
pub(crate) fn qobuz_playlist_url(playlist_id: &str) -> String {
    format!("https://play.qobuz.com/playlist/{playlist_id}")
}

/// Qobuz album URL — the `open.qobuz.com` form (#514; Tauri's
/// `shareAlbumQobuzLink` used `https://play.qobuz.com/album/{id}`). Also
/// the source URL fed to Song.link for the album-level "Album.link".
/// `share.rs:14-19`.
pub(crate) fn qobuz_album_url(album_id: &str) -> String {
    format!("https://open.qobuz.com/album/{album_id}")
}

/// Qobuz web-player artist URL (header Share action). `share.rs:21-24`.
pub(crate) fn qobuz_artist_url(artist_id: &str) -> String {
    format!("https://play.qobuz.com/artist/{artist_id}")
}

/// Qobuz web-player label URL (label-page header Share action). There is no
/// Song.link/Album.link equivalent for labels — Qobuz-link only.
/// `share.rs:26-30`.
pub(crate) fn qobuz_label_url(label_id: &str) -> String {
    format!("https://play.qobuz.com/label/{label_id}")
}

/// Long-lived clipboard instance. arboard ties the offer's lifetime to the
/// LAST live `Clipboard` object: dropping it destroys the X11 selection
/// window (contents survive only when a clipboard MANAGER accepts the
/// handoff — KDE ships one, stock GNOME/XFCE/Cinnamon do not) and ends the
/// Wayland offer with the same rule. The old create-per-copy pattern
/// therefore worked on KDE and silently lost the text everywhere else
/// (HiFi-wizard copy report, #514). One instance kept alive for the whole
/// process serves the offer like any normal app.
///
/// This is the #514 fix, verbatim from `share.rs:32-41` — it is NOT
/// boilerplate to be simplified into a local. A create-per-copy port would
/// pass every test run on KDE and lose the text everywhere else.
static CLIPBOARD: std::sync::OnceLock<std::sync::Mutex<Option<arboard::Clipboard>>> =
    std::sync::OnceLock::new();

/// Copy `text` to the system clipboard. Runs on a blocking thread —
/// clipboard backends (X11/Wayland) can block. `share.rs:43-70`.
///
/// The one adaptation vs the reference: in the Slint app the arm already runs
/// inside a tokio context, whereas a cxx-qt invokable runs on the **Qt event
/// loop thread**, where a bare `tokio::task::spawn_blocking` panics ("must be
/// called from the context of a Tokio runtime"). It therefore goes through
/// `crate::spawn` (main.rs) onto the process-global runtime first — the same
/// `spawn` + `spawn_blocking` sandwich `local_ephemeral.rs`, `browse_qt.rs`
/// and `ambient_qt.rs` already use.
///
/// Fire-and-forget by design: the caller never learns whether the copy
/// worked, which is why the toast at the call site is unconditional
/// (`main.rs:12758-12761` does the same).
pub(crate) fn copy_to_clipboard(text: String) {
    crate::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || {
            let cell = CLIPBOARD.get_or_init(|| std::sync::Mutex::new(None));
            let mut guard = match cell.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            if guard.is_none() {
                match arboard::Clipboard::new() {
                    Ok(c) => *guard = Some(c),
                    Err(e) => {
                        log::warn!("[qbz-qt] clipboard unavailable: {e}");
                        return;
                    }
                }
            }
            if let Some(clipboard) = guard.as_mut() {
                if let Err(e) = clipboard.set_text(text) {
                    log::warn!("[qbz-qt] clipboard set failed: {e}");
                    // Drop the instance so the next copy reconnects — the
                    // display connection may have gone away.
                    *guard = None;
                }
            }
        })
        .await;
    });
}

/// Artist header ⋯ → Share. `artist/ArtistPageView.slint:530-538` fires
/// `media-action("artist", ArtistState.id, "share")`; the arm at
/// `crates/qbz/src/main.rs:12749-12763` copies `share::qobuz_artist_url(&id)`
/// and raises the "Link copied" success toast.
///
/// Link-ONLY: there is no Song.link/Odesli path for artists (see the module
/// header), so there is no "Fetching…" info toast either — the reference
/// shows one only for the track/album Song.link arms.
///
/// The empty-id guard is the reference's: `main.rs:12755` skips both the copy
/// and the toast when the id is empty, so a header that has not resolved yet
/// stays silent instead of copying `.../artist/`.
pub(crate) fn share_artist(artist_id: String) {
    if artist_id.is_empty() {
        return;
    }
    copy_to_clipboard(qobuz_artist_url(&artist_id));
    // "Link copied" is an EXISTING msgid, present in all eight catalogues
    // (crates/qbz-ui/translations/*/LC_MESSAGES/qbz-ui.po) — no new string.
    crate::toast_qt::success(qbz_i18n::t("Link copied"));
}

/// Label header ⋯ → Share. Same shape as `share_artist`, and the reference is
/// the same `media-action` table: `crates/qbz/src/main.rs:13164-13178` copies
/// `share::qobuz_label_url(&label_id)` and raises the "Link copied" success
/// toast. Link-only — the reference's own comment there says there is no
/// Song.link/Album.link equivalent for labels.
///
/// THIS REPLACES A BROKEN LINK, not just a missing toast. `LabelView.qml`
/// copied `https://open.qobuz.com/label/{id}` through a QML `TextEdit`, and
/// `open.qobuz.com` has NO `/label/` route — the copied URL simply did not
/// resolve. The host is per-ENTITY, not per-app (#514): track and album live
/// on `open.qobuz.com`, artist / playlist / label on `play.qobuz.com`. Owner-
/// confirmed 2026-08-02 with a working example, `https://play.qobuz.com/label/
/// 68307`. That asymmetry is why the five builders above are ported as a set:
/// a call site that formats its own URL gets this wrong, and did.
pub(crate) fn share_label(label_id: String) {
    if label_id.is_empty() {
        return;
    }
    copy_to_clipboard(qobuz_label_url(&label_id));
    crate::toast_qt::success(qbz_i18n::t("Link copied"));
}
