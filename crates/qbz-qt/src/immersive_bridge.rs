//! QbzImmersive — immersive-mode domain bridge (viz_bridge.rs exemplar:
//! OnceLock<CxxQtThread>, `ui()` no-op pre-boot, `boot()` invokable, `impl
//! Default` for construction-seeded values).
//!
//! Port of the Slint `ImmersiveState` global per the immersive-port contract
//! (`qbz-nix-docs/qt-frontend/2026-08-02-immersive-port/00-CONTRACT.md` §3).
//! All enums are `i32` with the Slint numbering verbatim
//! (`state.slint:4238,4249,4254`): view_mode 0 FOCUS / 1 SPLIT; mode 0 Album
//! Reactive, 1 Static, 2 Coverflow, 3 Spectrum, 4 Lyrics, 5 Queue, 6 WaveBed;
//! split_panel 0 Lyrics, 1 TrackInfo, 2 Suggestions, 3 Queue.
//!
//! The `open` property uses a CUSTOM WRITE (`set_open`) so EVERY write — the
//! five QML exit paths AND the Rust invokables — funnels through one Rust
//! setter (contract §3/D3: Qt's answer to Slint's 6 write sites + 150 ms
//! guard timer). The false→true edge runs §3.1 apply-default-view; both edges
//! drive the viz two-source enable (§3.3a/§4.2).
//!
//! The §3.4 search surface is a BRIDGE SURFACE ONLY in B1: `search_live`
//! mirrors the text, `dismiss_search` clears open+selection, move/activate
//! are no-ops. The controller (debounce, ≥2-char gate, version-guarded load,
//! dispatch table) lands in B5 (`search_qt.rs`).

use std::pin::Pin;
use std::sync::OnceLock;

use cxx_qt::CxxQtThread;
use cxx_qt::Threading as _;
use cxx_qt_lib::QString;

/// Full-shape default for the search document (contract §3.4 payload: a
/// `sections` array, NEVER "{}" — trap 15). B5 fills the section schema when
/// the controller lands.
const IMM_SEARCH_EMPTY: &str = r#"{"sections":[]}"#;

/// Full-shape default for the atmosphere-independent docs is not needed, but
/// the glow default IS a color Qt must parse: Qt colors are `#AARRGGBB`, so
/// the Slint default `#6464ff59` (RRGGBBAA, `state.slint:4231`) is stored
/// converted. `ambient_qt` publishes the same conversion — see its comment.
const GLOW_DEFAULT_QT: &str = "#596464ff";

#[cxx_qt::bridge]
pub mod qbz_immersive {
    extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // Overlay open flag. CUSTOM WRITE (the `set_open` funnel below) —
        // QML writes `QbzImmersive.open = false` on every exit path and the
        // same edge logic runs (contract §3/D3).
        #[qproperty(bool, open, READ, WRITE = set_open, NOTIFY)]
        // 0 FOCUS / 1 SPLIT. Mutated ONLY by `setView` / the open-edge
        // restore — QML never writes these directly (§3.2).
        #[qproperty(i32, view_mode)]
        // FOCUS foreground: 0 AlbumReactive, 1 Static, 2 Coverflow, 3
        // Spectrum, 4 Lyrics, 5 Queue, 6 WaveBed.
        #[qproperty(i32, mode)]
        // SPLIT active panel: 0 Lyrics, 1 TrackInfo, 2 Suggestions, 3 Queue.
        #[qproperty(i32, split_panel)]
        // AlbumReactive glow, Qt `#AARRGGBB` (Slint `#6464ff59` RRGGBBAA
        // converted — see GLOW_DEFAULT_QT). Published by ambient_qt only on
        // success, never cleared (§4.3).
        #[qproperty(QString, glow_color)]
        // 128×128 atmosphere PNG as a file:// URL. Published ONLY when a
        // fresh asset is ready — NEVER cleared on track change (the flicker
        // fix, playback.rs:2344-2351 semantics; §4.3).
        #[qproperty(QString, atmosphere_url)]
        // --- Immersive search surface (§3.4; controller lands in B5) -------
        #[qproperty(bool, imm_search_open)]
        // Sections Albums/Artists/Playlists (+ local albums offline) — NO
        // track rows, NO top result. Full-shape default, never "{}".
        #[qproperty(QString, imm_search_json)]
        #[qproperty(bool, imm_search_loading)]
        // -1 = no selection (Down from -1 selects the first row; no wrap).
        #[qproperty(i32, imm_search_selected_index)]
        #[qproperty(f64, imm_search_scroll_y)]
        // Two-way with the search field.
        #[qproperty(QString, search_input_text)]

        type QbzImmersive = super::QbzImmersiveRust;

        /// Registers this object's Qt-thread hop (Main.qml boots EVERY
        /// domain singleton; only QbzSession.boot also fires crate::on_boot).
        #[qinvokable]
        fn boot(self: Pin<&mut QbzImmersive>);

        /// Shift+I path (§3.3): flips `open`; does NOT force mode=0
        /// (1:1 keybindings.rs:538-545).
        #[qinvokable]
        fn toggle(self: Pin<&mut QbzImmersive>);

        /// NPB ViewModeMenu path (§3.3): FIRST setView(current view_mode, 0,
        /// current split_panel), THEN open=true.
        #[qinvokable]
        fn open_from_menu(self: Pin<&mut QbzImmersive>);

        /// The ONLY mode mutator (§3.2): sets the three properties and
        /// persists the triple ONLY when immersive_default_view == "remember".
        #[qinvokable]
        fn set_view(self: Pin<&mut QbzImmersive>, view_mode: i32, mode: i32, split_panel: i32);

        /// §3.4 search surface — B1: mirrors text into search_input_text and
        /// nothing else (the controller lands in B5).
        #[qinvokable]
        fn search_live(self: Pin<&mut QbzImmersive>, text: QString);
        /// §3.4 search surface — B1: NO-OP (B5 implements the clamp/scroll).
        #[qinvokable]
        fn search_move_selection(self: Pin<&mut QbzImmersive>, delta: i32);
        /// §3.4 search surface — B1: NO-OP (B5 implements the dispatch table).
        #[qinvokable]
        fn search_row_activated(self: Pin<&mut QbzImmersive>, flat_index: i32);
        /// §3.4: clears imm_search_open + the selection.
        #[qinvokable]
        fn dismiss_search(self: Pin<&mut QbzImmersive>);
    }

    // The custom WRITE target of the `open` property. NOT a qinvokable — QML
    // reaches it by writing `QbzImmersive.open`, never by name. (cxx-qt
    // custom-setter pattern: the property's auto setter is replaced by this
    // method, which owns the store + notify + edge logic.)
    unsafe extern "RustQt" {
        fn set_open(self: Pin<&mut QbzImmersive>, value: bool);
    }

    impl cxx_qt::Threading for QbzImmersive {}
}

use qbz_immersive::QbzImmersive;

/// Rust side of the immersive bridge (plain storage, phase-1 pattern).
pub struct QbzImmersiveRust {
    open: bool,
    view_mode: i32,
    mode: i32,
    split_panel: i32,
    glow_color: QString,
    atmosphere_url: QString,
    imm_search_open: bool,
    imm_search_json: QString,
    imm_search_loading: bool,
    imm_search_selected_index: i32,
    imm_search_scroll_y: f64,
    search_input_text: QString,
}

impl Default for QbzImmersiveRust {
    fn default() -> Self {
        Self {
            open: false,
            view_mode: 0,
            mode: 0,
            split_panel: 0,
            glow_color: QString::from(GLOW_DEFAULT_QT),
            atmosphere_url: QString::from(""),
            imm_search_open: false,
            imm_search_json: QString::from(IMM_SEARCH_EMPTY),
            imm_search_loading: false,
            imm_search_selected_index: -1,
            imm_search_scroll_y: 0.0,
            search_input_text: QString::from(""),
        }
    }
}

// ---------------------------------------------------------------------------
// The UI hop (viz_bridge.rs pattern)
// ---------------------------------------------------------------------------

static QT_THREAD: OnceLock<CxxQtThread<QbzImmersive>> = OnceLock::new();

/// Queue an immersive-bridge mutation onto the Qt event loop (no-op before
/// boot registers the thread).
pub(crate) fn ui(f: impl FnOnce(Pin<&mut QbzImmersive>) + Send + 'static) {
    if let Some(thread) = QT_THREAD.get() {
        let _ = thread.queue(f);
    }
}

/// §3.1 pinned map: a pinned default always lands on `view_mode=0` + the
/// matching FOCUS `mode` (never WaveBed). `None` = "remember"/unknown — the
/// caller restores the persisted triple / does nothing respectively.
fn pinned_view(default_key: &str) -> Option<(i32, i32)> {
    match default_key {
        "reactive" => Some((0, 0)),
        "static" => Some((0, 1)),
        "coverflow" => Some((0, 2)),
        "spectrum" => Some((0, 3)),
        "lyrics" => Some((0, 4)),
        "queue" => Some((0, 5)),
        _ => None,
    }
}

/// The single funnel behind every `open` write (contract §3/D3). Holds the
/// store + notify, the §3.1 apply-default-view on the false→true edge, and
/// the §3.3a viz keep-alive on both edges.
fn apply_open(mut this: Pin<&mut QbzImmersive>, value: bool) {
    use cxx_qt::CxxQtType as _;
    if this.open == value {
        return;
    }
    this.as_mut().rust_mut().open = value;
    this.as_mut().open_changed();
    if value {
        // §3.1 apply-default-view — runs on EVERY false→true open edge, from
        // both entries. Restore writes go DIRECTLY to the properties, NOT
        // through set_view (no re-persist on restore — §3.1). The Slint
        // shader-mode force-reset is a v1 no-op (§1.3).
        let key = crate::settings_qt::pref_str("immersive_default_view", "remember");
        if key == "remember" {
            // The remember-last triple. Stored as JSON NUMBERS: ui_prefs.json
            // is co-owned with the Slint app, whose typed ui_prefs struct
            // declares these keys i32 (crates/qbz/src/ui_prefs.rs:517-526) —
            // string writes would poison its whole-document load. (The
            // contract's "pref_str i32-parse" would read NEITHER app's
            // numbers; pref_i32 is the compatible reader — delivery notes.)
            let vm = crate::settings_qt::pref_i32("immersive_last_view_mode", 0);
            let m = crate::settings_qt::pref_i32("immersive_last_mode", 0);
            let sp = crate::settings_qt::pref_i32("immersive_last_split_panel", 0);
            this.as_mut().set_view_mode(vm);
            this.as_mut().set_mode(m);
            this.as_mut().set_split_panel(sp);
        } else if let Some((vm, m)) = pinned_view(&key) {
            // Pinned: view_mode 0 + the mapped mode; split_panel untouched.
            this.as_mut().set_view_mode(vm);
            this.as_mut().set_mode(m);
        }
        // Unknown key: Slint's `_ => {}` — leave the current view alone.
        crate::viz_qt::immersive_opened();
    } else {
        crate::viz_qt::immersive_closed();
    }
}

impl qbz_immersive::QbzImmersive {
    pub fn boot(self: Pin<&mut Self>) {
        if QT_THREAD.set(self.qt_thread()).is_err() {
            log::warn!("[qbz-qt] immersive Qt thread already registered");
        }
    }

    /// The custom WRITE target of the `open` property — the funnel every
    /// QML `QbzImmersive.open = ...` write passes through (contract §3/D3).
    pub fn set_open(self: Pin<&mut Self>, value: bool) {
        apply_open(self, value);
    }

    /// Shift+I path (§3.3): flips `open`; mode NOT forced.
    pub fn toggle(mut self: Pin<&mut Self>) {
        let next = !self.open;
        apply_open(self.as_mut(), next);
    }

    /// NPB ViewModeMenu path (§3.3): FIRST setView(current view_mode, 0,
    /// current split_panel) (persists per §3.2 when "remember"), THEN
    /// open=true — the §3.1 restore on the open edge then lands the
    /// remembered view-mode/split with mode 0. Net: the menu ALWAYS opens on
    /// Album Reactive with the remembered view-mode/split.
    pub fn open_from_menu(mut self: Pin<&mut Self>) {
        let (vm, sp) = (self.view_mode, self.split_panel);
        self.as_mut().set_view(vm, 0, sp);
        apply_open(self.as_mut(), true);
    }

    /// The ONLY mode mutator (§3.2). Persists the triple via
    /// `settings_qt::save_pref` ONLY when immersive_default_view ==
    /// "remember"; a pinned default wins on the next open, so it must not be
    /// overwritten (main.rs:10339-10357 parity).
    pub fn set_view(mut self: Pin<&mut Self>, view_mode: i32, mode: i32, split_panel: i32) {
        self.as_mut().set_view_mode(view_mode);
        self.as_mut().set_mode(mode);
        self.as_mut().set_split_panel(split_panel);
        if crate::settings_qt::pref_str("immersive_default_view", "remember") == "remember" {
            // JSON numbers — co-owned file, Slint declares the keys i32 (see
            // apply_open's comment).
            crate::settings_qt::save_pref(
                "immersive_last_view_mode",
                serde_json::Value::Number(view_mode.into()),
            );
            crate::settings_qt::save_pref(
                "immersive_last_mode",
                serde_json::Value::Number(mode.into()),
            );
            crate::settings_qt::save_pref(
                "immersive_last_split_panel",
                serde_json::Value::Number(split_panel.into()),
            );
        }
    }

    /// §3.4 surface — B1: mirror the text only. The 220 ms debounce, the
    /// ≥2-char gate, the pref re-read and the version-guarded load all land
    /// with the B5 controller in search_qt.rs.
    pub fn search_live(mut self: Pin<&mut Self>, text: QString) {
        self.as_mut().set_search_input_text(text);
    }

    /// §3.4 surface — B1 NO-OP: B5 implements the arrow clamp (Down from -1
    /// selects the first row, Up from the first row returns to -1, no wrap)
    /// and the 56/24/4 scroll-y arithmetic.
    pub fn search_move_selection(self: Pin<&mut Self>, _delta: i32) {}

    /// §3.4 surface — B1 NO-OP: B5 implements the per-row-kind × action
    /// dispatch table (album/artist/playlist × replace/next/queue).
    pub fn search_row_activated(self: Pin<&mut Self>, _flat_index: i32) {}

    /// §3.4 dismiss: clears the dropdown + the selection (main.rs:10552-10564).
    pub fn dismiss_search(mut self: Pin<&mut Self>) {
        self.as_mut().set_imm_search_open(false);
        self.as_mut().set_imm_search_selected_index(-1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_map_matches_the_slint_open_table() {
        // main.rs:10311-10336: reactive/static/coverflow/spectrum/lyrics/queue
        // -> view_mode 0 + mode 0..5; a pinned view never lands on WaveBed.
        assert_eq!(pinned_view("reactive"), Some((0, 0)));
        assert_eq!(pinned_view("static"), Some((0, 1)));
        assert_eq!(pinned_view("coverflow"), Some((0, 2)));
        assert_eq!(pinned_view("spectrum"), Some((0, 3)));
        assert_eq!(pinned_view("lyrics"), Some((0, 4)));
        assert_eq!(pinned_view("queue"), Some((0, 5)));
    }

    #[test]
    fn remember_and_unknown_keys_are_not_pinned() {
        assert_eq!(pinned_view("remember"), None);
        assert_eq!(pinned_view(""), None);
        assert_eq!(pinned_view("wavebed"), None); // pinned never lands on WaveBed
    }

    #[test]
    fn search_doc_default_is_full_shape() {
        let doc: serde_json::Value = serde_json::from_str(IMM_SEARCH_EMPTY).unwrap();
        assert!(doc.get("sections").is_some_and(|s| s.is_array()));
    }

    #[test]
    fn glow_default_is_the_slint_default_in_qt_byte_order() {
        // Slint #6464ff59 (RRGGBBAA) -> Qt #AARRGGBB: alpha 0x59 first.
        assert_eq!(GLOW_DEFAULT_QT, "#596464ff");
    }
}
