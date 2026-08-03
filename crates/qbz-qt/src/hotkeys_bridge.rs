//! QbzHotkeys — keyboard/hotkeys domain bridge (viz_bridge.rs exemplar:
//! OnceLock<CxxQtThread>, `ui()` no-op pre-boot, `boot()` invokable, `impl
//! Default` for construction-seeded values).
//!
//! The Rust half of the 2026-08-03 hotkeys-port contract
//! (`qbz-nix-docs/qt-frontend/2026-08-03-hotkeys-port/00-CONTRACT.md` v2 §3.4,
//! block B1): the ONE brain behind the AppShell QML dispatcher (route (b),
//! divergence K1 — a QML-side single dispatcher + a Rust brain, because
//! cxx-qt-lib 0.7.3 ships no QEvent/QKeyEvent/installEventFilter plumbing).
//! The pure model (the 23-action table, grammar, store, capture, groups, the
//! §1.2 Escape stack) lives in `hotkeys_qt.rs`; this file is the singleton
//! surface + the §1.1 ordered pipeline.
//!
//! `keyPressed` runs the pipeline IN ORDER (contract §1.1, trap 7):
//!   (A)  capture mode — the customize editor is recording a binding
//!        (ALWAYS consumes; Escape cancels, conflict keeps recording);
//!   (B)  the search-dropdown Up/Down steal — immersive wins over the
//!        desktop cortinilla when both are open (main.rs:1338-1369);
//!   (C)  the central text-input gate — a BOOL COMPUTED IN QML
//!        (`activeFocusItem instanceof TextInput || TextEdit`, §1.4.4) and
//!        passed in; zero per-field probes (scoping's stated goal);
//!   (C2) Ctrl+A select-all — separate non-rebindable branch (§4.6, trap 8);
//!        the Qt select-all seam is B2, so the stub reports "no surface
//!        accepted" and the arm falls through to (D), exactly the Slint
//!        `select_all_active_surface` false case (main.rs:1381-1386);
//!   (D)  the binding lookup → dispatch to the §2 targets.
//!
//! The synchronous-return invokable is the app's core idiom
//! (session_bridge.rs:86 `tr`): the implementation finishes its OWN state
//! mutations BEFORE or AFTER — never across — the dispatch calls that emit
//! other bridges' property notifies (those ride the cross-singleton `ui()`
//! hops and land on the next event-loop pass regardless).

use std::pin::Pin;
use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

use crate::hotkeys_qt::{
    CaptureOutcome, EscapeState, EscapeTarget, QT_KEY_DOWN, QT_KEY_UP,
};

/// Full-shape default for the groups document (trap 15): the QML columns read
/// `.col1.length` in the pre-publish frame. `boot()` refreshes immediately.
const GROUPS_EMPTY: &str = r#"{"col1":[],"col2":[],"col3":[]}"#;

#[cxx_qt::bridge]
pub mod qbz_hotkeys {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // The §3.4 groups document: the 5 groups × rows {id,label,shortcut,
        // modified,contextual}, split round-robin (cols[i%3]) into col1/2/3
        // — the QML renders three Repeater columns from one doc. Labels are
        // localized via qbz_i18n::t at build time; recomputed on open AND on
        // the trRev language bump (QML calls refresh() there, §3.4/F12).
        #[qproperty(QString, groups_json)]
        // The customize header's "N modified" accent badge (§4.5).
        #[qproperty(i32, modified_count)]
        // Capture state (§3.3): the action id being recorded ("" = not
        // recording), the live formatted pending combo, and the conflicting
        // action's label while a conflict is held.
        #[qproperty(QString, recording_id)]
        #[qproperty(QString, pending_display)]
        #[qproperty(QString, conflict_label)]
        // The two modal open flags (the B3 QML modals self-gate on these;
        // Escape stack arms 2 and 3 read them).
        #[qproperty(bool, cheatsheet_open)]
        #[qproperty(bool, customize_open)]

        type QbzHotkeys = super::QbzHotkeysRust;

        /// Registers this object's Qt-thread hop (Main.qml boots EVERY
        /// domain singleton; only QbzSession.boot also fires crate::on_boot).
        /// Also computes the first groups document (labels localized for the
        /// boot language).
        #[qinvokable]
        fn boot(self: Pin<&mut QbzHotkeys>);

        /// The ONE key entry (§1.1): the AppShell-root dispatcher calls this
        /// with the raw QKeyEvent fields + the QML-computed text-input gate.
        /// Returns true = consumed (`event.accepted = <return>`, §4.1).
        #[qinvokable]
        fn key_pressed(
            self: Pin<&mut QbzHotkeys>,
            key: i32,
            modifiers: i32,
            text: QString,
            text_input_focused: bool,
        ) -> bool;

        /// Customize editor: begin recording a binding for `id`
        /// (keybindings.rs:390-397).
        #[qinvokable]
        fn start_record(self: Pin<&mut QbzHotkeys>, id: QString);
        /// Cancel the in-flight recording (the modal's own Escape arm).
        #[qinvokable]
        fn cancel_record(self: Pin<&mut QbzHotkeys>);
        /// Drop one override / every override + refresh (keybindings.rs
        /// :313-323).
        #[qinvokable]
        fn reset_one(self: Pin<&mut QbzHotkeys>, id: QString);
        #[qinvokable]
        fn reset_all(self: Pin<&mut QbzHotkeys>);
        /// ui.showShortcuts target + the HeaderBar menu row: open the
        /// cheatsheet (and recompute the groups, §3.4 — recomputed on open).
        #[qinvokable]
        fn open_cheatsheet(self: Pin<&mut QbzHotkeys>);
        /// Cheatsheet footer: open the customize editor.
        #[qinvokable]
        fn open_customize(self: Pin<&mut QbzHotkeys>);
        /// Close both modals + cancel any in-flight recording.
        #[qinvokable]
        fn close_all(self: Pin<&mut QbzHotkeys>);
        /// Recompute groups_json + modified_count — called from the trRev
        /// language bump (§3.4/F12) and after every store mutation.
        #[qinvokable]
        fn refresh(self: Pin<&mut QbzHotkeys>);
    }

    impl cxx_qt::Threading for QbzHotkeys {}
}

use qbz_hotkeys::QbzHotkeys;

/// Rust side of the hotkeys bridge (plain storage, phase-1 pattern).
pub struct QbzHotkeysRust {
    groups_json: QString,
    modified_count: i32,
    recording_id: QString,
    pending_display: QString,
    conflict_label: QString,
    cheatsheet_open: bool,
    customize_open: bool,
}

impl Default for QbzHotkeysRust {
    fn default() -> Self {
        Self {
            groups_json: QString::from(GROUPS_EMPTY),
            modified_count: 0,
            recording_id: QString::from(""),
            pending_display: QString::from(""),
            conflict_label: QString::from(""),
            cheatsheet_open: false,
            customize_open: false,
        }
    }
}

// ---------------------------------------------------------------------------
// The UI hop (viz_bridge.rs pattern)
// ---------------------------------------------------------------------------

static QT_THREAD: OnceLock<CxxQtThread<QbzHotkeys>> = OnceLock::new();

/// Queue a hotkeys-bridge mutation onto the Qt event loop (no-op before
/// boot registers the thread).
pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzHotkeys>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}

/// Recompute + publish the groups document and the modified count (the two
/// pieces derived from the store). Called by `refresh`, `boot`, every store
/// mutation, and the open verbs (§3.4 — recompute on open).
fn publish_groups(mut this: Pin<&mut QbzHotkeys>) {
    let overrides = crate::hotkeys_qt::load_overrides();
    let json = crate::hotkeys_qt::groups_json(&overrides);
    let count = crate::hotkeys_qt::modified_count_with(&overrides);
    this.as_mut().set_groups_json(QString::from(json.as_str()));
    this.as_mut().set_modified_count(count);
}

/// Clear the capture triple (recording id + pending + conflict) — the shared
/// tail of cancel / successful bind / closeAll (keybindings.rs:399-407,
/// :461-464).
fn clear_capture(mut this: Pin<&mut QbzHotkeys>) {
    this.as_mut().set_recording_id(QString::from(""));
    this.as_mut().set_pending_display(QString::from(""));
    this.as_mut().set_conflict_label(QString::from(""));
}

/// nav.search (Ctrl+f) — B2 SEAM (divergence K6, contract §4.3): Slint's
/// `focus_search` only flips `cortinilla_open` (keybindings.rs:533-536 — the
/// "the field grabs focus on open" comment is stale), with no visible effect
/// on an empty query. The Qt decision EXCEEDS Slint: a QML `focusSearch()`
/// hook on HeaderBar (`searchInput.forceActiveFocus()`), which lands with the
/// B2 dispatcher. B1 named stub.
fn focus_search() {
    log::debug!("[qbz-qt] hotkeys: nav.search — the HeaderBar focusSearch() hook lands in B2 (K6)");
}

/// (C2) Ctrl+A select-all — B2 SEAM (contract §4.6, trap 8). Round-1 finding:
/// NO select-all or multi-select-exit invokable exists in Qt (selection state
/// is in-view QML JS; `library_bulk.rs:8`); B2 adds `QbzShell.selectAllActive()`
/// + the duck-typed view interface. The stub reports "no surface accepted",
/// so the arm falls through to (D) exactly like the Slint
/// `select_all_active_surface` false case.
fn select_all_active() -> bool {
    log::debug!("[qbz-qt] hotkeys: Ctrl+A — the QbzShell.selectAllActive() seam lands in B2 (§4.6)");
    false
}

/// §1.3 seek math, verbatim from keybindings.rs:572-581:
/// `clamp(npElapsedSecs ± d, 0, npDurationSecs) / npDurationSecs`, same 1 Hz
/// base (the now_playing model is fed by the poll pump — immersive D14).
fn seek_relative(delta: i32) {
    let (pos, duration) = crate::now_playing::position();
    if duration <= 0 {
        return;
    }
    let target = (pos + delta).clamp(0, duration);
    crate::transport_seek(target as f32 / duration as f32);
}

impl qbz_hotkeys::QbzHotkeys {
    pub fn boot(self: Pin<&mut Self>) {
        if QT_THREAD.set(self.qt_thread()).is_err() {
            log::warn!("[qbz-qt] hotkeys Qt thread already registered");
        }
        // The first groups document (labels localized for the boot language).
        ui(|h| publish_groups(h));
    }

    /// The §1.1 ordered pipeline (see the file header). true = consumed.
    pub fn key_pressed(
        mut self: Pin<&mut Self>,
        key: i32,
        modifiers: i32,
        text: QString,
        text_input_focused: bool,
    ) -> bool {
        let text = text.to_string();

        // (A) Capture mode — the customize editor is recording. ALWAYS
        // consumes (keybindings.rs:434: "Always consumes the event").
        let recording = self.recording_id().to_string();
        if !recording.is_empty() {
            let overrides = crate::hotkeys_qt::load_overrides();
            match crate::hotkeys_qt::capture_step(&overrides, &recording, key, modifiers, &text) {
                CaptureOutcome::Cancelled => {
                    clear_capture(self.as_mut());
                }
                CaptureOutcome::Ignored => {}
                CaptureOutcome::Conflict { display, label } => {
                    // Live pending + the conflict label; recording STAYS so
                    // the user can pick a different combo
                    // (keybindings.rs:454-459).
                    self.as_mut().set_pending_display(QString::from(display.as_str()));
                    self.as_mut().set_conflict_label(QString::from(label.as_str()));
                }
                CaptureOutcome::Bound { shortcut } => {
                    crate::hotkeys_qt::set_binding(&recording, &shortcut);
                    clear_capture(self.as_mut());
                    publish_groups(self.as_mut());
                }
            }
            return true;
        }

        // (B) Search-dropdown Up/Down steal — immersive wins when both are
        // open (main.rs:1338-1369). The local field handlers fire FIRST via
        // the focus chain and accept; this arm only catches keys that reached
        // the root (§1.1(B) NOTE — no double-handling).
        if key == QT_KEY_UP || key == QT_KEY_DOWN {
            let delta = if key == QT_KEY_DOWN { 1 } else { -1 };
            if crate::immersive_bridge::is_open() && crate::search_qt::imm_search_open() {
                crate::search_qt::imm_move_selection(delta);
                return true;
            }
            if crate::search_qt::cortinilla_open() {
                crate::search_qt::move_selection(delta);
                return true;
            }
        }

        // (C) The central text-input gate (main.rs:1371-1375) — computed in
        // QML per §1.4.4 and passed in. Null activeFocusItem passes (the
        // semantically right case).
        if text_input_focused {
            return false;
        }

        // (C2) Ctrl+A select-all — separate NON-REBINDABLE branch (§4.6,
        // trap 8): the predicate is `ctrl && a` with other modifiers NOT
        // excluded, and the arm consumes ONLY when a surface accepted (the
        // Slint select_all_active_surface rule — B2 wires the Qt surface).
        if crate::hotkeys_qt::is_ctrl_a(key, modifiers, &text) && select_all_active() {
            return true;
        }

        // (D) Binding lookup → dispatch (keybindings.rs:476-500; the context
        // gates live in hotkeys_qt::action_for_key).
        let overrides = crate::hotkeys_qt::load_overrides();
        let Some(action) = crate::hotkeys_qt::action_for_key(
            &overrides,
            key,
            modifiers,
            &text,
            crate::immersive_bridge::is_open(),
        ) else {
            return false;
        };
        self.run_action(action.id)
    }

    /// The §2 dispatch table (keybindings.rs:502-531 `run_action`). true =
    /// consumed. The no-op seams are CONTRACTUAL (K3, trap 10): ui.openLink
    /// (LinkResolver not ported, HeaderBar.qml:23 — activates when it lands),
    /// ui.miniPlayer (no Qt miniplayer, §6 multi-window-ready), and the
    /// mini.* rows (no-op on the main window — kept in the table for
    /// binding-file compat + the cheatsheet).
    fn run_action(mut self: Pin<&mut Self>, id: &str) -> bool {
        match id {
            "playback.toggle" => crate::transport_toggle_play(),
            "playback.next" => crate::transport_next(),
            "playback.prev" => crate::transport_previous(),
            "nav.back" => crate::nav_qt::back(),
            "nav.forward" => crate::nav_qt::forward(),
            "nav.search" => focus_search(), // B2 QML seam (K6) — named stub
            "nav.settings" => crate::navigate_to("settings"),
            "ui.sidebar" => crate::shell_bridge::ui(|s| s.cycle_sidebar()),
            "ui.focusMode" => crate::immersive_bridge::toggle(),
            "ui.queue" => crate::shell_bridge::ui(|s| s.toggle_queue()),
            "ui.escape" => return self.handle_escape(),
            "ui.showShortcuts" => {
                // keybindings.rs:521-524: open + refresh (the groups recompute
                // on open, §3.4).
                self.as_mut().set_cheatsheet_open(true);
                publish_groups(self.as_mut());
            }
            "ui.openLink" => {
                log::debug!("[qbz-qt] hotkeys: ui.openLink — no-op seam, LinkResolver not ported (K3)");
            }
            "ui.miniPlayer" => {
                log::debug!("[qbz-qt] hotkeys: ui.miniPlayer — no-op seam, no Qt miniplayer (K3/§6)");
            }
            "focus.seekForward" => seek_relative(5),
            "focus.seekBack" => seek_relative(-5),
            "focus.seekForwardLong" => seek_relative(10),
            "focus.seekBackLong" => seek_relative(-10),
            // mini.* — action_for_key already gates Context::Mini out of the
            // main window; this arm is defensive only.
            _ => return false,
        }
        true
    }

    /// ui.escape — the §1.2 priority stack. The pure ordered check is
    /// hotkeys_qt::escape_target; here the state is sampled from the live
    /// surfaces and the winning arm executes. The matched ui.escape CONSUMES
    /// even when no surface closes (Slint parity — dispatch PreventDefaults on
    /// any fired action, keybindings.rs:498-499).
    fn handle_escape(mut self: Pin<&mut Self>) -> bool {
        let state = EscapeState {
            // ABSENT-BY-GAP (§1.2.1): the LinkResolver is not ported
            // (HeaderBar.qml:23) — the arm stays in the ORDER, never taken.
            link_resolver_open: false,
            customize_open: *self.customize_open(),
            cheatsheet_open: *self.cheatsheet_open(),
            cortinilla_open: crate::search_qt::cortinilla_open(),
            immersive_open: crate::immersive_bridge::is_open(),
            // §4.6 seam: no Qt multi-select exit invokable exists yet (B2
            // adds QbzShell.exitMultiSelect + the view reporters).
            multi_select_active: false,
            queue_open: crate::shell_bridge::queue_open(),
        };
        match crate::hotkeys_qt::escape_target(&state) {
            EscapeTarget::Customize => self.as_mut().set_customize_open(false),
            EscapeTarget::Cheatsheet => self.as_mut().set_cheatsheet_open(false),
            EscapeTarget::Cortinilla => crate::search_qt::dismiss(),
            EscapeTarget::Immersive => crate::immersive_bridge::close(),
            EscapeTarget::MultiSelect => {
                log::debug!("[qbz-qt] hotkeys: multi-select exit ABSENT — the §4.6 seam lands in B2");
            }
            EscapeTarget::Queue => crate::shell_bridge::ui(|s| s.toggle_queue()),
            EscapeTarget::LinkResolver | EscapeTarget::None => {}
        }
        true
    }

    pub fn start_record(mut self: Pin<&mut Self>, id: QString) {
        // keybindings.rs:390-397: record + clear pending/conflict.
        self.as_mut().set_recording_id(id);
        self.as_mut().set_pending_display(QString::from(""));
        self.as_mut().set_conflict_label(QString::from(""));
    }

    pub fn cancel_record(self: Pin<&mut Self>) {
        clear_capture(self);
    }

    pub fn reset_one(self: Pin<&mut Self>, id: QString) {
        crate::hotkeys_qt::reset_one(&id.to_string());
        publish_groups(self);
    }

    pub fn reset_all(self: Pin<&mut Self>) {
        crate::hotkeys_qt::reset_all();
        publish_groups(self);
    }

    pub fn open_cheatsheet(mut self: Pin<&mut Self>) {
        self.as_mut().set_cheatsheet_open(true);
        publish_groups(self.as_mut());
    }

    pub fn open_customize(mut self: Pin<&mut Self>) {
        self.as_mut().set_customize_open(true);
        publish_groups(self.as_mut());
    }

    pub fn close_all(mut self: Pin<&mut Self>) {
        self.as_mut().set_cheatsheet_open(false);
        self.as_mut().set_customize_open(false);
        clear_capture(self.as_mut());
    }

    pub fn refresh(self: Pin<&mut Self>) {
        publish_groups(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_default_is_full_shape() {
        // trap 15: the construction-seeded default parses and carries all
        // three columns — QML reads `.colN.length` in the pre-publish frame.
        let doc: serde_json::Value = serde_json::from_str(GROUPS_EMPTY).unwrap();
        for col in ["col1", "col2", "col3"] {
            assert!(doc[col].is_array());
        }
    }
}
