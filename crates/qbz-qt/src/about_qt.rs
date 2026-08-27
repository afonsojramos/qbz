//! About QBZ modal controller — the Qt port of `crates/qbz/src/about.rs`.
//!
//! Publishes ONE JSON document (`QbzAbout.aboutJson`) holding everything the
//! modal renders that is not a translated literal: the version, the platform
//! label, the build stamp, the release URL, the codename, the author chip and
//! the row-grouped contributor chips. The only async part is the GitHub
//! avatars, which are downloaded to the user cache dir and handed to QML as
//! `file://` paths — this port has NO precedent for an `https://` URL in
//! `Image.source` (every image goes through a Rust download-to-disk cache), so
//! the avatars follow that route rather than inventing a second one.
//!
//! # Divergences from the Slint reference, and why
//!
//! - **Platform label reads `"Linux (Qt)"`** (`about.rs:34-49` reads
//!   `"Linux (Slint)"`). The reference brands the FRONTEND on purpose — the
//!   Tauri build said `"(Tauri 2.0)"` — and this string ends up in bug
//!   reports, where knowing which frontend produced them matters.
//! - **The avatars are fetched on the FIRST OPEN, not at install.** The
//!   reference dispatches twelve unauthenticated `github.com/<handle>.png`
//!   requests at shell setup (`about.rs:126`), i.e. on every app start whether
//!   or not anyone ever opens the modal. There is no `install()` seam on this
//!   side to hang them from anyway, and deferring them costs nothing: the
//!   first open paints the blank circles the reference paints too, then fills
//!   them in as they land. Cached files make every later open instant.
//! - **`CONTRIBUTORS_PER_ROW` is 5** (`about.rs:76`). Both Slint comments that
//!   say "4 per row" (`state.slint:6091`, `AboutModal.slint:434`) are stale;
//!   the code is what ships. The rows are kept in the JSON even though QML has
//!   `Flow` and does not need pre-grouping, so both frontends read ONE document
//!   shape; `AboutModal.qml` flattens them.
//!
//! # The codename is a hand-set literal — a SECOND place to bump per release
//!
//! [`CODENAME`] mirrors `AboutModal.slint:371`. Neither is derived from
//! anything, so a release now updates two literals, not one. That is recorded
//! in the delivery doc; the alternative (a tiny shared crate both frontends
//! read) is a larger change and the owner's call.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};

use cxx_qt_lib::QString;
use serde::Serialize;

use crate::about_bridge::ui;

/// The real, displayed app version. `qbz-qt` inherits `version.workspace`, the
/// same source `crates/qbz` reads, so this is the REAL release version.
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Build date (`YYYY-MM-DD`) embedded by `build.rs`; empty if unavailable.
fn build_date() -> &'static str {
    env!("QBZ_BUILD_DATE")
}

/// Short git commit embedded by `build.rs`; empty in offline source builds.
fn build_commit() -> &'static str {
    env!("QBZ_BUILD_COMMIT")
}

/// The release codename shown in the build-info grid.
///
/// HAND-SET, exactly like `AboutModal.slint:371`. Bump it with the version.
pub const CODENAME: &str = "Rebuild 破 (You Can (Not) Advance)";

/// Platform label for the build-info grid. This is the Qt port, so the label
/// reads "(Qt)" rather than the Slint build's "(Slint)".
fn platform_label() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macOS (Qt)"
    }
    #[cfg(target_os = "windows")]
    {
        "Windows (Qt)"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "Linux (Qt)"
    }
}

/// The app author's GitHub handle (the single Author chip).
const AUTHOR_HANDLE: &str = "vicrodh";

/// The contributor handles. Copied VERBATIM from `about.rs:58-70`, order
/// included: first the Tauri About modal's order, then the Slint-era
/// external-PR contributors — `hoyon` (classical "work" grouping, PR #536),
/// `mxnix` (Russian translation, PR #517), `TerminalTilt`, plus contributors
/// whose implementation was superseded by Qt while their solution shipped.
/// Do not reorder existing entries.
const CONTRIBUTORS: &[&str] = &[
    "vorce",
    "boxdot",
    "arminfelder",
    "afonsojramos",
    "GwendalBeaumont",
    "AdamArstall",
    "Vudgekek",
    "DoubleGate",
    "hoyon",
    "mxnix",
    "TerminalTilt",
    "Alexandre-Menigault",
    "MarkusAbtion",
];

/// How many contributor chips per wrap row (`about.rs:76`). Slint has no
/// flex-wrap and needs the pre-grouping; QML uses `Flow` and flattens these
/// again. The grouping stays in the document so both frontends read one shape.
const CONTRIBUTORS_PER_ROW: usize = 5;

static OPEN: AtomicBool = AtomicBool::new(false);
/// Guards the one-shot avatar dispatch (see the module header — first open,
/// not install).
static AVATARS_STARTED: AtomicBool = AtomicBool::new(false);
/// handle -> `file://` path of its cached avatar. Absent = blank circle.
static AVATARS: LazyLock<Mutex<HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Serialize)]
struct Chip {
    name: String,
    url: String,
    /// `file://` path, or "" while it has not landed (QML draws the blank
    /// circle, exactly like `AboutModal.slint:98-99`).
    avatar: String,
}

#[derive(Serialize)]
struct Doc {
    open: bool,
    version: &'static str,
    #[serde(rename = "platformLabel")]
    platform_label: &'static str,
    #[serde(rename = "buildDate")]
    build_date: &'static str,
    #[serde(rename = "buildCommit")]
    build_commit: &'static str,
    #[serde(rename = "releaseUrl")]
    release_url: String,
    codename: &'static str,
    #[serde(rename = "authorName")]
    author_name: &'static str,
    #[serde(rename = "authorUrl")]
    author_url: String,
    #[serde(rename = "authorAvatar")]
    author_avatar: String,
    #[serde(rename = "contributorRows")]
    contributor_rows: Vec<Vec<Chip>>,
}

fn profile_url(handle: &str) -> String {
    format!("https://github.com/{handle}")
}

fn avatar_of(handle: &str) -> String {
    AVATARS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(handle)
        .cloned()
        .unwrap_or_default()
}

fn snapshot_doc() -> Doc {
    let version = app_version();
    Doc {
        open: OPEN.load(Ordering::SeqCst),
        version,
        platform_label: platform_label(),
        build_date: build_date(),
        build_commit: build_commit(),
        release_url: format!("https://github.com/vicrodh/qbz/releases/tag/v{version}"),
        codename: CODENAME,
        author_name: AUTHOR_HANDLE,
        author_url: profile_url(AUTHOR_HANDLE),
        author_avatar: avatar_of(AUTHOR_HANDLE),
        contributor_rows: CONTRIBUTORS
            .chunks(CONTRIBUTORS_PER_ROW)
            .map(|chunk| {
                chunk
                    .iter()
                    .map(|handle| Chip {
                        name: (*handle).to_string(),
                        url: profile_url(handle),
                        avatar: avatar_of(handle),
                    })
                    .collect()
            })
            .collect(),
    }
}

pub fn publish() {
    let json = serde_json::to_string(&snapshot_doc()).unwrap_or_else(|_| "{}".into());
    ui(move |mut b| b.as_mut().set_about_json(QString::from(json.as_str())));
}

pub fn open() {
    OPEN.store(true, Ordering::SeqCst);
    publish();
    start_avatar_loads();
}

pub fn close() {
    OPEN.store(false, Ordering::SeqCst);
    publish();
}

// ---------------------------------------------------------------------------
// Avatars
// ---------------------------------------------------------------------------

/// `<cache>/qbz/gh-avatars/` — beside the artwork cache, not inside it: the
/// artwork cache is a SQLite-keyed store for catalog art with its own eviction,
/// and twelve fixed 64px PNGs have nothing to do with it.
fn avatar_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("qbz").join("gh-avatars"))
}

/// The GitHub avatar URL for a handle (64px PNG, matching the reference).
fn avatar_url(handle: &str) -> String {
    format!("https://github.com/{handle}.png?size=64")
}

/// Resolve one avatar to a `file://` path, downloading it if the cache misses.
/// `None` on any network / decode / IO failure — the caller leaves the chip's
/// blank circle in place, no crash and no retry.
async fn ensure_avatar(client: &reqwest::Client, handle: &str) -> Option<String> {
    let dir = avatar_dir()?;
    let out = dir.join(format!("{handle}.png"));
    // A zero-byte file is a failed write that would otherwise count as a hit
    // forever (the artwork cache's documented trap) — treat it as a miss.
    if std::fs::metadata(&out)
        .map(|m| m.len() > 0)
        .unwrap_or(false)
    {
        return Some(crate::artwork_qt::file_url(&out.to_string_lossy()));
    }
    std::fs::create_dir_all(&dir).ok()?;

    let resp = client.get(avatar_url(handle)).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    let img = image::load_from_memory(&bytes).ok()?.thumbnail(64, 64);
    // ATOMIC: encode into a unique sibling, then rename — a half-written PNG
    // under the final name is a permanently broken chip (artwork_qt.rs:594).
    let tmp = dir.join(format!(".{handle}.{}.png", std::process::id()));
    if img.save_with_format(&tmp, image::ImageFormat::Png).is_err()
        || std::fs::rename(&tmp, &out).is_err()
    {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    Some(crate::artwork_qt::file_url(&out.to_string_lossy()))
}

/// Fetch every avatar (author + contributors) once, off the UI thread, and
/// republish the document as each one lands.
fn start_avatar_loads() {
    if AVATARS_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        // GitHub rejects requests without one.
        .user_agent("qbz")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[qbz-qt] about avatar client build failed: {e}");
            return;
        }
    };

    for handle in std::iter::once(&AUTHOR_HANDLE).chain(CONTRIBUTORS.iter()) {
        let client = client.clone();
        let handle = handle.to_string();
        crate::spawn(async move {
            if let Some(path) = ensure_avatar(&client, &handle).await {
                // The guard is scoped EXPLICITLY: `publish()` re-locks
                // AVATARS through `avatar_of`, and std's Mutex is not
                // re-entrant — holding it across the call would hard-hang the
                // download task on itself.
                {
                    AVATARS
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .insert(handle, path);
                }
                publish();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contributor_rows_are_chunks_of_five_in_source_order() {
        let doc = snapshot_doc();
        assert_eq!(doc.contributor_rows.len(), 3);
        assert_eq!(doc.contributor_rows[0].len(), 5);
        assert_eq!(doc.contributor_rows[1].len(), 5);
        assert_eq!(doc.contributor_rows[2].len(), 3);
        assert_eq!(doc.contributor_rows[0][0].name, "vorce");
        assert_eq!(doc.contributor_rows[2][0].name, "TerminalTilt");
        assert_eq!(doc.contributor_rows[2][1].name, "Alexandre-Menigault");
        assert_eq!(doc.contributor_rows[2][2].name, "MarkusAbtion");
        assert_eq!(
            doc.contributor_rows[2][2].url,
            "https://github.com/MarkusAbtion"
        );
    }

    #[test]
    fn release_url_is_the_v_prefixed_tag_for_the_running_version() {
        let doc = snapshot_doc();
        assert_eq!(
            doc.release_url,
            format!(
                "https://github.com/vicrodh/qbz/releases/tag/v{}",
                app_version()
            )
        );
    }

    #[test]
    fn platform_label_brands_the_frontend() {
        assert!(platform_label().ends_with("(Qt)"), "{}", platform_label());
    }
}
