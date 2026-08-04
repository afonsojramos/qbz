//! QbzMini — miniplayer domain bridge (the immersive_bridge.rs exemplar:
//! `OnceLock<CxxQtThread>`, `ui()` no-op pre-boot, `boot()` invokable, `impl
//! Default` for construction-seeded values, a custom-WRITE property funnel with
//! an `AtomicBool` mirror beside it).
//!
//! Port of the Slint `MiniPlayerState` global per the 2026-08-03
//! miniplayer/tray contract (`qbz-nix-docs/qt-frontend/2026-08-03-miniplayer-tray-port/00-CONTRACT.md`,
//! rows A-1 … A-9). The controller half — the queue projection, the prefs and
//! the pure arithmetic — is `mini_qt.rs`.
//!
//! **There is no mirror layer and there must never be one** (contract §3.2 /
//! §13-D1). Slint needs ~155 lines of `d.set_x(s.get_x())` because its globals
//! are per-component-tree; in Qt every bridge is a `#[qml_singleton]` in the ONE
//! `QQmlApplicationEngine`, so a second `Window` reads the same `QbzPlayer` /
//! `QbzQueue` / `QbzLyrics` / `QbzShell` objects the shell reads.
//!
//! `surface` and `open` are CUSTOM WRITES. QML reaches them by assignment
//! (`QbzMini.surface = 3`), and the funnel — not the call site — owns the
//! persist, the `OPEN` mirror and the surface-3 queue refresh, so no write path
//! can skip them.

use std::pin::Pin;
use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

#[cxx_qt::bridge]
pub mod qbz_mini {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // Miniplayer-mode flag. CUSTOM WRITE (the `set_open` funnel below), so
        // every write — QML's exit paths and the Rust verbs alike — passes
        // through one setter that also stores the `OPEN` mirror (contract A-2,
        // the exact shape of immersive_bridge.rs's `open`).
        #[qproperty(bool, open, READ, WRITE = set_open, NOTIFY)]
        // 0 micro · 1 compact · 2 artwork · 3 queue · 4 lyrics — the
        // MINI_VIEW_VALUES order minus "remember" (settings_qt.rs:1077).
        // CUSTOM WRITE, and it is the ONLY compilable reading of contract A-9:
        // that row asks for `#[qinvokable] fn set_surface`, but A-3 fixes the
        // property name `surface`, whose cxx-qt-generated setter is ALSO
        // `set_surface`/`setSurface` — one QObject cannot carry both. The Rust
        // signature is byte-identical to A-9's; only the attribute differs, and
        // routing it through the property is strictly stronger because a plain
        // assignment cannot bypass the persist. QML therefore writes
        // `QbzMini.surface = n` and MUST NOT call `QbzMini.setSurface(n)` —
        // that member does not exist on the object.
        #[qproperty(i32, surface, READ, WRITE = set_surface, NOTIFY)]
        // The static artwork-derived card background (`mini_background_blur`).
        // Written by `toggle_background_blur` (contract A-10).
        #[qproperty(bool, background_blur)]
        // The navigable queue: the current track first, then the FULL
        // unfiltered upcoming list. Built by `mini_qt::mini_queue_doc` and
        // published from the ONE queue funnel (`queue_qt::publish`), never from
        // a second fetch (§7-M1). Full-shape default, never "{}" (trap 15).
        #[qproperty(QString, mini_queue_json)]
        // THE PERSISTED EXPANDED GEOMETRY, exposed to QML (contract A-6b).
        // The `Window` is QML-created and Rust cannot reach a QQuickWindow, so
        // QML owns the sizing and needs the numbers; `save_geometry` writes the
        // clamped pair back here, so a condensed->expanded switch inside one
        // session restores the size the user last dragged without a re-read.
        // Seeded at CONSTRUCTION, not at boot(): the size expression is a
        // binding and the first frame is already too late for a post-boot push.
        #[qproperty(f32, mini_width)]
        #[qproperty(f32, mini_height)]

        type QbzMini = super::QbzMiniRust;

        /// Registers this object's Qt-thread hop (Main.qml boots EVERY domain
        /// singleton; only QbzSession.boot also fires crate::on_boot).
        /// **Without it every `mini_bridge::ui()` is a SILENT no-op** — the
        /// bridge never errors, it just never repaints.
        #[qinvokable]
        fn boot(self: Pin<&mut QbzMini>);
    }

    // The custom WRITE targets. NOT qinvokables — QML reaches them by writing
    // `QbzMini.open` / `QbzMini.surface`, never by name. (cxx-qt custom-setter
    // pattern: the property's generated setter is replaced by this method,
    // which owns the store + notify + the side effects.)
    unsafe extern "RustQt" {
        fn set_open(self: Pin<&mut QbzMini>, value: bool);
        fn set_surface(self: Pin<&mut QbzMini>, surface: i32);
    }

    impl cxx_qt::Threading for QbzMini {}
}

use qbz_mini::QbzMini;

/// Rust side of the mini bridge (plain storage, phase-1 pattern).
pub struct QbzMiniRust {
    open: bool,
    surface: i32,
    background_blur: bool,
    mini_queue_json: QString,
    mini_width: f32,
    mini_height: f32,
}

impl Default for QbzMiniRust {
    fn default() -> Self {
        // Read here, not in boot(): QML singletons instantiate lazily and the
        // window's size + surface bindings evaluate on its first frame, which
        // can precede QbzMini.boot(). A post-boot seed would leave the first
        // frame decided against cxx-qt's derived zeroes (trap 31).
        Self {
            open: false,
            surface: crate::mini_qt::initial_surface(),
            background_blur: crate::mini_qt::initial_background_blur(),
            mini_queue_json: QString::from(crate::mini_qt::MINI_QUEUE_EMPTY),
            mini_width: crate::mini_qt::initial_width(),
            mini_height: crate::mini_qt::initial_height(),
        }
    }
}

// ---------------------------------------------------------------------------
// The UI hop (immersive_bridge.rs pattern)
// ---------------------------------------------------------------------------

static QT_THREAD: OnceLock<CxxQtThread<QbzMini>> = OnceLock::new();

/// Queue a mini-bridge mutation onto the Qt event loop (no-op before boot
/// registers the thread).
pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzMini>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}

/// Rust-side mirror of the `open` property, written inside `apply_open` — the
/// funnel EVERY write passes through — so it is exact. Read by callers that run
/// on ANOTHER singleton or on a worker thread and therefore cannot reach this
/// QObject's properties: the hotkeys pipeline's mini/main context split and the
/// surface-3 queue refresh below (contract A-2).
static OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn is_open() -> bool {
    OPEN.load(std::sync::atomic::Ordering::SeqCst)
}

/// The single funnel behind every `open` write: store, mirror, THEN notify.
///
/// The mirror is stored BEFORE the notify on purpose. `open_changed()` emits
/// synchronously, so any QML `onOpenChanged` handler runs inside that call —
/// and anything it reaches that asks `is_open()` (the queue refresh below does)
/// would read the stale value if the store came last. `immersive_bridge.rs`'s
/// `apply_open` has the same two lines in the other order; the window there is
/// currently unreachable, so it is left alone rather than widened into this
/// commit.
fn apply_open(mut this: Pin<&mut QbzMini>, value: bool) {
    use cxx_qt::CxxQtType as _;
    if this.open == value {
        return;
    }
    this.as_mut().rust_mut().open = value;
    OPEN.store(value, std::sync::atomic::Ordering::SeqCst);
    this.as_mut().open_changed();
}

/// The single funnel behind every `surface` write: store, notify, persist, and
/// — on the queue surface — kick the queue funnel.
///
/// The persist and the refresh run on EVERY write, including a same-value one,
/// which is 1:1 with the reference (`miniplayer.rs:302-315` has no equality
/// guard: it saves the pref and re-kicks `spawn_queue_refresh` unconditionally).
/// Only the change NOTIFICATION is suppressed when nothing moved, which is
/// ordinary QML property semantics and not a behaviour the reference has an
/// analogue for. Re-asserting surface 3 therefore still refreshes the queue.
fn apply_surface(mut this: Pin<&mut QbzMini>, surface: i32) {
    use cxx_qt::CxxQtType as _;
    if this.surface != surface {
        this.as_mut().rust_mut().surface = surface;
        this.as_mut().surface_changed();
    }
    crate::mini_qt::save_surface(surface);
    if surface == 3 {
        refresh_queue();
    }
}

/// Port of `miniplayer.rs`'s `spawn_queue_refresh` (`:631-641`): gated on the
/// mini being open, and routed through THE queue funnel — `queue_qt::publish`,
/// the one place that reads queue state — never through a second fetch of its
/// own (§7-M1). The funnel's identical-document guards make it a no-op when
/// nothing moved, so the cost of a redundant call is one build, no Qt hop.
fn refresh_queue() {
    if !is_open() {
        return;
    }
    let runtime = crate::app();
    crate::spawn(async move { crate::queue_qt::publish(&runtime).await });
}

impl qbz_mini::QbzMini {
    pub fn boot(self: Pin<&mut Self>) {
        if QT_THREAD.set(self.qt_thread()).is_err() {
            log::warn!("[qbz-qt] mini Qt thread already registered");
        }
    }

    /// The custom WRITE target of the `open` property.
    pub fn set_open(self: Pin<&mut Self>, value: bool) {
        apply_open(self, value);
    }

    /// The custom WRITE target of the `surface` property (contract A-9):
    /// sets the property, persists `mini_surface`, and refreshes the queue on
    /// surface 3.
    pub fn set_surface(self: Pin<&mut Self>, surface: i32) {
        apply_surface(self, surface);
    }
}
