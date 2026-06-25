//! Playlist Importer controller — the Rust side of the "Import Playlist"
//! modal (PlaylistImportModal.slint), a 1:1 port of Tauri's
//! PlaylistImportModal.svelte driven by the headless `qbz-playlist-import`
//! crate. Every interpolated string (log lines, status line, summary
//! block) is formatted HERE and pushed into PlaylistImportState
//! pre-formatted; provider detection lives here too (Slint 1.16 strings
//! have no `.contains`, so every URL keystroke round-trips through
//! `url-edited`).
//!
//! Close-mid-import semantics (spec §1.8): closing the modal never cancels
//! the tokio import task. On completion the toast + sidebar refresh still
//! fire (main.rs arm); navigation happens only while the modal is still
//! open AND the run's generation is current. [`GENERATION`] is bumped on
//! every open() and execute(), so a stale run's sink events / completion
//! can never touch a reopened modal's fresh state.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use qbz_playlist_import::{
    detect_provider_key, ImportEvent, ImportPhase, ImportPlaylist, ImportProgressSink,
    ImportProvider, ImportSummary, ProviderKey,
};

use crate::{AppWindow, ImportLogEntry, PlaylistImportState, SidebarState};

/// Rust-side mirror of the Svelte component state that never reaches the
/// UI (sidebar.rs module-state pattern). Reset wholesale on every open.
#[derive(Default)]
struct Session {
    preview: Option<ImportPlaylist>,
    /// Trimmed URL the preview was fetched for (Svelte `previewUrl`).
    preview_url: String,
    /// Provider locked at fetch time; survives URL edits until the reset
    /// paths clear it (Svelte `lockedProvider`).
    locked_provider: Option<ProviderKey>,
    /// Trimmed URL of the last completed import (Svelte `lastImportedUrl`).
    last_imported_url: String,
    /// 5%-milestone tracker for the matching log lines (-1 = none yet).
    last_logged_percent: i32,
    /// Mirror of the modal's rename field, kept fresh by `name-edited`
    /// and read at execute time (Svelte `customName`).
    custom_name: String,
}

static SESSION: LazyLock<Mutex<Session>> = LazyLock::new(|| Mutex::new(Session::default()));

/// Import generation (§1.8): bumped on every open() and execute(). Sink
/// events and task completions carry the generation they were spawned
/// with; a mismatch means the modal was reset for a fresh run, so the
/// stale run may only fire toast + sidebar refresh, never modal writes.
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn current_generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

fn bump_generation() -> u64 {
    GENERATION.fetch_add(1, Ordering::SeqCst) + 1
}

/// Open the modal fully reset — Tauri remounts the Svelte component on
/// every open, so nothing persists. Event-loop thread.
pub fn open(window: &AppWindow) {
    // Invalidate any in-flight run's modal writes before resetting.
    bump_generation();
    *SESSION.lock().unwrap() = Session::default();

    let state = window.global::<PlaylistImportState>();
    state.set_url("".into());
    state.set_custom_name("".into());
    state.set_loading(false);
    state.set_error("".into());
    state.set_active_provider("".into());
    state.set_can_fetch(false);
    state.set_show_preview(false);
    state.set_import_completed(false);
    state.set_progress_visible(false);
    state.set_has_progress(false);
    state.set_progress(0.0);
    state.set_status_line("".into());
    state.set_current_track("".into());
    state.set_log(ModelRc::new(VecModel::from(Vec::<ImportLogEntry>::new())));
    clear_summary(window);

    // Folder dropdown from the sidebar's folder list — the exact
    // create-playlist builder pattern: index 0 = "No folder" (id "").
    let folders = window.global::<SidebarState>().get_folders();
    let mut opts: Vec<slint::SharedString> = vec![qbz_i18n::t("No folder").into()];
    let mut ids: Vec<slint::SharedString> = vec!["".into()];
    for i in 0..folders.row_count() {
        if let Some(f) = folders.row_data(i) {
            opts.push(f.name);
            ids.push(f.id);
        }
    }
    state.set_folder_options(ModelRc::new(VecModel::from(opts)));
    state.set_folder_ids(ModelRc::new(VecModel::from(ids)));
    state.set_folder_index(0);

    state.set_open(true);
}

/// Recompute the URL-derived properties on every keystroke (Svelte's
/// derived detectedProvider / activeProvider / isValid / showPreview),
/// plus the post-completion fresh-import reset path. Event-loop thread.
pub fn on_url_edited(window: &AppWindow, text: &str) {
    let state = window.global::<PlaylistImportState>();
    let trimmed = text.trim();
    let detected = detect_provider_key(text);

    let mut s = SESSION.lock().unwrap();

    // After a completed import, editing the URL away from the imported
    // one rearms the modal for a fresh import without reopening.
    if state.get_import_completed() && trimmed != s.last_imported_url {
        s.locked_provider = None;
        state.set_import_completed(false);
        state.set_error("".into());
        state.set_log(ModelRc::new(VecModel::from(Vec::<ImportLogEntry>::new())));
        state.set_progress_visible(false);
        state.set_has_progress(false);
        state.set_progress(0.0);
        state.set_status_line("".into());
        state.set_current_track("".into());
        clear_summary(window);
    }

    let active = s.locked_provider.or(detected);
    state.set_active_provider(active.map(|p| p.as_str()).unwrap_or("").into());
    state.set_can_fetch(detected.is_some() && !crate::offline_mode::engine().is_offline());
    state.set_show_preview(s.preview.is_some() && trimmed == s.preview_url);
}

/// Keep the session's rename mirror fresh (read back by
/// [`begin_execute`]). Fired on every name-LineEdit keystroke.
pub fn on_name_edited(text: &str) {
    SESSION.lock().unwrap().custom_name = text.to_string();
}

/// Step A gate + reset (Svelte handlePreview's pre-invoke block). Returns
/// the URL to fetch, or None when the gate fails. Event-loop thread.
pub fn begin_fetch(window: &AppWindow) -> Option<String> {
    let state = window.global::<PlaylistImportState>();
    if state.get_loading() || !state.get_can_fetch() {
        return None;
    }
    let url = state.get_url().to_string();
    let detected = detect_provider_key(&url)?;
    {
        let mut s = SESSION.lock().unwrap();
        s.preview = None;
        s.preview_url.clear();
        s.locked_provider = Some(detected);
    }
    state.set_loading(true);
    state.set_error("".into());
    state.set_show_preview(false);
    state.set_import_completed(false);
    state.set_active_provider(detected.as_str().into());
    state.set_has_progress(false);
    state.set_progress(0.0);
    state.set_status_line("".into());
    state.set_current_track("".into());
    clear_summary(window);
    state.set_log(ModelRc::new(VecModel::from(Vec::<ImportLogEntry>::new())));
    push_log(window, qbz_i18n::t("Checking playlist link..."), "info");
    state.set_progress_visible(true);
    Some(url)
}

/// Preview fetch succeeded (Svelte handlePreview's try arm). Event-loop.
pub fn apply_preview_ok(window: &AppWindow, url: &str, preview: ImportPlaylist) {
    let state = window.global::<PlaylistImportState>();
    let count = preview.tracks.len();
    let provider = provider_display_name(&preview.provider);
    state.set_custom_name(preview.name.as_str().into());
    {
        let mut s = SESSION.lock().unwrap();
        s.custom_name = preview.name.clone();
        s.preview_url = url.trim().to_string();
        s.preview = Some(preview);
    }
    push_log(
        window,
        qbz_i18n::t_args("Found {} tracks from {}.", &[&count.to_string(), provider]),
        "success",
    );
    state.set_loading(false);
    // The URL input is disabled during the fetch, so it still equals the
    // fetched URL — step B (rename + Import) becomes visible.
    state.set_show_preview(
        state.get_url().trim() == SESSION.lock().unwrap().preview_url.as_str(),
    );
}

/// Preview fetch failed (Svelte handlePreview's catch arm). Event-loop.
pub fn apply_preview_err(window: &AppWindow, err: &str) {
    let state = window.global::<PlaylistImportState>();
    state.set_error(err.into());
    push_log(window, qbz_i18n::t_args("Import failed: {}", &[err]), "error");
    state.set_loading(false);
}

/// Everything the execute task needs, snapshotted on the event loop.
pub struct ExecuteArgs {
    pub url: String,
    pub name_override: Option<String>,
    /// Local folder id chosen in the dropdown ("" = no folder).
    pub folder_id: String,
    /// The run's generation (§1.8), carried by the sink and the
    /// completion arms.
    pub generation: u64,
}

/// Step B gate + reset (Svelte handleExecute's pre-invoke block).
/// Event-loop thread.
pub fn begin_execute(window: &AppWindow) -> Option<ExecuteArgs> {
    let state = window.global::<PlaylistImportState>();
    if state.get_loading() || state.get_import_completed() {
        return None;
    }
    let (url, name_override) = {
        let mut s = SESSION.lock().unwrap();
        let source_name = s.preview.as_ref()?.name.clone();
        // Rename goes out only when it differs from the source name; an
        // empty rename falls back to the source name (Appendix A).
        let custom = s.custom_name.trim().to_string();
        let name_override = if custom != source_name && !custom.is_empty() {
            Some(custom)
        } else {
            None
        };
        s.last_logged_percent = -1;
        (s.preview_url.clone(), name_override)
    };
    let folder_id = {
        let ids = state.get_folder_ids();
        ids.row_data(state.get_folder_index() as usize)
            .map(|s| s.to_string())
            .unwrap_or_default()
    };
    state.set_loading(true);
    state.set_error("".into());
    state.set_progress_visible(true);
    Some(ExecuteArgs {
        url,
        name_override,
        folder_id,
        generation: bump_generation(),
    })
}

/// One sink event onto the modal — the two Svelte event listeners. The
/// generation is checked by the caller (SlintSink). Event-loop thread.
pub fn apply_event(window: &AppWindow, event: ImportEvent) {
    let state = window.global::<PlaylistImportState>();
    match event {
        ImportEvent::Phase(phase) => match phase {
            ImportPhase::Matching => {
                push_log(window, qbz_i18n::t("Searching Qobuz catalog..."), "info");
            }
            // Creating / Adding re-fire once per created part — log each,
            // as Tauri does.
            ImportPhase::Creating => {
                push_log(window, qbz_i18n::t("Creating playlist..."), "success");
            }
            ImportPhase::Adding => {
                push_log(window, qbz_i18n::t("Adding tracks to playlist..."), "info");
            }
        },
        ImportEvent::Progress(p) => {
            // Bar + status update on EVERY event (Tauri parity, no
            // coalescing).
            state.set_has_progress(p.total > 0);
            if p.total > 0 {
                state.set_progress(p.current as f32 / p.total as f32);
            }
            if p.phase == "adding" {
                // Status line per phase — deliberate owner deviation from
                // the Tauri modal, which reused the "Matching tracks…"
                // string here (see qbz_playlist_import::sink::ImportPhase).
                let line = qbz_i18n::t_args("Adding tracks: {} / {}", &[&p.current.to_string(), &p.total.to_string()]);
                state.set_status_line(line.as_str().into());
                // One log line per 50-track chunk event (chunk counts,
                // not tracks) — Tauri logs every adding event.
                push_log(window, line, "info");
            } else if p.total > 0 {
                let line = qbz_i18n::t_args(
                    "Matching tracks: {} / {} ({} found)",
                    &[
                        &group_thousands(p.current),
                        &group_thousands(p.total),
                        &group_thousands(p.matched_so_far),
                    ],
                );
                state.set_status_line(line.as_str().into());
                // Matching is high-frequency (one event per track): log
                // only at 5% milestones, exactly like the Svelte listener.
                let pct = (p.current as u64 * 100 / p.total as u64) as i32;
                let should_log = {
                    let mut s = SESSION.lock().unwrap();
                    if pct >= s.last_logged_percent + 5 {
                        s.last_logged_percent = pct;
                        true
                    } else {
                        false
                    }
                };
                if should_log {
                    push_log(window, line, "info");
                }
            }
            state.set_current_track(p.current_track.unwrap_or_default().into());
        }
    }
}

/// Import finished (Svelte handleExecute's success arm): completion logs
/// + summary block + import-completed. Toast / sidebar refresh /
/// navigation live in the main.rs arm (§1.8: those fire even after
/// close; this fn is generation-guarded by the caller). Event-loop.
pub fn apply_execute_ok(window: &AppWindow, summary: &ImportSummary) {
    let state = window.global::<PlaylistImportState>();
    {
        let mut s = SESSION.lock().unwrap();
        s.last_imported_url = s.preview_url.clone();
    }
    state.set_import_completed(true);
    push_log(
        window,
        qbz_i18n::t_args(
            "Imported {} of {} tracks into QBZ.",
            &[
                &summary.matched_tracks.to_string(),
                &summary.total_tracks.to_string(),
            ],
        ),
        "success",
    );
    if !summary.qobuz_playlist_ids.is_empty() {
        if summary.parts_created > 1 {
            push_log(window, parts_line(summary.parts_created), "success");
        } else {
            push_log(window, qbz_i18n::t("Playlist created in Qobuz™."), "success");
        }
    } else {
        push_log(window, qbz_i18n::t("No matching tracks found."), "error");
    }
    // Summary block (pre-formatted; "" = hidden). `playlist_name` is the
    // name the playlist was created under — rename included (deliberate
    // owner fix vs the Tauri original, see qbz_playlist_import::importer).
    state.set_summary_playlist(qbz_i18n::t_args("Playlist: {}", &[&summary.playlist_name]).into());
    state.set_summary_matched(
        qbz_i18n::t_args(
            "Tracks matched: {} / {}",
            &[
                &summary.matched_tracks.to_string(),
                &summary.total_tracks.to_string(),
            ],
        )
        .into(),
    );
    state.set_summary_skipped(qbz_i18n::t_args("Skipped: {}", &[&summary.skipped_tracks.to_string()]).into());
    state.set_summary_parts(if summary.parts_created > 1 {
        parts_line(summary.parts_created).into()
    } else {
        "".into()
    });
    // The bar/status hide with loading, as in Tauri (`loading` gates the
    // bar there).
    state.set_has_progress(false);
    state.set_status_line("".into());
    state.set_current_track("".into());
    state.set_loading(false);
}

/// Import failed (Svelte handleExecute's catch arm). The error toast
/// lives in the main.rs arm. Event-loop thread.
pub fn apply_execute_err(window: &AppWindow, err: &str) {
    let state = window.global::<PlaylistImportState>();
    state.set_error(err.into());
    push_log(window, qbz_i18n::t_args("Import failed: {}", &[err]), "error");
    state.set_has_progress(false);
    state.set_status_line("".into());
    state.set_current_track("".into());
    state.set_loading(false);
}

/// Streams crate events onto the modal via the established
/// `upgrade_in_event_loop` cross-thread hop — one hop per event, the same
/// frequency profile as the artwork/scan pipelines (Tauri also updated
/// the bar per event, no coalescing).
pub struct SlintSink {
    weak: slint::Weak<AppWindow>,
    generation: u64,
}

impl SlintSink {
    pub fn new(weak: slint::Weak<AppWindow>, generation: u64) -> Self {
        Self { weak, generation }
    }
}

impl ImportProgressSink for SlintSink {
    fn emit(&self, event: ImportEvent) {
        let generation = self.generation;
        let _ = self.weak.upgrade_in_event_loop(move |w| {
            // Stale generation = the modal was reset (reopened) while
            // this run was in flight — its events must never touch the
            // fresh modal state (§1.8).
            if generation == current_generation() {
                apply_event(&w, event);
            }
        });
    }
}

/// Append one pre-formatted line to the conversion log (append-only
/// VecModel). Event-loop thread.
fn push_log(window: &AppWindow, message: String, status: &str) {
    let state = window.global::<PlaylistImportState>();
    let log = state.get_log();
    let entry = ImportLogEntry {
        message: message.into(),
        status: status.into(),
    };
    if let Some(vec) = log.as_any().downcast_ref::<VecModel<ImportLogEntry>>() {
        vec.push(entry);
    } else {
        // First write after the .slint default literal — swap in a
        // VecModel (open()/begin_fetch() normally install one already).
        let mut entries: Vec<ImportLogEntry> = log.iter().collect();
        entries.push(entry);
        state.set_log(ModelRc::new(VecModel::from(entries)));
    }
}

fn clear_summary(window: &AppWindow) {
    let state = window.global::<PlaylistImportState>();
    state.set_summary_playlist("".into());
    state.set_summary_matched("".into());
    state.set_summary_skipped("".into());
    state.set_summary_parts("".into());
}

/// "Split into {count} playlists (Qobuz 2000-track limit)" — used as both
/// a log line and the summary parts line, as in Tauri.
fn parts_line(count: u32) -> String {
    qbz_i18n::t_args("Split into {} playlists (Qobuz 2000-track limit)", &[&count.to_string()])
}

/// Display names for the "Found N tracks from {provider}." log (Svelte
/// formatProvider). The enum is exhaustive, so Svelte's "Unknown" arm is
/// unreachable here.
fn provider_display_name(provider: &ImportProvider) -> &'static str {
    match provider {
        ImportProvider::Spotify => "Spotify",
        ImportProvider::AppleMusic => "Apple Music",
        ImportProvider::Tidal => "Tidal",
        ImportProvider::Deezer => "Deezer",
    }
}

/// `toLocaleString()` twin for the matching log/status numbers
/// ("12,345"). Tauri rendered these with the user's locale; fixed en-US
/// grouping is the deliberate choice here.
fn group_thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::group_thousands;

    #[test]
    fn group_thousands_matches_to_locale_string() {
        assert_eq!(group_thousands(0), "0");
        assert_eq!(group_thousands(999), "999");
        assert_eq!(group_thousands(1000), "1,000");
        assert_eq!(group_thousands(12345), "12,345");
        assert_eq!(group_thousands(1234567), "1,234,567");
    }
}
