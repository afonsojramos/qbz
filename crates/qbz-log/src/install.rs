//! One-shot logger installation + the on-disk file sink (open / rotate).

use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::tee::TeeLogger;

static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Install the [`TeeLogger`] as the global `log` logger.
///
/// Builds the inner `env_logger` logger from `RUST_LOG` (falling back to `default_level`),
/// opens/rotates the on-disk file, then sets the boxed logger + max level. Idempotent:
/// a second call is a guarded no-op (it neither rotates the file again nor panics).
pub fn install(default_level: &str) {
    install_with_file_sink(default_level, true);
}

/// Same as [`install`], but with the on-disk file sink DISABLED (stderr + ring only).
///
/// For an internal, disposable child process that re-enters the same `main`.
/// [`open_log_file`] rotates `qbz.log` to `qbz.log.prev` and creates a fresh
/// file on every call, so a child running it MID-RUN renames the live log out
/// from under the parent — which keeps writing to the renamed inode — and
/// leaves `qbz.log` holding nothing but the child's own first lines.
///
/// Field-reported in #749: on Linux every launch spawns the presentation /
/// GPU preflight child, so `~/.local/share/qbz/logs/qbz.log` ended at two
/// lines on EVERY run while the app itself was healthy and its whole log went
/// to `qbz.log.prev`. The user's log file, the first thing a bug report
/// attaches, was structurally useless. A child's output is already captured
/// by the parent (proof line on stdout, `preflight_output_tail` on failure),
/// so it loses nothing by staying off the file.
pub fn install_without_file_sink(default_level: &str) {
    install_with_file_sink(default_level, false);
}

fn install_with_file_sink(default_level: &str, file_sink: bool) {
    // True one-shot guard: avoid re-rotating the log file or fighting an already-set logger.
    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }

    // Demote chatty foreign crates in the DEFAULT filter: zbus 5 logs every
    // D-Bus message it dispatches/reads as multi-KB Debug dumps at INFO (via
    // the tracing-log bridge, which also emits `tracing::span` events). On a
    // desktop with an MPRIS applet polling GetAll this flooded the file sink
    // within ~1s of startup and drowned real entries (field-confirmed twice
    // in #555 logs) — and each suppressed record now costs nothing, since
    // `log!` checks the filter before formatting. An explicit RUST_LOG still
    // replaces the whole default, so full zbus tracing stays one env var away.
    let inner = env_logger::Builder::from_env(
        env_logger::Env::default()
            .default_filter_or(format!("{default_level},zbus=warn,tracing=warn")),
    )
    .build();
    let level = inner.filter();
    let file = if file_sink { open_log_file() } else { None };

    // Ignore the Err if a logger was somehow already set elsewhere.
    if log::set_boxed_logger(Box::new(TeeLogger::new(inner, file))).is_ok() {
        log::set_max_level(level);
    }
}

/// Runtime log-level toggle (e.g. info <-> debug) with no restart.
pub fn set_level(level: log::LevelFilter) {
    log::set_max_level(level);
}

/// Path to the current-run log file (`~/.local/share/qbz/logs/qbz.log`), if a data dir exists.
pub fn log_file_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join("qbz").join("logs").join("qbz.log"))
}

/// Open the log file for this run, rotating any previous one to `qbz.log.prev`.
/// Returns `None` (file sink disabled, gracefully) on any filesystem error.
fn open_log_file() -> Option<BufWriter<File>> {
    open_log_file_at(&log_file_path()?)
}

/// [`open_log_file`] against an explicit path, so the rotation contract is
/// testable without a real data dir.
///
/// NOTE the hazard this encodes: every call renames the CURRENT file away and
/// starts an empty one. That is right once per process run and wrong for
/// anything else — see [`install_without_file_sink`].
fn open_log_file_at(path: &std::path::Path) -> Option<BufWriter<File>> {
    let dir = path.parent()?;
    std::fs::create_dir_all(dir).ok()?;

    if path.exists() {
        let prev = dir.join("qbz.log.prev");
        // Best-effort rotation; a failure here must not disable logging.
        let _ = std::fs::rename(path, &prev);
    }

    let file = File::create(path).ok()?;
    Some(BufWriter::new(file))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// The rotation contract, and the reason a child process must never run it:
    /// a second open renames the live file to `qbz.log.prev` and leaves an
    /// empty `qbz.log` behind. This is #749 in three lines.
    #[test]
    fn a_second_open_rotates_the_live_file_away() {
        let dir = std::env::temp_dir().join(format!("qbz-log-rotate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("qbz.log");

        let mut first = open_log_file_at(&path).expect("first open");
        writeln!(first, "parent line").unwrap();
        first.flush().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "parent line\n");

        // A second process (the preflight child) installing its own logger.
        let mut second = open_log_file_at(&path).expect("second open");
        writeln!(second, "child line").unwrap();
        second.flush().unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "child line\n");
        assert_eq!(
            std::fs::read_to_string(dir.join("qbz.log.prev")).unwrap(),
            "parent line\n",
            "the parent's log survives only under .prev"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
