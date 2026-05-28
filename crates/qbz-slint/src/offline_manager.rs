//! Offline Cache Manager controller — loads the cached-tracks rollup + stats
//! into `OfflineManagerState`. Per-item actions reuse `offline_cache::*`
//! (Slice 3); this module owns the data load + the size-limit edit.

use std::collections::BTreeMap;

use qbz_offline_cache::{CachedTrackInfo, OfflineCacheStatus};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{AppWindow, OfflineManagerState, OfflineRow};

const GB: u64 = 1024 * 1024 * 1024;

fn human_size(bytes: u64) -> String {
    let b = bytes as f64;
    if bytes >= GB {
        format!("{:.1} GB", b / GB as f64)
    } else if bytes >= 1024 * 1024 {
        format!("{:.0} MB", b / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KB", b / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn track_status_int(s: &OfflineCacheStatus) -> i32 {
    match s {
        OfflineCacheStatus::Ready => 3,
        OfflineCacheStatus::Failed => 4,
        _ => 2, // queued / downloading
    }
}

fn fmt_duration(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Read the index.db, build the album→track rollup + stats, and push them to
/// `OfflineManagerState`.
async fn rebuild(weak: slint::Weak<AppWindow>) {
    let off = crate::offline::get().await;
    let (tracks, limit): (Vec<CachedTrackInfo>, Option<u64>) = match off {
        Some(ref o) => {
            let limit = *o.limit_bytes.lock().await;
            let guard = o.db.lock().await;
            let tracks = guard
                .as_ref()
                .and_then(|db| db.get_all_tracks().ok())
                .unwrap_or_default();
            (tracks, limit)
        }
        None => (Vec::new(), None),
    };

    let total_size: u64 = tracks.iter().map(|t| t.file_size_bytes).sum();
    let tracks_count = tracks.len() as i32;

    // Group by album_id, remembering first-seen order.
    let mut order: Vec<String> = Vec::new();
    let mut groups: BTreeMap<String, (String, String, Vec<CachedTrackInfo>)> = BTreeMap::new();
    for t in tracks {
        let aid = t.album_id.clone().unwrap_or_else(|| "__singles__".to_string());
        if !groups.contains_key(&aid) {
            order.push(aid.clone());
        }
        let album = t.album.clone().unwrap_or_else(|| "Singles".to_string());
        groups
            .entry(aid)
            .or_insert_with(|| (album, t.artist.clone(), Vec::new()))
            .2
            .push(t);
    }

    let mut rows: Vec<OfflineRow> = Vec::new();
    for aid in &order {
        let (album, artist, group) = groups.get(aid).unwrap();
        let any_failed = group
            .iter()
            .any(|t| matches!(t.status, OfflineCacheStatus::Failed));
        let any_active = group.iter().any(|t| {
            matches!(
                t.status,
                OfflineCacheStatus::Queued | OfflineCacheStatus::Downloading
            )
        });
        let all_ready = group
            .iter()
            .all(|t| matches!(t.status, OfflineCacheStatus::Ready));
        let album_status = if any_failed {
            4
        } else if any_active {
            2
        } else if all_ready {
            3
        } else {
            0
        };
        let album_size: u64 = group.iter().map(|t| t.file_size_bytes).sum();
        rows.push(OfflineRow {
            kind: "album".into(),
            album_id: aid.clone().into(),
            track_id: SharedString::new(),
            title: album.clone().into(),
            subtitle: artist.clone().into(),
            meta: format!("{} tracks · {}", group.len(), human_size(album_size)).into(),
            status: album_status,
            progress: 0.0,
        });
        for t in group {
            rows.push(OfflineRow {
                kind: "track".into(),
                album_id: aid.clone().into(),
                track_id: t.track_id.to_string().into(),
                title: t.title.clone().into(),
                subtitle: t.artist.clone().into(),
                meta: fmt_duration(t.duration_secs).into(),
                status: track_status_int(&t.status),
                progress: t.progress_percent as f32 / 100.0,
            });
        }
    }

    let (limit_text, usage, limit_gb) = match limit {
        Some(l) if l > 0 => (
            format!("· of {}", human_size(l)),
            (total_size as f32 / l as f32).clamp(0.0, 1.0),
            (l / GB).max(1) as i32,
        ),
        _ => ("· Unlimited".to_string(), 0.0, 5),
    };
    let size_text = human_size(total_size);

    let _ = weak.upgrade_in_event_loop(move |w| {
        let st = w.global::<OfflineManagerState>();
        st.set_rows(ModelRc::new(VecModel::from(rows)));
        st.set_tracks_count(tracks_count);
        st.set_size_text(SharedString::from(size_text));
        st.set_limit_text(SharedString::from(limit_text));
        st.set_usage(usage);
        st.set_limit_gb(limit_gb);
        st.set_loading(false);
    });
}

/// Load (or refresh) the manager. Marks loading, then rebuilds.
pub fn load(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    let _ = weak.upgrade_in_event_loop(|w| {
        w.global::<OfflineManagerState>().set_loading(true);
    });
    handle.spawn(rebuild(weak));
}

/// Rebuild after a brief delay — used after a manager mutation (remove /
/// re-download) so the list reflects the async DB op without a manual refresh.
pub fn reload_soon(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle) {
    handle.spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        rebuild(weak).await;
    });
}

/// Set the in-memory cache size limit (GB) and refresh. (Session-scoped for
/// now — Slint-side persistence is a follow-up.)
pub fn set_limit(weak: slint::Weak<AppWindow>, handle: tokio::runtime::Handle, gb: i32) {
    handle.spawn(async move {
        if let Some(off) = crate::offline::get().await {
            *off.limit_bytes.lock().await = Some((gb.max(1) as u64) * GB);
        }
        rebuild(weak).await;
    });
}
