//! About modal controller.
//!
//! Seeds the static `AboutState` fields (version, platform label, build date +
//! commit, the release URL and the contributor list) and wires `AboutActions`.
//! The text fields are static for the lifetime of the app; the one async bit is
//! the GitHub avatars (author + contributors), fetched off the UI thread and
//! painted onto their chips as they arrive. `install` is a one-shot seed +
//! callback wire + avatar dispatch, called once at shell setup.
//!
//! App version: the `qbz` binary inherits `version.workspace = true` (currently
//! 1.2.15), so `env!("CARGO_PKG_VERSION")` is the REAL release version, not the
//! 0.1.0 the workspace pins for library crates. The diagnostics panel reads the
//! same source. Build date + commit come from `build.rs` (`QBZ_BUILD_*`).

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::{AboutContributorGroup, AboutContributorRow, AboutState, AboutActions, AppWindow};

/// The real, displayed app version (workspace package version, e.g. "1.2.15").
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

/// Platform label for the build-info grid. This is the Slint port, so the label
/// reads "(Slint)" rather than the Tauri build's "(Tauri 2.0)".
fn platform_label() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macOS (Slint)"
    }
    #[cfg(target_os = "windows")]
    {
        "Windows (Slint)"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "Linux (Slint)"
    }
}

/// The app author's GitHub handle (the single Author chip).
const AUTHOR_HANDLE: &str = "vicrodh";

/// The contributor handles + their GitHub profile URLs. First the Tauri About
/// modal's order, then the Slint-era external-PR contributors: `hoyon`
/// (classical "work" grouping, PR #536), `mxnix` (Russian translation,
/// PR #517) and `TerminalTilt`.
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
];

/// How many contributor chips per wrap row. Slint has no flex-wrap, so the flat
/// list is pre-grouped into fixed rows (see `AboutContributorGroup`). 5 fills
/// the widened ~840px panel instead of the old 4-per-row / 3-row layout that
/// left the wider modal half-empty.
const CONTRIBUTORS_PER_ROW: usize = 5;

/// Build the row-grouped contributor model. Avatars start blank (default image)
/// and are filled in async by `spawn_avatar_loads`.
fn build_contributor_groups() -> Vec<AboutContributorGroup> {
    CONTRIBUTORS
        .chunks(CONTRIBUTORS_PER_ROW)
        .map(|chunk| {
            let rows: Vec<AboutContributorRow> = chunk
                .iter()
                .map(|handle| AboutContributorRow {
                    name: (*handle).into(),
                    url: format!("https://github.com/{handle}").into(),
                    avatar: slint::Image::default(),
                })
                .collect();
            AboutContributorGroup {
                items: ModelRc::new(VecModel::from(rows)),
            }
        })
        .collect()
}

/// Seed the static fields, wire the open-url callback, and dispatch the avatar
/// fetches. Call once at shell setup. `handle` runs the avatar downloads off the
/// UI thread.
pub fn install(window: &AppWindow, handle: tokio::runtime::Handle) {
    let state = window.global::<AboutState>();

    let version = app_version();
    state.set_version(version.into());
    state.set_platform_label(platform_label().into());
    state.set_build_date(build_date().into());
    state.set_build_commit(build_commit().into());
    state.set_release_url(format!("https://github.com/vicrodh/qbz/releases/tag/v{version}").into());
    state.set_author_name(AUTHOR_HANDLE.into());
    state.set_author_url(format!("https://github.com/{AUTHOR_HANDLE}").into());

    state.set_contributor_rows(ModelRc::new(VecModel::from(build_contributor_groups())));

    window.global::<AboutActions>().on_open_url(|url| {
        let url = url.to_string();
        if url.is_empty() {
            return;
        }
        if let Err(e) = open::that(&url) {
            log::warn!("[qbz-slint] open About URL failed ({url}): {e}");
        }
    });

    spawn_avatar_loads(window.as_weak(), handle);
}

/// The GitHub avatar URL for a handle (64px PNG, matching the Tauri build).
fn avatar_url(handle: &str) -> String {
    format!("https://github.com/{handle}.png?size=64")
}

/// Fetch every GitHub avatar (author + contributors) off the UI thread and paint
/// each onto its own chip as it arrives. A failed fetch just leaves that chip's
/// blank circle in place — no crash, no retry.
fn spawn_avatar_loads(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .user_agent("qbz")
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[qbz-slint] about avatar client build failed: {e}");
            return;
        }
    };

    // Author avatar → its dedicated AboutState field.
    {
        let weak = weak.clone();
        let client = client.clone();
        let url = avatar_url(AUTHOR_HANDLE);
        handle.spawn(async move {
            if let Some((pixels, w, h)) = fetch_avatar(&client, &url).await {
                let _ = weak.upgrade_in_event_loop(move |win| {
                    let img = crate::artwork::pixels_to_image(&pixels, w, h);
                    win.global::<AboutState>().set_author_avatar(img);
                });
            }
        });
    }

    // Contributor avatars → addressed by (group, position) in the grouped model.
    for (idx, contributor) in CONTRIBUTORS.iter().enumerate() {
        let weak = weak.clone();
        let client = client.clone();
        let url = avatar_url(contributor);
        let group = idx / CONTRIBUTORS_PER_ROW;
        let pos = idx % CONTRIBUTORS_PER_ROW;
        handle.spawn(async move {
            if let Some((pixels, w, h)) = fetch_avatar(&client, &url).await {
                let _ = weak.upgrade_in_event_loop(move |win| {
                    let img = crate::artwork::pixels_to_image(&pixels, w, h);
                    let groups = win.global::<AboutState>().get_contributor_rows();
                    if let Some(grp) = groups.row_data(group) {
                        let items = grp.items.clone();
                        if let Some(mut row) = items.row_data(pos) {
                            row.avatar = img;
                            items.set_row_data(pos, row);
                        }
                    }
                });
            }
        });
    }
}

/// Fetch one GitHub avatar and decode it to RGBA8 (downscaled to 64px). `None`
/// on any network/decode failure — the caller leaves the blank circle.
async fn fetch_avatar(
    client: &reqwest::Client,
    url: &str,
) -> Option<(Vec<u8>, u32, u32)> {
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    let rgba = image::load_from_memory(&bytes).ok()?.thumbnail(64, 64).to_rgba8();
    let (w, h) = rgba.dimensions();
    Some((rgba.into_raw(), w, h))
}
