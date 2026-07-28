//! Queue panel controller — Slint-free port of the `QueueState` assembly
//! side of `crates/qbz/src/queue.rs`: NOW PLAYING card, UP NEXT with the
//! #442 "Next in queue" / "Next up" section markers (from the core's
//! `manual_next_count`), 40-row pagination, live search filter, and the
//! History tab. Mutations go straight to the core queue API; every one
//! ends with a republish.
//!
//! POC-NOTEs:
//! - QConnect remote reorder, the playlist picker (save-as-playlist opens
//!   a modal picker upstream), infinite-play engine, sleep timer, stop-after
//!   marker, ephemeral rows, coverflow: out of scope.
//! - Row favorite seeds from the phase-5 library feed (Slint uses
//!   fav_cache — same truth).

use std::sync::Mutex;

use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use cxx_qt_lib::QString;
use qbz_models::QueueTrack;
use serde::Serialize;
use std::sync::Arc;

/// queue.rs PAGE_SIZE.
pub const PAGE_SIZE: usize = 40;

#[derive(Clone, Default, Serialize)]
pub struct QueueRow {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    pub album: String,
    pub duration: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    pub explicit: bool,
    #[serde(rename = "artUrl")]
    pub art_url: String,
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
    /// #442 section header drawn ABOVE this row: "" | "next-in-queue" |
    /// "next-up" (only on the unfiltered list).
    pub section: String,
}

#[derive(Default, Serialize)]
pub struct QueueDoc {
    #[serde(rename = "hasCurrent")]
    pub has_current: bool,
    pub current: Option<QueueRow>,
    pub upcoming: Vec<QueueRow>,
    #[serde(rename = "upcomingTotal")]
    pub upcoming_total: usize,
    #[serde(rename = "upcomingRemaining")]
    pub upcoming_remaining: usize,
    pub history: Vec<QueueRow>,
    pub page: usize,
    #[serde(rename = "pageCount")]
    pub page_count: usize,
    #[serde(rename = "pageStart")]
    pub page_start: usize,
    #[serde(rename = "pageEnd")]
    pub page_end: usize,
    pub shuffle: bool,
    #[serde(rename = "repeatMode")]
    pub repeat_mode: i32,
}

/// Panel view state (search + page), session-scope like the Slint view.
#[derive(Default)]
struct ViewState {
    search: String,
    page: usize,
}

static VIEW: Mutex<ViewState> = Mutex::new(ViewState {
    search: String::new(),
    page: 0,
});

/// queue.rs `display_title` (version suffix).
fn display_title(track: &QueueTrack) -> String {
    match track.version.as_deref().filter(|v| !v.is_empty()) {
        Some(version) => format!("{} ({version})", track.title),
        None => track.title.clone(),
    }
}

fn fmt_duration(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn row_from(track: &QueueTrack) -> QueueRow {
    let is_favorite = crate::library_qt::with_library(|d| {
        d.feed
            .iter()
            .any(|i| i.kind == "track" && i.id == track.id.to_string() && i.is_favorite)
    })
    .unwrap_or(false);
    let tier = match track.bit_depth {
        Some(d) if d >= 24 => "hires",
        Some(_) => "cd",
        None if track.hires => "hires",
        None => "",
    }
    .to_string();
    QueueRow {
        id: track.id.to_string(),
        title: display_title(track),
        artist: track.artist.clone(),
        artist_id: track.artist_id.map(|id| id.to_string()).unwrap_or_default(),
        album: track.album.clone(),
        duration: fmt_duration(track.duration_secs),
        quality_tier: tier,
        quality_detail: crate::home_qt::quality_detail_from_parts(
            track.bit_depth,
            track.sample_rate,
        ),
        explicit: track.parental_warning,
        art_url: track.artwork_url.clone().unwrap_or_default(),
        is_favorite,
        section: String::new(),
    }
}

/// queue.rs `paginate` (unit-tested upstream; port).
fn paginate(total: usize, requested_page: usize) -> (usize, usize, usize, usize) {
    let page_count = total.div_ceil(PAGE_SIZE).max(1);
    let page = requested_page.min(page_count - 1);
    let start = page * PAGE_SIZE;
    let end = (start + PAGE_SIZE).min(total);
    (page, page_count, start, end)
}

/// Build + publish the whole panel document.
pub async fn publish(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let state = runtime.core().get_queue_state_full().await;
    let (search, requested_page) = {
        let view = VIEW.lock().unwrap();
        (view.search.clone(), view.page)
    };
    let query = search.trim().to_lowercase();

    // --- UP NEXT (search-filtered, paged, #442 markers) ------------------
    let filtered: Vec<&QueueTrack> = if query.is_empty() {
        state.upcoming.iter().collect()
    } else {
        state
            .upcoming
            .iter()
            .filter(|t| {
                display_title(t).to_lowercase().contains(&query)
                    || t.artist.to_lowercase().contains(&query)
            })
            .collect()
    };
    let upcoming_total = filtered.len();
    let (page, page_count, start, end) = paginate(upcoming_total, requested_page);
    VIEW.lock().unwrap().page = page;

    let page_rows: Vec<QueueRow> = filtered[start..end]
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mut r = row_from(t);
            if query.is_empty() {
                let g = start + i;
                if state.manual_next_count > 0 && g == 0 {
                    r.section = "next-in-queue".into();
                } else if g == state.manual_next_count {
                    r.section = "next-up".into();
                }
            }
            r
        })
        .collect();

    let remaining = state
        .current_index
        .map(|idx| state.total_tracks.saturating_sub(idx + 1))
        .unwrap_or(state.total_tracks);

    let history: Vec<QueueRow> = state.history.iter().map(row_from).collect();
    let current = state.current_track.as_ref().map(row_from);

    let doc = QueueDoc {
        has_current: current.is_some(),
        current,
        upcoming: page_rows,
        upcoming_total,
        upcoming_remaining: remaining,
        history,
        page,
        page_count,
        page_start: if upcoming_total == 0 { 0 } else { start + 1 },
        page_end: end,
        shuffle: state.shuffle,
        repeat_mode: match state.repeat {
            qbz_models::RepeatMode::Off => 0,
            qbz_models::RepeatMode::All => 1,
            qbz_models::RepeatMode::One => 2,
        },
    };
    let json = serde_json::to_string(&doc).unwrap_or_else(|_| "{}".into());
    crate::queue_bridge::ui(move |mut b| {
        b.as_mut().set_queue_json(QString::from(json.as_str()));
    });
}

// ---------------------------------------------------------------------------
// View state + mutations (every mutation republishes)
// ---------------------------------------------------------------------------

pub fn set_search(query: &str) {
    {
        let mut view = VIEW.lock().unwrap();
        view.search = query.to_string();
        view.page = 0;
    }
}

pub fn set_page(page: i32) {
    VIEW.lock().unwrap().page = page.max(0) as usize;
}

pub async fn play_upcoming(runtime: &Arc<AppRuntime<LoggingAdapter>>, page_index: usize) {
    let start = VIEW.lock().unwrap().page * PAGE_SIZE;
    let upcoming_index = start + page_index;
    if let Some(track) = runtime.core().play_upcoming_at(upcoming_index).await {
        crate::playback_qt::play_queue_track_public(runtime, track.id).await;
    }
}

pub async fn remove_upcoming(runtime: &Arc<AppRuntime<LoggingAdapter>>, page_index: usize) {
    let start = VIEW.lock().unwrap().page * PAGE_SIZE;
    let upcoming_index = start + page_index;
    runtime.core().remove_upcoming_track(upcoming_index).await;
    publish(runtime).await;
}

pub async fn remove_all_after(runtime: &Arc<AppRuntime<LoggingAdapter>>, page_index: usize) {
    let start = VIEW.lock().unwrap().page * PAGE_SIZE;
    let upcoming_index = start + page_index;
    runtime.core().remove_upcoming_after(upcoming_index).await;
    publish(runtime).await;
}

/// Drag reorder with QUEUE-WIDE indices (the QML list is unfiltered when
/// drag is enabled, so page-local == page*40 + row).
pub async fn move_track(runtime: &Arc<AppRuntime<LoggingAdapter>>, from: usize, to: usize) {
    runtime.core().move_track(from, to).await;
    publish(runtime).await;
}

/// History replay (queue.rs play_history: a fresh single-track queue).
pub async fn play_history(runtime: &Arc<AppRuntime<LoggingAdapter>>, index: usize) {
    let state = runtime.core().get_queue_state_full().await;
    let Some(track) = state.history.get(index).cloned() else {
        log::warn!("[qbz-qt] queue: play_history {index} out of range");
        return;
    };
    runtime.core().set_queue(vec![track.clone()], Some(0)).await;
    crate::playback_qt::play_queue_track_public(runtime, track.id).await;
}

pub async fn clear_queue(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    // queue.rs clear_queue: drops everything INCLUDING the current track
    // (keep_current: false) and stops playback.
    runtime.core().clear_queue(false).await;
    let _ = runtime.core().stop();
    publish(runtime).await;
    crate::playback_qt::refresh_now_playing(runtime).await;
}

/// Toggle a heart for a queue row (Qobuz favorite), then republish.
pub async fn toggle_favorite(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    kind: &str,
    id: &str,
) {
    let _ = crate::library_qt::toggle_favorite(runtime, kind, id).await;
    publish(runtime).await;
}
