//! macOS custom-chrome niceties that need AppKit access.
//!
//! With the overlay window attributes (titlebar_transparent + title_hidden +
//! fullsize_content_view — main.rs window-attributes hook) the NATIVE traffic
//! lights float over QBZ's own header. But AppKit parks them at the STANDARD
//! titlebar height (~28pt) while the QBZ header bar is 42px tall, so the
//! lights sat ~7pt above the header controls' vertical centre (owner ask
//! 2026-07-21: centre them, like the official Qobuz app does).
//!
//! The lights are three NSButtons inside a single container view; shifting
//! the container moves all three at once and preserves their spacing. We
//! measure the close button's centre in window coordinates (bottom-left
//! origin), compute the delta to the header's vertical centre, and shift the
//! container's frame by it. AppKit keeps the container's frame across
//! ordinary resizes (it is pinned top-left), so a one-shot after show
//! suffices. Known limitation: AppKit re-lays out the titlebar on
//! fullscreen enter/exit, where the centre can drift until the next launch.

use objc2_app_kit::{NSWindow, NSWindowButton};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// Vertical centre target in AppKit points, measured from the window's top
/// edge: half of `Layout.header-height` (42px — Slint px == AppKit points on
/// macOS). Keep in sync with qbz-ui/ui/foundation/layout.slint.
const HEADER_CENTRE_PT: f64 = 21.0;

/// Vertically centre the native traffic lights in the QBZ header bar.
/// No-op with the system title bar (AppKit owns the layout there), off the
/// winit/AppKit backend, or when the buttons are already centred.
/// Main-thread only — call right after `window.show()`.
pub fn center_traffic_lights(window: &slint::Window) {
    if crate::ui_prefs::load().use_system_title_bar {
        return;
    }
    let Ok(handle) = window.window_handle().window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(kit) = handle.as_raw() else {
        return;
    };
    // SAFETY: `ns_window` is the live NSWindow of the just-shown main window,
    // dereferenced on the main thread; the window outlives this call. Every
    // AppKit call below is main-thread-only view geometry.
    unsafe {
        let ns_window: &NSWindow = &*(kit.ns_window.as_ptr() as *const NSWindow);
        let Some(close) =
            ns_window.standardWindowButton(NSWindowButton::NSWindowCloseButton)
        else {
            return;
        };
        let win_h = ns_window.frame().size.height;
        // Close-button rect in window base coordinates (origin bottom-left).
        let in_window = close.convertRect_toView(close.frame(), None);
        let centre_from_top = win_h - (in_window.origin.y + in_window.size.height / 2.0);
        let dy = centre_from_top - HEADER_CENTRE_PT;
        if dy.abs() < 0.5 {
            return; // already centred (idempotent re-entry)
        }
        if let Some(container) = close.superview() {
            let mut frame = container.frame();
            frame.origin.y += dy;
            container.setFrame(frame);
            log::info!("[macos-chrome] traffic lights centred in header (dy={dy:.1}pt)");
        }
    }
}
