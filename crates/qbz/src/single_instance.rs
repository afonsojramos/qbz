//! Single-instance guard (issues #544/#559 — Tauri parity: the old app
//! shipped tauri-plugin-single-instance; the Slint rebuild lost it, so
//! every click on a pinned taskbar shortcut / launcher entry spawned
//! another full player — reported on both Hyprland and KDE).
//!
//! The first instance takes ownership of the well-known session-bus name
//! `com.blitzfc.qbz` (Flatpak auto-grants owning the app-id name — no
//! finish-args change needed) and exports a `com.blitzfc.qbz.SingleInstance`
//! interface with `Present()` and `OpenUrl(url)` methods. A second launch
//! sees the name taken and calls `OpenUrl(url)` when its own argv carried a
//! Qobuz deep link (the primary presents itself AND navigates — Tauri
//! parity, the piece #618 didn't port) or `Present()` otherwise — which
//! raises whichever window is current (the mini when the miniplayer is
//! open, else the main window) and works from process start, login screen
//! included — and exits. If the primary predates the interface (≤2.0.x) the
//! call errors and the second launch falls back to the MPRIS `Raise`
//! method, which only exists after session entry. `OpenUrl` failing on an
//! older primary (interface present, method missing) falls back to bare
//! `Present()`. Any D-Bus problem — no session bus, weird sandbox — falls
//! through as "we are primary": the guard must never block startup.
//!
//! Blocking zbus API on purpose: this runs once on the main thread before
//! the UI exists, and the async-io executor self-drives the connection
//! from any context (the zbus 5 "tokio" feature is FORBIDDEN graph-wide —
//! see the rfd/ksni comments in Cargo.toml).
#![cfg(target_os = "linux")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use zbus::blocking::fdo::DBusProxy;
use zbus::blocking::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::names::WellKnownName;

use crate::AppWindow;

const BUS_NAME: &str = "com.blitzfc.qbz";
const OBJECT_PATH: &str = "/com/blitzfc/qbz";
const IFACE_NAME: &str = "com.blitzfc.qbz.SingleInstance";

/// Keeps the acquired name owned for the process lifetime (releasing it
/// would let a second launch believe it is primary).
static CONN: std::sync::OnceLock<Connection> = std::sync::OnceLock::new();

/// The main window, published by `bind_window` right after `AppWindow::new()`
/// so `Present()` can raise it. `slint::Weak` is Send+Sync; upgrades happen
/// on the event loop (`tray::present` hops there itself).
static MAIN_WEAK: OnceLock<slint::Weak<AppWindow>> = OnceLock::new();

/// A `Present()` arrived before the window existed (simultaneous cold starts:
/// the DoNotQueue loser can call in while the winner is still initializing).
/// Drained once by `bind_window`.
static PENDING_PRESENT: AtomicBool = AtomicBool::new(false);

/// D-Bus surface the primary exports for activation, independent of MPRIS
/// (which only registers after session entry — a second launch while the
/// primary sits at the login window must still raise it).
struct SingleInstanceIface;

/// Shared Present path for the iface methods: raise whichever window is
/// current, or remember the request until `bind_window` (simultaneous cold
/// starts: the DoNotQueue loser can call in while the winner is still
/// initializing).
fn present_or_defer() {
    match MAIN_WEAK.get() {
        Some(weak) => crate::tray::present(weak),
        None => PENDING_PRESENT.store(true, Ordering::SeqCst),
    }
}

#[zbus::interface(name = "com.blitzfc.qbz.SingleInstance")]
impl SingleInstanceIface {
    /// Raise whichever window is current (mini when the miniplayer is open,
    /// else the main window). Runs on a zbus executor thread — never touch
    /// Slint state here; `tray::present` routes through the event loop.
    fn present(&self) {
        present_or_defer();
    }

    /// A second launch carrying a Qobuz deep link forwards it here: stash
    /// the URL, present ourselves, and dispatch it through the running
    /// instance's Ctrl+L link-resolver flow (`deep_link::drain_pending`).
    /// With no shell up (sitting at the login screen, or offline) the URL
    /// stays pending for the next successful `enter_shell`.
    fn open_url(&self, url: &str) {
        crate::deep_link::stash(url.to_string());
        present_or_defer();
        crate::deep_link::drain_pending();
    }
}

/// Publish the main window to the `Present()` handler. Call right after
/// `AppWindow::new()`; drains a Present that landed before the window existed.
pub fn bind_window(weak: slint::Weak<AppWindow>) {
    let _ = MAIN_WEAK.set(weak);
    if PENDING_PRESENT.swap(false, Ordering::SeqCst) {
        if let Some(weak) = MAIN_WEAK.get() {
            crate::tray::present(weak);
        }
    }
}

/// True = we are the primary instance (name acquired, or D-Bus unusable).
/// False = another instance owns the name; it has been asked to raise its
/// window and the caller should exit.
pub fn acquire_or_raise() -> bool {
    match probe() {
        Ok(primary) => primary,
        Err(e) => {
            log::warn!(
                "[qbz-slint] single-instance: D-Bus probe failed ({e}); continuing as primary"
            );
            true
        }
    }
}

fn probe() -> zbus::Result<bool> {
    let conn = Connection::session()?;
    // Export the Present interface BEFORE requesting the name: the moment a
    // second launch sees Exists, the object must already be callable (no
    // window where the name is owned but Present() isn't served yet).
    conn.object_server().at(OBJECT_PATH, SingleInstanceIface)?;
    let proxy = DBusProxy::new(&conn)?;
    let name: WellKnownName<'_> = BUS_NAME.try_into().map_err(zbus::Error::from)?;
    match proxy.request_name(name, RequestNameFlags::DoNotQueue.into())? {
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => {
            let _ = CONN.set(conn);
            Ok(true)
        }
        // Exists (or the DO_NOT_QUEUE-unreachable InQueue): another instance
        // runs. Ask it to present itself; both calls are best-effort — the
        // duplicate still must not start.
        RequestNameReply::Exists | RequestNameReply::InQueue => {
            // Carrying a deep link from argv (captured at process start)?
            // Forward it: OpenUrl makes the primary present itself AND
            // navigate. An older primary without the method errors → the
            // bare-Present ladder below (version-skew tolerance); the URL
            // is then lost with this exiting process, same as a failed emit
            // in the Tauri era.
            let forwarded = match crate::deep_link::take_pending() {
                Some(url) => conn
                    .call_method(Some(BUS_NAME), OBJECT_PATH, Some(IFACE_NAME), "OpenUrl", &url)
                    .is_ok(),
                None => false,
            };
            let presented = forwarded
                || conn
                    .call_method(Some(BUS_NAME), OBJECT_PATH, Some(IFACE_NAME), "Present", &())
                    .is_ok();
            if !presented {
                // Older primary (≤2.0.x) without the SingleInstance interface:
                // fall back to MPRIS Raise. Full MPRIS name =
                // "org.mpris.MediaPlayer2." + BUS_SUFFIX, and
                // qbz-media-controls registers with BUS_SUFFIX = the app id
                // (linux.rs), NOT "qbz".
                let _ = conn.call_method(
                    Some("org.mpris.MediaPlayer2.com.blitzfc.qbz"),
                    "/org/mpris/MediaPlayer2",
                    Some("org.mpris.MediaPlayer2"),
                    "Raise",
                    &(),
                );
            }
            Ok(false)
        }
    }
}
