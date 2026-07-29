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

use std::collections::HashSet;
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
    /// LOCAL or Plex row (`QueueTrack::is_local`). The heart is NOT
    /// clickable on these — see `toggle_favorite`: their `id` is a
    /// library.db row id, not a Qobuz catalog id.
    #[serde(rename = "isLocal")]
    pub is_local: bool,
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

/// Canonical decimal parse — the exact equivalent of the old
/// `feed_id == track.id.to_string()` string compare (rejects the two forms
/// `u64::from_str` accepts but `to_string` never emits: a leading `+` and
/// leading zeros), so the heart state does not change with the lookup shape.
fn canonical_u64(s: &str) -> Option<u64> {
    if s.is_empty() || s.starts_with('+') || (s.len() > 1 && s.starts_with('0')) {
        return None;
    }
    s.parse::<u64>().ok()
}

/// Favorite TRACK ids from the phase-5 library feed — built ONCE per publish.
///
/// PERF (owner report: "the queue lags while a local album is loaded"). This
/// used to be a per-ROW `feed.iter().any(...)`: a full linear scan of the
/// ~10k-row library feed for EVERY row a publish builds (1 current + <=40 page
/// + <=50 history — the core caps history at 50, `qbz-player/src/queue.rs:748`),
/// with a fresh `track.id.to_string()` heap allocation on every compared
/// element and a separate LIBRARY lock per row. That is ~900k allocations and
/// ~91 lock round-trips per publish, and a LOCAL row can never match a Qobuz
/// feed id, so `any()` never short-circuits — every local row paid the FULL
/// scan, which is exactly why the lag shows up with a local album loaded.
///
/// The Slint reference resolves the same truth in O(1) from `fav_cache`'s
/// `HashSet<u64>` (`crates/qbz/src/queue.rs:230`); this is the port of that
/// shape onto the feed we have: one lock, one pass, allocation-free lookups.
fn favorite_track_ids() -> HashSet<u64> {
    crate::library_qt::with_library(|d| {
        d.feed
            .iter()
            .filter(|i| i.is_favorite && i.kind == "track")
            .filter_map(|i| canonical_u64(&i.id))
            .collect()
    })
    .unwrap_or_default()
}

fn row_from(track: &QueueTrack, favorites: &HashSet<u64>) -> QueueRow {
    let is_favorite = favorites.contains(&track.id);
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
        is_local: track.is_local,
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

/// Queue-wide upcoming indices of the rows the panel is CURRENTLY showing, in
/// display order (search filter + pagination applied) — the hoisted port of
/// `crates/qbz/src/queue.rs::resolve_upcoming_index`, resolved once per action
/// so one call answers every row on the page.
///
/// WHY IT EXISTS: the QML hands PAGE-LOCAL row numbers, and `page * PAGE_SIZE
/// + row` is the queue index ONLY while the search box is empty. With a query
/// active the page walks the FILTERED list, so that arithmetic names a
/// different track than the one under the pointer — the core indexes the
/// unfiltered upcoming list. Every row mutation (play / remove /
/// remove-all-after / drag reorder) resolves through here, which is also what
/// makes the reorder's insertion slot land on the right gap when the panel is
/// paged or filtered.
///
/// The `paginate` call clamps the page exactly like `publish` does, so the
/// slice is the one on screen even if `VIEW.page` is momentarily past the end.
fn page_row_indices(upcoming: &[QueueTrack], query: &str, page: usize) -> Vec<usize> {
    if query.is_empty() {
        let (_, _, start, end) = paginate(upcoming.len(), page);
        return (start..end).collect();
    }
    let matches: Vec<usize> = upcoming
        .iter()
        .enumerate()
        .filter(|(_, t)| matches_query(t, query))
        .map(|(i, _)| i)
        .collect();
    let (_, _, start, end) = paginate(matches.len(), page);
    matches[start..end].to_vec()
}

/// `page_row_indices` against the live queue + the current view state. ONE
/// `get_queue_state_full` per user action (the mutations below all await a
/// core call anyway); the returned vector is at most PAGE_SIZE long.
async fn current_page_indices(runtime: &Arc<AppRuntime<LoggingAdapter>>) -> Vec<usize> {
    let (search, page) = {
        let view = VIEW.lock().unwrap();
        (view.search.clone(), view.page)
    };
    let query = search.trim().to_lowercase();
    let state = runtime.core().get_queue_state_full().await;
    page_row_indices(&state.upcoming, &query, page)
}

/// Search predicate — same truth as `display_title(t)` + artist, minus the
/// intermediate `display_title` clone in the (dominant) version-less case.
/// Runs over the WHOLE upcoming list on every keystroke, so the saved
/// allocation is one per queue track per keypress.
fn matches_query(track: &QueueTrack, query: &str) -> bool {
    let title_hit = match track.version.as_deref().filter(|v| !v.is_empty()) {
        Some(version) => format!("{} ({version})", track.title)
            .to_lowercase()
            .contains(query),
        None => track.title.to_lowercase().contains(query),
    };
    title_hit || track.artist.to_lowercase().contains(query)
}

/// Last document handed to Qt — the publish is skipped when the freshly built
/// one is byte-identical (`set_queue_json` emits unconditionally, and a
/// no-op emit is NOT free downstream: THREE QML files re-`JSON.parse` the
/// document (QueuePanel, PlayerBar, NowPlayingBarSmall) and QueuePanel's
/// `onDocChanged` re-dispatches the artwork window for every row — one
/// `stat` + one `libraryArtworkReady` emit + one full `coverMap` object
/// rebuild per row, invalidating every cover binding in the panel AND the
/// sidebar). Several user actions publish twice for one change (e.g.
/// `local_playback::play_rows` -> publish, then the audible step's
/// `play_queue_track` -> publish again), and the poll loop's track edge
/// publishes on top of the transport's own. Same dedup convention as
/// `now_playing::set_effective_stream`.
static LAST_JSON: Mutex<String> = Mutex::new(String::new());

/// Ids of the LOCAL/Plex rows in the document currently on screen (<=91),
/// refreshed by every publish — the guard `toggle_favorite` consults so a
/// local row's library.db id never reaches the Qobuz favorites API.
static LOCAL_ROW_IDS: std::sync::LazyLock<Mutex<HashSet<u64>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashSet::new()));

/// Build + publish the whole panel document.
pub async fn publish(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let t_build = std::time::Instant::now();
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
            .filter(|t| matches_query(t, &query))
            .collect()
    };
    let upcoming_total = filtered.len();
    let (page, page_count, start, end) = paginate(upcoming_total, requested_page);
    VIEW.lock().unwrap().page = page;

    // ONE feed pass for the whole document (see `favorite_track_ids`).
    let favorites = favorite_track_ids();

    let page_rows: Vec<QueueRow> = filtered[start..end]
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mut r = row_from(t, &favorites);
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

    // Bounded by construction: the core keeps at most 50 history entries
    // (`qbz-player/src/queue.rs:748`), so the document cannot grow without
    // bound — <=91 rows total (1 current + <=40 page + <=50 history).
    let history: Vec<QueueRow> = state
        .history
        .iter()
        .map(|t| row_from(t, &favorites))
        .collect();
    let current = state.current_track.as_ref().map(|t| row_from(t, &favorites));

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
    let rows = doc.upcoming.len() + doc.history.len() + usize::from(doc.has_current);
    *LOCAL_ROW_IDS.lock().unwrap() = doc
        .upcoming
        .iter()
        .chain(doc.history.iter())
        .chain(doc.current.iter())
        .filter(|r| r.is_local)
        .filter_map(|r| canonical_u64(&r.id))
        .collect();
    let json = serde_json::to_string(&doc).unwrap_or_else(|_| "{}".into());
    // Identical document -> no Qt hop, no QML re-parse, no artwork re-dispatch.
    // The compare/store and the Qt post stay under ONE lock so two concurrent
    // publishes cannot post out of order and strand a stale document behind an
    // up-to-date `LAST_JSON` (there is no `.await` in here, so the guard never
    // crosses a yield point).
    let mut last = LAST_JSON.lock().unwrap();
    if *last == json {
        log::debug!(
            "[qbz-qt][perf] queue publish: unchanged, skipped ({rows} rows, {:?})",
            t_build.elapsed()
        );
        return;
    }
    last.clear();
    last.push_str(&json);
    log::debug!(
        "[qbz-qt][perf] queue publish: {rows} rows, {} favs, {} bytes, build {:?}",
        favorites.len(),
        json.len(),
        t_build.elapsed()
    );
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
    let Some(&upcoming_index) = current_page_indices(runtime).await.get(page_index) else {
        log::warn!("[qbz-qt] queue: play_upcoming {page_index} out of range");
        return;
    };
    if let Some(track) = runtime.core().play_upcoming_at(upcoming_index).await {
        crate::playback_qt::play_queue_track_public(runtime, track.id).await;
    }
}

pub async fn remove_upcoming(runtime: &Arc<AppRuntime<LoggingAdapter>>, page_index: usize) {
    let Some(&upcoming_index) = current_page_indices(runtime).await.get(page_index) else {
        log::warn!("[qbz-qt] queue: remove_upcoming {page_index} out of range");
        return;
    };
    runtime.core().remove_upcoming_track(upcoming_index).await;
    publish(runtime).await;
}

pub async fn remove_all_after(runtime: &Arc<AppRuntime<LoggingAdapter>>, page_index: usize) {
    let Some(&upcoming_index) = current_page_indices(runtime).await.get(page_index) else {
        log::warn!("[qbz-qt] queue: remove_all_after {page_index} out of range");
        return;
    };
    runtime.core().remove_upcoming_after(upcoming_index).await;
    publish(runtime).await;
}

/// Drag reorder — move the PAGE-LOCAL row `from_page` to the insertion slot
/// `to_slot` (0..=page_len), the contract of `QueueSidebar.slint`'s
/// `QueueState.reorder`. Both ends resolve to QUEUE-WIDE upcoming indices
/// through `page_row_indices` first: the panel hands page-local numbers like
/// every other row action, and under a search filter (or on page >= 1 of a
/// filtered list) a page-local number is a position in the FILTERED list, not
/// in the queue — passing it straight to the core moved a different track.
///
/// Slot semantics (Slint 1:1): slot k < page_len inserts BEFORE page-local row
/// k; slot == page_len appends after the last visible row (one past it in
/// queue-wide upcoming space). `to == from` and `to == from + 1` are the same
/// gap and are dropped by the panel before they get here; `from_q == to_q` is
/// re-checked below because the two page-local slots can collapse onto one
/// queue index once the filter is resolved.
///
/// POC-NOTE: the Slint arm first offers the move to the QConnect cloud
/// (WS-authoritative when connected); there is no QConnect seam in this port,
/// so the local core path is the only one.
pub async fn move_track(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    from_page: usize,
    to_slot: usize,
) {
    let page_rows = current_page_indices(runtime).await;
    let Some(&from_q) = page_rows.get(from_page) else {
        log::warn!("[qbz-qt] queue: reorder from {from_page} out of range");
        return;
    };
    let to_q = if to_slot >= page_rows.len() {
        match page_rows.last() {
            Some(&last) => last + 1,
            None => return,
        }
    } else {
        page_rows[to_slot]
    };
    if from_q == to_q {
        return;
    }
    runtime.core().move_track(from_q, to_q).await;
    publish(runtime).await;
}

/// History replay (queue.rs play_history: a fresh single-track queue).
pub async fn play_history(runtime: &Arc<AppRuntime<LoggingAdapter>>, index: usize) {
    let state = runtime.core().get_queue_state_full().await;
    let Some(track) = state.history.get(index).cloned() else {
        log::warn!("[qbz-qt] queue: play_history {index} out of range");
        return;
    };
    // Through the stamping seam: a history row already carries the origin it
    // was played from, so it survives the replay; a legacy row with none falls
    // back to its own album instead of publishing a dead layers glyph.
    crate::playback_qt::set_queue_stamped(runtime, vec![track.clone()], Some(0), None).await;
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
///
/// GUARD: a LOCAL/Plex row's `id` is a library.db row id which is NUMERIC,
/// so `library_qt::is_local_feed_id` (which recognizes local tracks only by
/// their non-numeric FILE-PATH feed ids) would classify it as a Qobuz id and
/// send `add_favorite("tracks", "<db row id>")` — favoriting an unrelated
/// catalog track. Local hearts have no seam in this port (`local_bulk.rs`
/// documents the same gap), so the mutation is refused here AND the row
/// publishes `isLocal` so the panel can drop the control instead of showing
/// one that cannot work.
pub async fn toggle_favorite(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    kind: &str,
    id: &str,
) {
    if kind == "track" {
        let local = canonical_u64(id)
            .map(|tid| LOCAL_ROW_IDS.lock().unwrap().contains(&tid))
            .unwrap_or(false);
        if local {
            log::warn!(
                "[qbz-qt] queue favorite: {id} is a LOCAL/Plex row — no local \
                 favorites seam in this port, refusing to route it to the \
                 Qobuz favorites API"
            );
            return;
        }
    }
    let _ = crate::library_qt::toggle_favorite(runtime, kind, id).await;
    publish(runtime).await;
}
