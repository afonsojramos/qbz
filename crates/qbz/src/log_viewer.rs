//! Log viewer controller (developer log overlay).
//!
//! Wires the `LogViewerState` Slint global to the `qbz_log` in-memory ring. The
//! viewer is a thin read surface over `qbz_log::ring`: `refresh` snapshots the
//! ring, applies the level + search filters, caps to the last 1000 rows, and
//! pushes `[LogRow]`. `clear` empties the ring; `set-level` / `set-search`
//! re-filter; `auto-tail` re-runs `refresh` every 1.5s via a `slint::Timer`.
//! `copy-all` copies the currently-filtered rows; `copy-bundle` builds a
//! GitHub-ready diagnostics bundle; `upload` POSTs that bundle to paste.rs and
//! surfaces the returned URL; `open-log-file` opens the on-disk log.
//!
//! All log text is redacted at the ring's write choke point; clipboard/upload
//! paths redact again defensively.

use std::sync::Arc;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::adapter::SlintAdapter;
use crate::{AppWindow, LogRow, LogViewerState};

type Runtime = Arc<qbz_app::shell::AppRuntime<SlintAdapter>>;

/// Maximum rows pushed to the viewer after filtering (the ring holds up to
/// `qbz_log::ring::RING_CAP`; the view shows the most recent slice).
const MAX_VIEW_ROWS: usize = 1000;

/// Auto-tail refresh cadence.
const AUTO_TAIL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1500);

thread_local! {
    /// The auto-tail timer. UI-thread only (slint::Timer requirement); started /
    /// stopped from the `toggle-auto-tail` callback (always on the UI thread).
    static AUTO_TAIL_TIMER: slint::Timer = slint::Timer::default();
}

/// Wire every `LogViewerState` callback. Call once at shell setup. `runtime` is
/// used by the "Copy diagnostics bundle" / "Upload" paths to gather the COMPLETE
/// diagnostics report (system + live audio + graphics + playback + qconnect).
pub fn install(window: &AppWindow, runtime: Runtime, handle: tokio::runtime::Handle) {
    let state = window.global::<LogViewerState>();

    {
        let weak = window.as_weak();
        state.on_refresh(move || rebuild(&weak));
    }
    {
        let weak = window.as_weak();
        state.on_clear(move || {
            qbz_log::ring::clear();
            rebuild(&weak);
        });
    }
    {
        let weak = window.as_weak();
        // The new value is already stored in the in-out `filter-level`; rebuild
        // reads it back. Same for search.
        state.on_set_level(move |_level| rebuild(&weak));
    }
    {
        let weak = window.as_weak();
        state.on_set_search(move |_search| rebuild(&weak));
    }
    {
        let weak = window.as_weak();
        state.on_toggle_auto_tail(move |on| {
            if let Some(w) = weak.upgrade() {
                w.global::<LogViewerState>().set_auto_tail(on);
            }
            AUTO_TAIL_TIMER.with(|timer| {
                if on {
                    let weak = weak.clone();
                    timer.start(slint::TimerMode::Repeated, AUTO_TAIL_INTERVAL, move || {
                        rebuild(&weak);
                    });
                } else {
                    timer.stop();
                }
            });
        });
    }
    {
        let weak = window.as_weak();
        let handle = handle.clone();
        state.on_copy_all(move || {
            let text = filtered_text(&weak).join("\n");
            crate::share::copy_to_clipboard(text);
            flash_copied(&weak, &handle);
        });
    }
    {
        let weak = window.as_weak();
        let handle = handle.clone();
        let runtime = runtime.clone();
        state.on_copy_bundle(move || {
            let weak = weak.clone();
            let runtime = runtime.clone();
            handle.spawn(async move {
                let bundle = build_share_text(&runtime).await;
                crate::share::copy_to_clipboard(bundle);
                let _ = weak.upgrade_in_event_loop(|w| {
                    w.global::<LogViewerState>().set_copied(true);
                });
                tokio::time::sleep(AUTO_TAIL_INTERVAL).await;
                let _ = weak.upgrade_in_event_loop(|w| {
                    w.global::<LogViewerState>().set_copied(false);
                });
            });
        });
    }
    {
        let weak = window.as_weak();
        let handle = handle.clone();
        let runtime = runtime.clone();
        state.on_upload(move || {
            if let Some(w) = weak.upgrade() {
                w.global::<LogViewerState>().set_uploading(true);
            }
            let weak = weak.clone();
            let runtime = runtime.clone();
            handle.spawn(async move {
                let bundle = build_share_text(&runtime).await;
                let url = match reqwest::Client::new()
                    .post("https://paste.rs/")
                    .body(bundle)
                    .send()
                    .await
                {
                    Ok(resp) => resp.text().await.unwrap_or_default().trim().to_string(),
                    Err(e) => {
                        log::warn!("[qbz-slint] log upload failed: {e}");
                        String::new()
                    }
                };
                let _ = weak.upgrade_in_event_loop(move |w| {
                    let st = w.global::<LogViewerState>();
                    st.set_uploaded_url(url.into());
                    st.set_uploading(false);
                });
            });
        });
    }
    {
        state.on_open_log_file(move || {
            if let Some(path) = qbz_log::install::log_file_path() {
                if let Err(e) = open::that(path) {
                    log::warn!("[qbz-slint] open log file failed: {e}");
                }
            }
        });
    }
    {
        let weak = window.as_weak();
        state.on_copy_url(move || {
            if let Some(w) = weak.upgrade() {
                let url = w.global::<LogViewerState>().get_uploaded_url().to_string();
                if !url.is_empty() {
                    crate::share::copy_to_clipboard(url);
                }
            }
        });
    }
}

/// Whether `line` passes the level + search filters currently set on the global.
/// `level` is the lowercased `filter-level` ("all" = no level filter); `search`
/// is the lowercased query (empty = no search filter), matched over target +
/// message.
fn line_matches(line: &qbz_log::LogLine, level: &str, search: &str) -> bool {
    let level_ok = level == "all" || line.level_str().eq_ignore_ascii_case(level);
    let search_ok = search.is_empty()
        || line.target.to_lowercase().contains(search)
        || line.message.to_lowercase().contains(search);
    level_ok && search_ok
}

/// Snapshot + filter the ring, cap to the last [`MAX_VIEW_ROWS`], and push the
/// rows + counters onto `LogViewerState`. Runs on the UI thread.
fn rebuild(weak: &slint::Weak<AppWindow>) {
    let Some(w) = weak.upgrade() else {
        return;
    };
    let st = w.global::<LogViewerState>();
    let level = st.get_filter_level().to_string().to_lowercase();
    let search = st.get_search().to_string().to_lowercase();

    let snap = qbz_log::ring::snapshot();
    let total = snap.len();
    let filtered: Vec<&qbz_log::LogLine> = snap
        .iter()
        .filter(|line| line_matches(line, &level, &search))
        .collect();
    let start = filtered.len().saturating_sub(MAX_VIEW_ROWS);
    let rows: Vec<LogRow> = filtered[start..]
        .iter()
        .map(|line| LogRow {
            ts: line.format_ts().into(),
            level: line.level_str().into(),
            target: line.target.clone().into(),
            message: line.message.clone().into(),
        })
        .collect();
    let shown = rows.len();

    st.set_rows(ModelRc::new(VecModel::from(rows)));
    st.set_total(total as i32);
    st.set_shown(shown as i32);
}

/// The currently-filtered rows as redacted `"{ts} {level} {target} {message}"`
/// lines (last [`MAX_VIEW_ROWS`]). Used by `copy-all`.
fn filtered_text(weak: &slint::Weak<AppWindow>) -> Vec<String> {
    let Some(w) = weak.upgrade() else {
        return Vec::new();
    };
    let st = w.global::<LogViewerState>();
    let level = st.get_filter_level().to_string().to_lowercase();
    let search = st.get_search().to_string().to_lowercase();

    let snap = qbz_log::ring::snapshot();
    let filtered: Vec<&qbz_log::LogLine> = snap
        .iter()
        .filter(|line| line_matches(line, &level, &search))
        .collect();
    let start = filtered.len().saturating_sub(MAX_VIEW_ROWS);
    filtered[start..]
        .iter()
        .map(|line| {
            format!(
                "{} {} {} {}",
                line.format_ts(),
                line.level_str(),
                line.target,
                qbz_log::redact(&line.message)
            )
        })
        .collect()
}

/// Flash `copied = true` for the standard window, then reset on a tokio timer.
fn flash_copied(weak: &slint::Weak<AppWindow>, handle: &tokio::runtime::Handle) {
    if let Some(w) = weak.upgrade() {
        w.global::<LogViewerState>().set_copied(true);
    }
    let weak = weak.clone();
    handle.spawn(async move {
        tokio::time::sleep(AUTO_TAIL_INTERVAL).await;
        let _ = weak.upgrade_in_event_loop(|w| {
            w.global::<LogViewerState>().set_copied(false);
        });
    });
}

/// Build the COMPLETE shareable diagnostics text used by both "Copy diagnostics
/// bundle" and "Upload (public)": the full diagnostics report (system + the LIVE
/// active audio device + graphics + playback + qconnect) followed by the last 200
/// redacted log lines. This is what makes the uploaded paste complete rather than
/// "just logs". All log lines are already redacted at the ring's write choke
/// point; `qbz_log::redact` is applied again defensively.
async fn build_share_text(runtime: &Runtime) -> String {
    let report = crate::diagnostics::build_full_report(runtime).await;

    let lines = qbz_log::ring::snapshot();
    let start = lines.len().saturating_sub(200);
    let mut logs = String::new();
    for line in &lines[start..] {
        logs.push_str(&format!(
            "{} {} {} {}\n",
            line.format_ts(),
            line.level_str(),
            line.target,
            qbz_log::redact(&line.message)
        ));
    }

    format!("{report}\n\n## Recent logs\n\n```log\n{logs}```\n")
}
