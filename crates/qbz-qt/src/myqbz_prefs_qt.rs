//! MyQBZ per-user sidecars — the Slint-free port of
//! `crates/qbz/src/myqbz_prefs.rs` (nav BRANDING: custom label + custom icon)
//! and `crates/qbz/src/myqbz_view_prefs.rs` (per-collection VIEW PREFS).
//!
//! Both concerns live here because both are tiny per-user JSON sidecars under
//! the same directory, so two files would duplicate the path helper
//! (spec 02 §3).
//!
//! ```text
//! <data_dir>/qbz/users/<uid>/myqbz_branding.json        { label, icon_path }
//! <data_dir>/qbz/users/<uid>/collection_view_prefs.json { "<id>": { …7 fields… } }
//! ```
//!
//! Both documents are SHARED with the shipping Slint app, so:
//!  - the persisted keys stay snake_case and EXACTLY as Slint spells them — a
//!    rename would silently drop the user's stored prefs (spec 02 §12 Q11);
//!  - every write goes through `settings_qt::read_json_object` +
//!    `write_json_object_atomic` (temp file + `rename(2)`), never `fs::write`
//!    and never a rebuild from `{}` — the discipline at
//!    `settings_qt.rs:109-150`, which `settings_qt::save_myqbz_label` already
//!    applies to `myqbz_branding.json`.
//!
//! Branding semantics (1:1 with `myqbz_prefs.rs`):
//!  - a trimmed-empty label coerces to the default `"My QBZ"` and THAT literal
//!    is what gets persisted;
//!  - `icon_path` is an absolute filesystem path or "" for the default glyph;
//!    reset stores "" rather than a default path;
//!  - a stored path whose file has gone missing does NOT mutate the store — the
//!    user can re-pick and the path is preserved in case the file returns. The
//!    published doc simply reports `hasCustomIcon: false`.
//!
//! View-prefs lifecycle (driven by `myqbz_detail_qt`):
//!  - restore on open: `load_view_prefs(id)`;
//!  - persist on change: `save_view_prefs(id, &prefs)`;
//!  - clear on delete: `remove_view_prefs(id)`.
//!
//! The hydration gate (spec 02 §7 T15 — no persist until `apply()` has restored
//! the stored prefs, or an early setter clobbers them) lives in
//! `myqbz_detail_qt`, which is the state machine that opens and closes it
//! (`reset()` closes, `apply()` opens after the restore, `teardown()` closes).
//! This module deliberately holds NO second copy of that latch: it had one, and
//! because nothing ever opened it every `save_view_prefs` call returned early —
//! the per-collection prefs were never written at all.

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use cxx_qt_lib::QString;
use serde::{Deserialize, Serialize};

/// The default "My QBZ" label (`myqbz_prefs.rs:34 DEFAULT_LABEL`). Persisted
/// verbatim, untranslated — `NavFlyout.qml` falls back to a translated literal
/// only when the stored label is absent (wiring W15).
pub(crate) const DEFAULT_LABEL: &str = "My QBZ";

const BRANDING_FILE: &str = "myqbz_branding.json";
const VIEW_PREFS_FILE: &str = "collection_view_prefs.json";

/// The per-user directory, bound at session activation. `None` before login —
/// both stores then degrade to defaults (there is no pre-login MyQBZ surface).
static USER_DIR: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));

/// The bound per-user dir, falling back to the shell's own binding so this
/// module and `settings_qt::save_myqbz_label` always resolve the SAME file.
fn user_dir() -> Option<PathBuf> {
    if let Some(dir) = USER_DIR
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .cloned()
    {
        return Some(dir);
    }
    crate::sidebar_qt::user_dir()
}

fn branding_path() -> Option<PathBuf> {
    Some(user_dir()?.join(BRANDING_FILE))
}

fn view_prefs_path() -> Option<PathBuf> {
    Some(user_dir()?.join(VIEW_PREFS_FILE))
}

// ---------------------------------------------------------------------------
// Branding store
// ---------------------------------------------------------------------------

/// The persisted branding. Missing fields default sanely so an older file (and
/// the `{}` a first run starts from) still deserializes.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Branding {
    #[serde(default = "default_label")]
    label: String,
    /// Absolute path to a custom icon, or "" for the default glyph.
    #[serde(default)]
    icon_path: String,
}

fn default_label() -> String {
    DEFAULT_LABEL.to_string()
}

impl Default for Branding {
    fn default() -> Self {
        Self {
            label: default_label(),
            icon_path: String::new(),
        }
    }
}

/// Load the active user's branding. A missing / unreadable / unparseable file
/// degrades to defaults (`myqbz_prefs.rs:79`).
fn read_branding() -> Branding {
    let Some(path) = branding_path() else {
        return Branding::default();
    };
    match std::fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Branding::default(),
    }
}

/// Read-modify-write ONE key of the co-owned document. A document that exists
/// but does not parse is left alone rather than rebuilt (settings_qt's rule 2).
fn write_branding_key(key: &str, value: String) {
    let Some(path) = branding_path() else {
        log::warn!("[qbz-qt] myqbz branding: no active user, not saving");
        return;
    };
    let Some(mut doc) = crate::settings_qt::read_json_object(&path) else {
        return;
    };
    doc.insert(key.to_string(), serde_json::Value::String(value));
    crate::settings_qt::write_json_object_atomic(&path, &doc);
}

/// Coerce a raw label input to the persisted value: trimmed-empty becomes the
/// default, and the default STRING is what is stored (`myqbz_prefs.rs:121`).
fn coerce_label(label: &str) -> String {
    let trimmed = label.trim();
    if trimmed.is_empty() {
        DEFAULT_LABEL.to_string()
    } else {
        trimmed.to_string()
    }
}

// ---------------------------------------------------------------------------
// Branding document (spec 02 §5.7 BrandingDoc)
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize)]
struct BrandingDoc {
    label: String,
    /// `file://…` for a readable custom icon, "" otherwise.
    #[serde(rename = "iconPath")]
    icon_path: String,
    /// True only when a path is stored AND the file exists.
    #[serde(rename = "hasCustomIcon")]
    has_custom_icon: bool,
}

/// Resolve the (label, icon) pair the nav renders (`myqbz_prefs.rs:158`).
/// A stored-but-missing file yields `hasCustomIcon: false` and an empty path;
/// the store is NOT mutated.
fn branding_doc() -> BrandingDoc {
    let b = read_branding();
    let trimmed = b.icon_path.trim();
    if trimmed.is_empty() {
        return BrandingDoc {
            label: b.label,
            icon_path: String::new(),
            has_custom_icon: false,
        };
    }
    if Path::new(trimmed).is_file() {
        BrandingDoc {
            label: b.label,
            icon_path: crate::artwork_qt::file_url(trimmed),
            has_custom_icon: true,
        }
    } else {
        log::warn!(
            "[qbz-qt] myqbz branding: custom icon '{trimmed}' is not a readable \
             file, using the default glyph"
        );
        BrandingDoc {
            label: b.label,
            icon_path: String::new(),
            has_custom_icon: false,
        }
    }
}

/// The seed for `QbzMyQbzRust::default()`. `brandingJson` is seeded at
/// CONSTRUCTION, not at `boot()`: `NavFlyout.qml` binds the section label and
/// the very first frame is already too late for a post-boot push (the same
/// reason `theme_json` and `window_width` are seeded there).
pub(crate) fn branding_json() -> String {
    serde_json::to_string(&branding_doc()).unwrap_or_else(|_| "{}".into())
}

/// Push the branding onto the bridge. Called on session activation, after every
/// set/reset, and by `settings_qt::save_myqbz_label` (wiring W11) so the
/// sidebar and the header flyout update live.
pub(crate) fn republish_branding() {
    let json = branding_json();
    crate::myqbz_bridge::ui(move |mut b| {
        b.as_mut().set_branding_json(QString::from(json.as_str()));
    });
}

/// Persist a new label and republish. A trimmed-empty input coerces to
/// `"My QBZ"` and that literal is what is written.
pub(crate) fn set_label(label: &str) {
    write_branding_key("label", coerce_label(label));
    republish_branding();
}

/// Persist a custom icon path. An empty / whitespace path CLEARS the custom
/// icon (`myqbz_prefs.rs:141`).
pub(crate) fn set_icon_path(path: &str) {
    write_branding_key("icon_path", path.trim().to_string());
    republish_branding();
}

/// Reset to the default branded glyph — stores "" rather than a default path.
pub(crate) fn reset_icon() {
    set_icon_path("");
}

/// Open the native image picker; on pick, persist the path and republish. A
/// cancel is a no-op with NO toast (`myqbz_prefs.rs:208`). The filter matches
/// the reference's set exactly.
pub(crate) fn pick_icon() {
    crate::spawn(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .set_title(&qbz_i18n::t_args("Choose a {} icon", &[DEFAULT_LABEL]))
            .add_filter(&qbz_i18n::t("Image"), &["svg", "png", "jpg", "jpeg", "webp"])
            .pick_file()
            .await
        else {
            return; // cancelled — leave the branding untouched.
        };
        let path = file.path().to_string_lossy().to_string();
        set_icon_path(&path);
    });
}

// ---------------------------------------------------------------------------
// Per-collection view prefs
// ---------------------------------------------------------------------------

/// The SEVEN persisted view-pref fields for one collection. Slint has no Set,
/// so the source filter is three independent flags; together they round-trip
/// the Tauri `sourceFilter:[SourceKind]` array. `searchQuery` and `selectMode`
/// are deliberately TRANSIENT and never persisted.
///
/// Field names are the on-disk keys and are shared with the Slint build — do
/// NOT rename them or add `#[serde(rename)]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prefs {
    #[serde(default = "d_list")]
    pub view_mode: String,
    #[serde(default = "d_position")]
    pub sort_by: String,
    #[serde(default = "d_asc")]
    pub sort_dir: String,
    #[serde(default = "d_all")]
    pub type_filter: String,
    #[serde(default)]
    pub src_qobuz: bool,
    #[serde(default)]
    pub src_plex: bool,
    #[serde(default)]
    pub src_local: bool,
}

fn d_list() -> String {
    "list".to_string()
}
fn d_position() -> String {
    "position".to_string()
}
fn d_asc() -> String {
    "asc".to_string()
}
fn d_all() -> String {
    "all".to_string()
}

impl Default for Prefs {
    /// The §18 defaults: list / position / asc / all / empty source set.
    fn default() -> Self {
        Self {
            view_mode: d_list(),
            sort_by: d_position(),
            sort_dir: d_asc(),
            type_filter: d_all(),
            src_qobuz: false,
            src_plex: false,
            src_local: false,
        }
    }
}

/// The whole `{ collection-id -> Prefs }` map. A missing / unreadable /
/// unparseable file degrades to an empty map.
fn read_view_prefs() -> serde_json::Map<String, serde_json::Value> {
    let Some(path) = view_prefs_path() else {
        return serde_json::Map::new();
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(serde_json::Value::Object(map)) => map,
            _ => serde_json::Map::new(),
        },
        Err(_) => serde_json::Map::new(),
    }
}

/// The stored prefs for `id`, or the defaults when none are stored (or the
/// stored entry is malformed).
pub(crate) fn load_view_prefs(id: &str) -> Prefs {
    if id.is_empty() {
        return Prefs::default();
    }
    read_view_prefs()
        .get(id)
        .cloned()
        .and_then(|v| serde_json::from_value::<Prefs>(v).ok())
        .unwrap_or_default()
}

/// Persist the prefs for `id` (read-modify-write the whole map). No-op for an
/// empty id. Writing the default set is harmless — a re-open restores the same
/// defaults.
///
/// The T15 hydration gate is NOT checked here: the sole caller
/// (`myqbz_detail_qt::persist_prefs`) owns it and returns before reaching this
/// function while the gate is closed. A second latch in this module is what
/// silently disabled persistence entirely.
pub(crate) fn save_view_prefs(id: &str, prefs: &Prefs) {
    if id.is_empty() {
        return;
    }
    let Some(path) = view_prefs_path() else {
        log::warn!("[qbz-qt] collection view-prefs: no active user, not saving");
        return;
    };
    let Ok(value) = serde_json::to_value(prefs) else {
        return;
    };
    let Some(mut doc) = crate::settings_qt::read_json_object(&path) else {
        return;
    };
    doc.insert(id.to_string(), value);
    crate::settings_qt::write_json_object_atomic(&path, &doc);
}

/// Drop the orphaned key after a collection is deleted. No-op when absent.
pub(crate) fn remove_view_prefs(id: &str) {
    if id.is_empty() {
        return;
    }
    let Some(path) = view_prefs_path() else {
        return;
    };
    let Some(mut doc) = crate::settings_qt::read_json_object(&path) else {
        return;
    };
    if doc.remove(id).is_some() {
        crate::settings_qt::write_json_object_atomic(&path, &doc);
    }
}

// ---------------------------------------------------------------------------
// Session binding
// ---------------------------------------------------------------------------

/// Bind both stores to the activated user's directory and publish that user's
/// branding. Called from all three session-activation blocks in `auth_qt`
/// (login / restore / offline entry — wiring W7).
/// The T15 gate is not touched here: `myqbz_detail_qt` closes it in `reset()`
/// (every load) and in `teardown()` (logout), and its document's id is empty
/// until a collection is actually opened, so no persist can escape to the new
/// user's file before `apply()` restores that collection's prefs.
pub(crate) fn init_for_user(dir: &Path) {
    *USER_DIR.lock().unwrap_or_else(|e| e.into_inner()) = Some(dir.to_path_buf());
    republish_branding();
}

/// Drop the user binding on logout and publish the default branding, or the
/// next account inherits the previous one's label and icon.
pub(crate) fn teardown() {
    *USER_DIR.lock().unwrap_or_else(|e| e.into_inner()) = None;
    let json = serde_json::to_string(&BrandingDoc {
        label: DEFAULT_LABEL.to_string(),
        icon_path: String::new(),
        has_custom_icon: false,
    })
    .unwrap_or_else(|_| "{}".into());
    crate::myqbz_bridge::ui(move |mut b| {
        b.as_mut().set_branding_json(QString::from(json.as_str()));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coerce_blank_label_yields_the_default() {
        assert_eq!(coerce_label(""), "My QBZ");
        assert_eq!(coerce_label("   "), "My QBZ");
        assert_eq!(coerce_label("  Tapes  "), "Tapes");
        assert_eq!(coerce_label("Tapes"), "Tapes");
    }

    #[test]
    fn branding_defaults_and_legacy_json() {
        let b = Branding::default();
        assert_eq!(b.label, "My QBZ");
        assert!(b.icon_path.is_empty());
        let b: Branding = serde_json::from_str("{}").expect("empty object deserializes");
        assert_eq!(b.label, "My QBZ");
        assert!(b.icon_path.is_empty());
        let b: Branding =
            serde_json::from_str(r#"{"label":"Tapes"}"#).expect("partial object deserializes");
        assert_eq!(b.label, "Tapes");
        assert!(b.icon_path.is_empty());
    }

    #[test]
    fn view_prefs_defaults_match_spec_18() {
        let p = Prefs::default();
        assert_eq!(p.view_mode, "list");
        assert_eq!(p.sort_by, "position");
        assert_eq!(p.sort_dir, "asc");
        assert_eq!(p.type_filter, "all");
        assert!(!p.src_qobuz && !p.src_plex && !p.src_local);
    }

    #[test]
    fn view_prefs_partial_json_keeps_present_fields() {
        let p: Prefs = serde_json::from_str(r#"{"view_mode":"grid","src_plex":true}"#)
            .expect("partial object deserializes");
        assert_eq!(p.view_mode, "grid");
        assert!(p.src_plex);
        assert_eq!(p.sort_by, "position");
        assert_eq!(p.type_filter, "all");
        assert!(!p.src_qobuz);
    }

    #[test]
    fn view_prefs_keys_stay_snake_case_on_disk() {
        // The file is shared with the Slint build; a camelCase rename would
        // silently drop the user's stored prefs.
        let json = serde_json::to_string(&Prefs::default()).expect("serializes");
        for key in [
            "view_mode",
            "sort_by",
            "sort_dir",
            "type_filter",
            "src_qobuz",
            "src_plex",
            "src_local",
        ] {
            assert!(json.contains(&format!("\"{key}\"")), "missing key {key}");
        }
    }

    /// An empty id never reaches the store, on either path. The T15 hydration
    /// gate is asserted where it lives, in `myqbz_detail_qt`.
    #[test]
    fn empty_id_is_a_no_op_on_every_view_prefs_path() {
        assert_eq!(load_view_prefs(""), Prefs::default());
        // No user dir is bound in tests, so these cannot touch the filesystem
        // either way; what is pinned is that the id guard comes first.
        save_view_prefs("", &Prefs::default());
        remove_view_prefs("");
    }
}
