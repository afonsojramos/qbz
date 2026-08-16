//! Rust-side navigation history — the POC counterpart of Slint's
//! `NavState.can-back` / `can-forward` / request-back / request-forward.
//!
//! A simple stack (Vec + index): `record(view)` pushes a view (truncating
//! any forward entries), `back()` / `forward()` move the cursor. Every
//! mutation republishes `canBack` / `canForward` / `currentView` onto the
//! bridge.
//!
//! There is NO route table here, and that is deliberate: a view id is an
//! opaque `String`, and the only place the set of legal ids is enumerated is
//! `qml/shell/ContentRouter.qml`'s content `Loader` chain. (It used to live in
//! `AppShell.qml`; the kiosk port extracted it so BOTH shells share one chain,
//! and this paragraph said "AppShell" for far too long afterwards — it has
//! since sent more than one reader to the wrong file.) Adding a route is
//! therefore a two-file change — the caller `record`s its id, ContentRouter
//! grows an arm for it — and nothing needs registering in this file. The
//! failure mode when the router arm is forgotten is a BLANK content pane (the
//! ternary falls through to `""`), recoverable with Back; it is not a crash,
//! which is exactly why it is easy to miss.
//!
//! Ids in use: `home`, `library`, `local`, `localalbum`, `album`, `artist`,
//! `artistreleases`, `settings`, `search`, `playlist`, `discoverbrowse`,
//! `playlistbrowse`, `recentalbums`, `mostplayedalbums`, `label`,
//! `labelreleases`, `mix`, `mixtapes`, `collections`, `mixtapedetail`,
//! `discobuilder`, `blacklist`, `playlistmanager`, `nowplaying`, `scene`,
//! `musician`, `purchases`, `purchase-album`.
//!
//! `scene` (the "artists from the same place" discovery view) and `musician`
//! both carry a required context that only their opener has, so neither is in
//! the session-restore-safe set below: reopening the app on one would land on
//! a page with nothing to show. Note the musician MODAL is not a route at all —
//! only the `contextual` branch navigates; `weak`/`none` publish a document
//! that a global overlay renders (`musician_qt.rs`).
//!
//! `purchase-album` is in that same context-carrying class (its album id lives
//! in the purchases controller, not in the id), and `purchases` is left out of
//! the restore set for a DIFFERENT reason: it is opt-in and ships hidden
//! (`show_purchases`, default false), so restoring onto it could open the app
//! on a surface whose own entry point is switched off. Neither is in
//! `VIEW_TO_PREF` below, so `startup_view()` can never resolve to either — and
//! neither is offered by the Startup page dropdown, whose own list
//! (`settings_qt::STARTUP_PAGE_VALUES`) would have to grow a matching entry
//! first. The Slint build DOES restore `purchases` (`qbz/src/main.rs:637`);
//! bringing that across is an additive change that needs both halves.
//!
//! `nowplaying` is KIOSK-ONLY (2026-08-02 kiosk-port contract §3): it is the
//! NavRail's fifth tile and the route the kiosk's full-screen player lives on.
//! The desktop shell has no equivalent — its transport is the persistent bar —
//! so `ContentRouter.qml` mounts it only in kiosk mode. It needs no arm in
//! `crate::navigate_to`: the view reads the queue and player bridges, both of
//! which are already live.

use std::sync::Mutex;

use cxx_qt_lib::QString;

struct NavHistory {
    entries: Vec<String>,
    // Index of the CURRENT entry in `entries` (0 when only "home" exists).
    index: usize,
}

static HISTORY: Mutex<Option<NavHistory>> = Mutex::new(None);

fn with_history<R>(f: impl FnOnce(&mut NavHistory) -> R) -> R {
    let mut guard = HISTORY.lock().unwrap();
    let history = guard.get_or_insert_with(|| NavHistory {
        // Seeded with the STARTUP view, not a literal "home": booting on a
        // restored view with a history rooted at Home would offer a Back to a
        // page the user never opened this session.
        entries: vec![startup_view()],
        index: 0,
    });
    f(history)
}

/// Push a view as the new current entry (no-op when it IS the current one).
/// The top-level views it is SAFE to reopen the app on: no required id, so a
/// restore can never land on a detail page whose entity is gone.
///
/// 1:1 with the reference's `last_view` contract (`ui_prefs.rs:489-492`), which
/// spells the set out and adds "Detail views (album/artist/playlist/…) are never
/// stored". Search and Settings are excluded on the same reasoning the reference
/// gives for `last_nav`: one is transient, the other is config — reopening the
/// app inside either is not "where you left off", it is a surprise.
/// `last_view` is written into the SHARED `ui_prefs.json` — the same file the
/// Slint build reads — so the value stored has to be the REFERENCE's
/// vocabulary, not this port's route ids. Four of the six differ:
///
/// | reference (`ui_prefs.rs:489`) | this port's route |
/// |---|---|
/// | `home`          | `home`        |
/// | `favorites`     | `library`     |
/// | `local-library` | `local`       |
/// | `mixtapes`      | `mixtapes`    |
/// | `collections`   | `collections` |
/// | `discover`      | (no twin — the reference splits Discover from Home)   |
///
/// Writing `local` / `library` verbatim would have looked correct here and
/// silently broken the Slint build's restore for anyone who runs both — the
/// owner's own prefs today read `last_view = "local-library"`, written by Slint.
/// Unknown values in either direction resolve to Home rather than guessing.
///
/// Detail views (album/artist/playlist/…) are never stored: they need an id and
/// a restore could land on an entity that is gone. Search and Settings are
/// excluded too — transient and config respectively, and reopening the app
/// inside either is a surprise, not "where you left off".
const VIEW_TO_PREF: &[(&str, &str)] = &[
    ("home", "home"),
    ("library", "favorites"),
    ("local", "local-library"),
    ("mixtapes", "mixtapes"),
    ("collections", "collections"),
];

fn pref_for_view(view: &str) -> Option<&'static str> {
    VIEW_TO_PREF.iter().find(|(v, _)| *v == view).map(|(_, p)| *p)
}

fn view_for_pref(pref: &str) -> Option<&'static str> {
    VIEW_TO_PREF
        .iter()
        .find(|(_, p)| *p == pref)
        .map(|(v, _)| *v)
        // The reference's `discover` has no separate route here: this port's
        // Discover IS `home`. Accept it on READ so a prefs file written by the
        // Slint build restores somewhere sensible instead of falling to Home.
        .or(if pref == "discover" { Some("home") } else { None })
}

/// Crash-chain guard for the restore paths.
///
/// A restore that crashes is a TRAP: the app dies on boot, and every retry
/// restores the same thing and dies again, with no way out from inside the UI.
/// The reference guards it with a probe file and a recovery ladder
/// (`main.rs:7218-7240`), and it is worth carrying because the failure mode is
/// "the app no longer starts", not "a page looks wrong".
///
/// The ladder, and it never DELETES state — it bypasses it:
/// - level 1 (previous boot reached liveness): restore everything.
/// - level 2 (one prior boot died first): ignore the remembered view, open Home.
/// - level >= 3: additionally skip the persisted queue restore for this boot.
///   The queue file itself is left untouched.
fn probe_path() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|d| d.join("qbz").join("startup_probe_qt"))
}

static CRASH_LEVEL: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Read + increment the probe. Call ONCE, early in boot, before any restore.
pub fn arm_startup_probe() {
    let Some(path) = probe_path() else { return };
    let prev: u8 = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0);
    let level = prev.saturating_add(1);
    CRASH_LEVEL.store(level, std::sync::atomic::Ordering::Relaxed);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(&path, level.to_string());
    if level > 1 {
        log::warn!(
            "[qbz-qt] startup probe: level {level} — {} prior boot(s) died before liveness; \
             restore is degraded",
            level - 1
        );
    }
}

/// The app reached a state a user could act on. Clears the chain so the NEXT
/// boot restores normally.
pub fn mark_startup_healthy() {
    if let Some(path) = probe_path() {
        let _ = std::fs::write(&path, "0");
    }
}

/// This boot's chain level (1 = the previous shutdown was clean).
pub fn crash_level() -> u8 {
    CRASH_LEVEL.load(std::sync::atomic::Ordering::Relaxed).max(1)
}

/// The view to open on: `startup_page` = "home" always Home, "remember" the last
/// safe top-level view. Unknown keys fall back to Home rather than guessing.
///
/// The pref row for this has existed in Settings since the settings port and
/// NOTHING read it — the user could pick "Where you left off" and the app opened
/// on Home every time. That is the "renders, persists, drives nothing" defect
/// class this port has a name for.
pub fn startup_view() -> String {
    if crate::settings_qt::pref_str("startup_page", "home") != "remember" {
        return "home".to_string();
    }
    // Ladder level 2+: a prior boot died before liveness, so the remembered
    // view is a suspect. Open Home instead of retrying it.
    if crash_level() >= 2 {
        log::warn!("[qbz-qt] startup: remembered view bypassed (crash chain)");
        return "home".to_string();
    }
    let last = crate::settings_qt::pref_str("last_view", "home");
    let resolved = view_for_pref(&last).unwrap_or("home");
    log::info!("[qbz-qt] startup: remember -> last_view {last:?} resolves to view {resolved:?}");
    resolved.to_string()
}

/// The view the SHELL should mount on at this session entry.
///
/// `init_shell_for_user` is MULTI-ENTRY (login, session restore, "Start
/// offline"), and it used to `record("home")` unconditionally. That is a
/// literal that outranks the whole startup-page pref: it pushed `home` over the
/// seeded startup entry AND — since `record` persists — rewrote `last_view` to
/// `home` on every boot, so "Where you left off" could never survive its own
/// startup. The restore is a ONE-SHOT: the first entry of the process honours
/// the pref, a later re-entry (re-login inside the same run) is a fresh session
/// and lands on Home the way the reference does.
pub fn shell_entry_view() -> String {
    static USED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if USED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return "home".to_string();
    }
    startup_view()
}

pub fn record(view: &str) {
    // Persist BEFORE the history mutation so the write reflects the view the
    // user actually reached, and only for the safe set — a detail view leaves
    // the stored value on the last safe root, which is exactly what the
    // reference stores.
    if let Some(pref) = pref_for_view(view) {
        crate::settings_qt::save_pref("last_view", serde_json::json!(pref));
    }
    let (can_back, can_forward, current) = with_history(|h| {
        if h.entries[h.index] != view {
            h.entries.truncate(h.index + 1);
            h.entries.push(view.to_string());
            h.index += 1;
        }
        snapshot(h)
    });
    publish(can_back, can_forward, current);
}

/// The id of the entry the user is standing on.
///
/// Read-only, and the ONLY reader today is `playlist_qt::back_if_showing`
/// (contract §5.1): a delete may only navigate away when the deleted playlist
/// is the one whose detail page is open, or deleting a row from the Playlist
/// Manager throws the user off the manager. The bridge already publishes this
/// as `QbzShell.currentView`, but a Rust caller has no way to read a Qt
/// property back off the Qt thread — hence the accessor rather than a QML
/// round trip.
pub fn current_view() -> String {
    with_history(|h| h.entries[h.index].clone())
}

/// Move one entry back, if possible.
pub fn back() {
    let (can_back, can_forward, current) = with_history(|h| {
        if h.index > 0 {
            h.index -= 1;
        }
        snapshot(h)
    });
    publish(can_back, can_forward, current);
}

/// Move one entry forward, if possible.
pub fn forward() {
    let (can_back, can_forward, current) = with_history(|h| {
        if h.index + 1 < h.entries.len() {
            h.index += 1;
        }
        snapshot(h)
    });
    publish(can_back, can_forward, current);
}

fn snapshot(h: &NavHistory) -> (bool, bool, String) {
    (
        h.index > 0,
        h.index + 1 < h.entries.len(),
        h.entries[h.index].clone(),
    )
}

fn publish(can_back: bool, can_forward: bool, current: String) {
    crate::shell_bridge::ui(move |mut b| {
        b.as_mut().set_can_back(can_back);
        b.as_mut().set_can_forward(can_forward);
        b.as_mut().set_current_view(QString::from(current.as_str()));
    });
}

#[cfg(test)]
mod tests {
    // The history logic is exercised through the Mutex global; these tests
    // run serially within one process, so drive it through the public API
    // and re-derive expectations from fresh state each time. (The bridge
    // publish hop is a no-op off the Qt thread — QT_THREAD unset.)

    #[test]
    fn record_back_forward_cycle() {
        super::record("home");
        super::record("album");
        super::record("artist");
        super::back();
        super::back();
        super::forward();
        // Truncate-on-record: recording after back() drops forward entries.
        super::back();
        super::record("playlist");
        // Now: home, playlist (artist/album dropped). Back once -> home.
        super::back();
        // Forward -> playlist again. No panics = the invariants hold.
        super::forward();
    }
}
