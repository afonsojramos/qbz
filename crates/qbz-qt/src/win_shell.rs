//! Windows shell helpers backed by `cxx/win_shell.cpp` (the `font_qt.rs`
//! pattern: a thin `extern "C"` seam, one safety note per call).
//!
//! The C++ half compiles on every platform, so this module does too. What is
//! Windows-specific is what the callers do with the handle.

use std::ffi::c_void;
use std::ptr::NonNull;

unsafe extern "C" {
    fn qbz_main_window_hwnd() -> *mut c_void;
    fn qbz_install_commit_data_handler(cb: extern "C" fn());
}

/// The top-level window's native handle, or `None` before it is shown.
///
/// GUI THREAD ONLY, and never before `exec()`: `QWindow::winId()` creates the
/// platform window as a side effect, so calling it early both returns a handle
/// for a half-built window and changes the handle the real one ends up with.
#[allow(dead_code)] // consumed by Task 2 (SMTC) and Task 5 (tray)
pub(crate) fn main_window_hwnd() -> Option<NonNull<c_void>> {
    // SAFETY: takes no arguments and only reads QGuiApplication state. The
    // caller guarantees the GUI thread and a live QGuiApplication.
    NonNull::new(unsafe { qbz_main_window_hwnd() })
}

/// Run `cb` when Windows asks whether the session may end (WM_QUERYENDSESSION).
///
/// Qt blocks inside the signal until the handler returns, so `cb` must do its
/// work synchronously -- that is the point of the hook, and the reason a
/// queued connection would be wrong here.
#[allow(dead_code)] // consumed by Task 8 (logoff persistence)
pub(crate) fn install_commit_data_handler(cb: extern "C" fn()) {
    // SAFETY: `qApp` exists (this runs after `QGuiApplication::new`), and the
    // lambda stored on the C++ side only calls `cb`, which is a `'static` fn
    // pointer with no captured state.
    unsafe { qbz_install_commit_data_handler(cb) }
}
