//! Linux single-instance guard and warm deep-link forwarding.
//!
//! The primary owns `com.blitzfc.qbz` on the session bus. A later launch asks
//! it to present its current window, forwarding a launcher URL when present,
//! and exits. D-Bus failure is deliberately fail-open so startup is never
//! blocked by a broken or absent session bus.
#![cfg(target_os = "linux")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use zbus::blocking::fdo::DBusProxy;
use zbus::blocking::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::names::WellKnownName;

const BUS_NAME: &str = "com.blitzfc.qbz";
const OBJECT_PATH: &str = "/com/blitzfc/qbz";
const IFACE_NAME: &str = "com.blitzfc.qbz.SingleInstance";

static CONN: OnceLock<Connection> = OnceLock::new();
static UI_READY: AtomicBool = AtomicBool::new(false);
static PENDING_PRESENT: AtomicBool = AtomicBool::new(false);

struct SingleInstanceIface;

fn present_or_defer() {
    if UI_READY.load(Ordering::SeqCst) {
        crate::tray_qt::present();
    } else {
        PENDING_PRESENT.store(true, Ordering::SeqCst);
    }
}

#[zbus::interface(name = "com.blitzfc.qbz.SingleInstance")]
impl SingleInstanceIface {
    fn present(&self) {
        present_or_defer();
    }

    fn open_url(&self, url: &str) {
        crate::deep_link_qt::stash(url.to_string());
        present_or_defer();
        crate::deep_link_qt::drain_pending();
    }
}

/// Called once QbzTray has registered its Qt-thread hop. It is independent of
/// whether the optional tray icon itself is enabled.
pub(crate) fn bind_ui() {
    UI_READY.store(true, Ordering::SeqCst);
    if PENDING_PRESENT.swap(false, Ordering::SeqCst) {
        crate::tray_qt::present();
    }
}

/// True when this process should continue as the primary instance.
pub(crate) fn acquire_or_raise() -> bool {
    match probe() {
        Ok(primary) => primary,
        Err(e) => {
            log::warn!("[qbz-qt] single-instance probe failed ({e}); continuing");
            true
        }
    }
}

fn probe() -> zbus::Result<bool> {
    let conn = Connection::session()?;
    conn.object_server().at(OBJECT_PATH, SingleInstanceIface)?;
    let proxy = DBusProxy::new(&conn)?;
    let name: WellKnownName<'_> = BUS_NAME.try_into().map_err(zbus::Error::from)?;
    match proxy.request_name(name, RequestNameFlags::DoNotQueue.into())? {
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => {
            let _ = CONN.set(conn);
            Ok(true)
        }
        RequestNameReply::Exists | RequestNameReply::InQueue => {
            let forwarded = match crate::deep_link_qt::take_pending() {
                Some(url) => conn
                    .call_method(
                        Some(BUS_NAME),
                        OBJECT_PATH,
                        Some(IFACE_NAME),
                        "OpenUrl",
                        &url,
                    )
                    .is_ok(),
                None => false,
            };
            let presented = forwarded
                || conn
                    .call_method(
                        Some(BUS_NAME),
                        OBJECT_PATH,
                        Some(IFACE_NAME),
                        "Present",
                        &(),
                    )
                    .is_ok();
            if !presented {
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
