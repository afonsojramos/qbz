//! Launcher deep links shared by cold starts and the Linux single-instance
//! D-Bus handoff.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

static PENDING: Mutex<Option<String>> = Mutex::new(None);
static ONLINE_SESSION: AtomicBool = AtomicBool::new(false);

/// The native Qobuz URL shapes accepted by the desktop launcher files.
pub(crate) fn is_qobuz_link(arg: &str) -> bool {
    arg.starts_with("qobuzapp://")
        || arg.starts_with("https://play.qobuz.com/")
        || arg.starts_with("http://play.qobuz.com/")
        || arg.starts_with("https://open.qobuz.com/")
        || arg.starts_with("http://open.qobuz.com/")
}

fn select_link(args: &[String]) -> Option<String> {
    args.iter().find(|arg| is_qobuz_link(arg)).cloned()
}

/// Capture argv before the single-instance decision so a duplicate process
/// can forward its URL to the owner of the bus name.
pub(crate) fn capture_argv() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(url) = select_link(&args) {
        log::info!(
            "[qbz-qt] deep link: captured {}",
            url.split('?').next().unwrap_or(&url)
        );
        stash(url);
    }
}

/// Newest launch intent wins, matching the former frontend.
pub(crate) fn stash(url: String) {
    if let Ok(mut pending) = PENDING.lock() {
        *pending = Some(url);
    }
}

pub(crate) fn take_pending() -> Option<String> {
    PENDING.lock().ok().and_then(|mut pending| pending.take())
}

/// Bind/unbind the only context in which a Qobuz route can be fetched.
pub(crate) fn set_online_session(active: bool) {
    ONLINE_SESSION.store(active, Ordering::SeqCst);
    if active {
        drain_pending();
    }
}

/// Resolve the pending link once an authenticated shell is available.
pub(crate) fn drain_pending() {
    if !ONLINE_SESSION.load(Ordering::SeqCst) {
        return;
    }
    let Some(url) = take_pending() else {
        return;
    };
    log::info!(
        "[qbz-qt] deep link: resolving {}",
        url.split('?').next().unwrap_or(&url)
    );
    crate::link_resolver_qt::resolve_deep_link(url);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_launcher_qobuz_shapes() {
        assert!(is_qobuz_link("https://open.qobuz.com/album/a"));
        assert!(is_qobuz_link("http://play.qobuz.com/track/1"));
        assert!(is_qobuz_link("qobuzapp://artist/1"));
        assert!(!is_qobuz_link("https://open.qobuz.com.evil.test/album/a"));
        assert!(!is_qobuz_link("https://spotify.com/track/1"));
        assert!(!is_qobuz_link("qbz://album/a"));
    }

    #[test]
    fn first_matching_argument_wins() {
        let args = vec![
            "--flag".to_string(),
            "https://open.qobuz.com/album/first".to_string(),
            "qobuzapp://album/second".to_string(),
        ];
        assert_eq!(
            select_link(&args),
            Some("https://open.qobuz.com/album/first".to_string())
        );
    }
}
