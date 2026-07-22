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

use objc2_app_kit::{NSView, NSWindowButton};
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
    let slint_handle = window.window_handle();
    let Ok(handle) = slint_handle.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(kit) = handle.as_raw() else {
        return;
    };
    // SAFETY: `ns_view` is the live content view of the just-shown main
    // window (raw-window-handle 0.6 exposes only the view; the NSWindow is
    // reached via `-[NSView window]`), dereferenced on the main thread; the
    // window outlives this call. Every AppKit call below is main-thread-only
    // view geometry.
    unsafe {
        let ns_view: &NSView = &*(kit.ns_view.as_ptr() as *const NSView);
        let Some(ns_window) = ns_view.window() else {
            return;
        };
        let Some(close) =
            ns_window.standardWindowButton(NSWindowButton::NSWindowCloseButton)
        else {
            return;
        };
        let win_h = ns_window.frame().size.height;
        // Close-button centre in window base coordinates (origin bottom-left,
        // +y up), measured from the button's own BOUNDS — its frame lives in
        // the superview's space and double-counts the in-container offset
        // (2026-07-22 regression: measuring frame + assuming a bottom-left
        // superview shifted the lights UP instead of down).
        let measure_centre_from_top = |btn: &NSView| -> f64 {
            let r = btn.convertRect_toView(btn.bounds(), None);
            win_h - (r.origin.y + r.size.height / 2.0)
        };
        let before = measure_centre_from_top(&close);
        let move_down = HEADER_CENTRE_PT - before; // visual pts, +ve = down
        if move_down.abs() < 0.5 {
            return; // already centred (idempotent re-entry)
        }
        let Some(container) = close.superview() else {
            return;
        };
        // The container's frame is in ITS superview's coordinate space, and
        // AppKit flips the titlebar hierarchy (+y down when flipped).
        let parent_flipped = container
            .superview()
            .map(|sv| sv.isFlipped())
            .unwrap_or(false);
        let original = container.frame();
        let mut frame = original;
        frame.origin.y += if parent_flipped { move_down } else { -move_down };
        container.setFrame(frame);
        // Re-measure: if the shift landed further from target than where we
        // started (coordinate-model surprise), undo it — stock placement is
        // the acceptable fallback, a wrongly shifted one is not.
        let after = measure_centre_from_top(&close);
        if (after - HEADER_CENTRE_PT).abs() > (before - HEADER_CENTRE_PT).abs() {
            container.setFrame(original);
            log::warn!(
                "[macos-chrome] traffic-light shift reverted: centre {before:.1}pt -> {after:.1}pt from top (target {HEADER_CENTRE_PT}pt, parent flipped: {parent_flipped})"
            );
            return;
        }
        log::info!(
            "[macos-chrome] traffic lights centred: {before:.1}pt -> {after:.1}pt from top (target {HEADER_CENTRE_PT}pt, parent flipped: {parent_flipped})"
        );
    }
}
