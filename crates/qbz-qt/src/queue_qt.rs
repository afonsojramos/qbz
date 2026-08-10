//! Queue panel controller — Slint-free port of the `QueueState` assembly
//! side of `crates/qbz/src/queue.rs`: NOW PLAYING card, UP NEXT with the
//! #442 "Next in queue" / "Next up" section markers (from the core's
//! `manual_next_count`), 40-row pagination, live search filter, and the
//! History tab. Mutations go straight to the core queue API; every one
//! ends with a republish.
//!
//! OWED PORTS (what the reference has and this file still does not):
//! - QConnect remote reorder — `reorder_upcoming_if_remote` is offered the
//!   move FIRST in the reference (the cloud is authoritative for queue order
//!   while connected). There is no QConnect service in this port yet, so the
//!   local core path is the only one; when the service lands, `move_track`
//!   grows that arm BEFORE `core().move_track`.
//!
//! The immersive coverflow's flat model (the old second owed port) is now
//! fed by `publish_coverflow` below (immersive-port contract §4.4).
//!
//! Row favorite seeds from the phase-5 library feed (Slint uses fav_cache —
//! same truth).

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
    /// Ephemeral row (a one-shot file play that leaves no trace in QBZ).
    /// The panel drops every PERSISTENCE affordance on these — the heart,
    /// "Add to playlist", "Track info" — exactly as `QueueSidebar.slint`
    /// gates on `item.is-ephemeral`.
    #[serde(rename = "isEphemeral")]
    pub is_ephemeral: bool,
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
    /// "Stop after this song" marker — the decimal-string id of the marked
    /// track, or "" for none. Each row compares its own id to this to draw
    /// the CircleStop in place of its track number (`QueueState.stop-after-id`).
    #[serde(rename = "stopAfterId")]
    pub stop_after_id: String,
    /// Whether InfiniteRadio autoplay is on (the ∞ footer button's accent).
    #[serde(rename = "infinitePlay")]
    pub infinite_play: bool,
    /// The live search query, echoed back so the panel can tell "the queue is
    /// empty" apart from "nothing matched" (`QueueState.search-query`).
    #[serde(rename = "searchQuery")]
    pub search_query: String,
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

/// queue.rs `display_title` (version suffix). `pub(crate)` for `mini_qt`'s row
/// projection, which must not grow a private copy of it.
pub(crate) fn display_title(track: &QueueTrack) -> String {
    match track.version.as_deref().filter(|v| !v.is_empty()) {
        Some(version) => format!("{} ({version})", track.title),
        None => track.title.clone(),
    }
}

/// `pub(crate)` for the same reason as `display_title`.
pub(crate) fn fmt_duration(secs: u64) -> String {
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
/// `HashSet<u64>` (`crates/qbz/src/queue.rs:230`); the port now has that cache
/// (`fav_cache_qt`), so it is the PRIMARY source — the library feed is empty
/// until the user opens the Library view, which is why the now-playing heart
/// used to draw blank on favourited tracks for the whole session. The feed is
/// unioned in because it is the fresher of the two once loaded.
fn favorite_track_ids() -> HashSet<u64> {
    let mut ids = crate::fav_cache_qt::all_tracks();
    if let Some(feed_ids) = crate::library_qt::with_library(|d| {
        d.feed
            .iter()
            .filter(|i| i.is_favorite && i.kind == "track")
            .filter_map(|i| canonical_u64(&i.id))
            .collect::<HashSet<u64>>()
    }) {
        ids.extend(feed_ids);
    }
    ids
}

fn row_from(track: &QueueTrack, favorites: &HashSet<u64>) -> QueueRow {
    // LOCAL/Plex rows carry a library.db row id, not a Qobuz track id, so a
    // numeric collision with the favourite set would light a heart on the
    // wrong row — and `toggle_favorite` below refuses to act on those anyway.
    let is_favorite = !track.is_local && favorites.contains(&track.id);
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
        // Both halves of the reference's test (`queue.rs:148`): the queue
        // track's own source tag, and the id-range check for a row that
        // reached the queue without one.
        is_ephemeral: track.source.as_deref() == Some("ephemeral")
            || crate::local_ephemeral::is_ephemeral_id(track.id as i64),
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

// ---------------------------------------------------------------------------
// Immersive coverflow (contract §4.4) — the FLAT full-queue model
// ---------------------------------------------------------------------------

/// One coverflow entry. Only the four fields the immersive panels read
/// (`{id, title, artist, artUrl}`) — NOT a full QueueRow: the flat model is
/// rebuilt only on id-sequence change and QML resolves covers via
/// `artwork_qt` from `artUrl`.
#[derive(Clone, Serialize)]
struct CoverflowTrack {
    id: String,
    title: String,
    artist: String,
    #[serde(rename = "artUrl")]
    art_url: String,
}

impl CoverflowTrack {
    fn from_track(t: &QueueTrack) -> Self {
        Self {
            id: t.id.to_string(),
            title: display_title(t),
            artist: t.artist.clone(),
            art_url: t.artwork_url.clone().unwrap_or_default(),
        }
    }
}

/// The flat model + NOW index, 1:1 with the Slint builder
/// (`crates/qbz/src/queue.rs:296-321`, semantics `state.slint:4854-4885`):
/// `[history.reversed (oldest..newest), NOW-PLAYING?, upcoming...]`.
/// `history` arrives MOST-RECENT-FIRST (core order), so it is reversed for
/// the oldest-first flat order. The index is `history.len()` in both arms:
/// with a current track it is NOW's slot (state.slint:4878-4881); without
/// one it points at the first upcoming (Slint queue.rs:315-318).
fn coverflow_flat(
    history: &[QueueTrack],
    current: Option<&QueueTrack>,
    upcoming: &[QueueTrack],
) -> (Vec<CoverflowTrack>, usize) {
    let mut flat: Vec<CoverflowTrack> = Vec::with_capacity(history.len() + 1 + upcoming.len());
    flat.extend(history.iter().rev().map(CoverflowTrack::from_track));
    let index = flat.len();
    if let Some(t) = current {
        flat.push(CoverflowTrack::from_track(t));
    }
    flat.extend(upcoming.iter().map(CoverflowTrack::from_track));
    (flat, index)
}

/// Order-sensitive rolling fingerprint of the flat id-sequence (Slint
/// queue.rs:328-336): equal => pure advance/jump => the tracks array is
/// REUSED and only `index` moves; different => rebuild. Hashing the ordered
/// ids (not a set) catches shuffle/reorder.
fn coverflow_seq_hash(flat: &[CoverflowTrack]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    flat.len().hash(&mut h);
    for t in flat {
        t.id.hash(&mut h);
    }
    h.finish()
}

/// (seq hash, serialized tracks array) — the array is rebuilt ONLY on
/// id-sequence change (state.slint:4854-4885 semantics).
static COVERFLOW_CACHE: Mutex<Option<(u64, String)>> = Mutex::new(None);
/// Last coverflow document handed to Qt — same equality-guarded publish
/// discipline as `LAST_JSON` below (:395-419 precedent).
static LAST_COVERFLOW_JSON: Mutex<String> = Mutex::new(String::new());

/// Build + publish the coverflow document (`{"index":i32,"tracks":[...]}`).
/// Runs on EVERY publish BEFORE the queueJson dedup: a change past the
/// current page can leave queueJson byte-identical (same page rows, same
/// totals) while the flat model moved.
fn publish_coverflow(state: &qbz_models::QueueState) {
    let (flat, index) = coverflow_flat(
        &state.history,
        state.current_track.as_ref(),
        &state.upcoming,
    );
    let seq = coverflow_seq_hash(&flat);
    let tracks_json = {
        let mut cache = COVERFLOW_CACHE.lock().unwrap();
        match cache.as_ref() {
            Some((h, json)) if *h == seq => json.clone(),
            _ => {
                let json = serde_json::to_string(&flat).unwrap_or_else(|_| "[]".into());
                *cache = Some((seq, json.clone()));
                json
            }
        }
    };
    let doc = format!("{{\"index\":{index},\"tracks\":{tracks_json}}}");
    // Identical document -> no Qt hop, no QML re-parse (LAST_JSON precedent).
    let mut last = LAST_COVERFLOW_JSON.lock().unwrap();
    if *last == doc {
        return;
    }
    last.clear();
    last.push_str(&doc);
    crate::queue_bridge::ui(move |mut b| {
        b.as_mut().set_coverflow_json(QString::from(doc.as_str()));
    });
}

/// Last miniplayer queue document handed to Qt — the mini's OWN
/// identical-document guard, same discipline as `LAST_COVERFLOW_JSON` above.
/// The mini queue surface is scrollable, and a post that carries an unchanged
/// document resets its scroll for nothing (miniplayer/tray contract §7-M9).
///
/// **Correcting §7-M9's stated reason, which does not hold in this tree:** it
/// says the mini is fed by "a pump that ticks every second". `publish` is not
/// per-tick — its three call sites inside `start_poll_loop` are all edge-gated
/// (`playback_qt.rs`: the peer-track-id edge, the track-id edge, and the
/// stop-after arm), plus the explicit mutation sites. So the guard earns its
/// place on the mutation path (a reorder or a remove that leaves the mini
/// document identical), not on a 1 Hz republish that does not exist.
///
/// **Known asymmetry with its sibling, stated rather than hidden (§8.1):**
/// `publish_coverflow` has `COVERFLOW_CACHE` and skips the serialize itself
/// when the queue sequence is unchanged; this one serializes first and compares
/// after. Accepted here because `publish` is edge-triggered, so the cost is one
/// build per real queue change, not per frame. If `publish` ever becomes
/// unconditional, this is the function that needs the cache.
static LAST_MINI_QUEUE_JSON: Mutex<String> = Mutex::new(String::new());

/// Build + publish the miniplayer's navigable queue document
/// (`{"currentId":"…","rows":[…]}` — `mini_qt::mini_queue_doc`).
///
/// Its own function rather than an inline block, because the guard's early
/// `return` aborts whatever encloses it: inlined into `publish`, an unchanged
/// mini document would skip the panel's own post below and silently freeze the
/// queue panel. Runs on EVERY publish BEFORE the queueJson dedup, for the same
/// reason `publish_coverflow` does — a change past the current page can leave
/// queueJson byte-identical while the mini model moved.
fn publish_mini_queue(state: &qbz_models::QueueState) {
    let doc = crate::mini_qt::mini_queue_doc(state);
    let mut last = LAST_MINI_QUEUE_JSON.lock().unwrap();
    if *last == doc {
        return;
    }
    last.clear();
    last.push_str(&doc);
    crate::mini_bridge::ui(move |mut b| {
        b.as_mut().set_mini_queue_json(QString::from(doc.as_str()));
    });
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

    // One core read for the marker (the reference reads it in the same pass,
    // `queue.rs:396`), so a row can compare its id without a second round trip.
    let stop_after_id = runtime
        .core()
        .get_stop_after()
        .await
        .map(|id| id.to_string())
        .unwrap_or_default();

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
        stop_after_id,
        infinite_play: is_infinite_play(),
        // The RAW typed text, not the trimmed/lowercased `query` — this is
        // what the reference's `QueueState.search-query` holds, and the
        // "nothing matched" state gates on it being non-empty.
        search_query: search,
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
    // The coverflow doc publishes on EVERY publish, BEFORE the queueJson
    // dedup below — see publish_coverflow for why.
    publish_coverflow(&state);
    // The miniplayer's navigable model rides the same funnel and the same
    // before-the-dedup placement, with its own guard (see publish_mini_queue).
    publish_mini_queue(&state);
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

/// QUEUE-WIDE 0-based index into the UNFILTERED upcoming list (contract
/// §4.4) — the immersive coverflow side cards and up-next rows call this.
/// Unlike `play_upcoming` above it bypasses the page/search VIEW entirely,
/// so it resolves the right track even when the desktop panel was left
/// filtered or paged (state.slint:4896-4900 `play-coverflow-upcoming`).
pub async fn play_upcoming_flat(runtime: &Arc<AppRuntime<LoggingAdapter>>, upcoming_index: usize) {
    match runtime.core().play_upcoming_at(upcoming_index).await {
        Some(track) => crate::playback_qt::play_queue_track_public(runtime, track.id).await,
        None => log::warn!("[qbz-qt] queue: play_upcoming_flat {upcoming_index} out of range"),
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
/// (WS-authoritative when connected) — that seam is now ported below, 1:1
/// with `queue.rs:686-697`.
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
    // Connected -> the cloud reorders and echoes a QueueUpdated that
    // materialize_remote_queue applies; do NOT also reorder locally (would
    // diverge). TRAP (§7): routed reorders never touch the local queue, and
    // on ERROR there is NO local fallback (Slint queue.rs:688-697).
    if let Some(svc) = crate::qconnect_qt::service() {
        match svc.reorder_upcoming_if_remote(from_q, to_q).await {
            Ok(true) => return,
            Ok(false) => {} // not connected -> local path
            Err(e) => {
                log::warn!("[qbz-qt] queue: reorder handoff failed: {e}");
                return; // connected but errored -> do NOT local-reorder
            }
        }
    }
    runtime.core().move_track(from_q, to_q).await;
    publish(runtime).await;
}

/// Ask the panel whether to start playing after an empty-queue drop, or clear
/// the ask. Split from the insert so a MULTI-row drop raises exactly one
/// prompt, after the last row has landed.
pub fn set_drop_play_prompt(on: bool) {
    crate::queue_bridge::ui(move |mut b| b.as_mut().set_drop_play_prompt(on));
}

/// The prompt's two answers. `true` starts the queue from its first upcoming
/// row — the same entry point the row's own click uses, so a dropped-then-
/// played track goes through one path, not two.
pub fn answer_drop_prompt(play: bool) {
    set_drop_play_prompt(false);
    if !play {
        return;
    }
    let runtime = crate::app();
    crate::spawn(async move { play_upcoming_flat(&runtime, 0).await });
}

/// Drop a dragged track into the queue AT A POSITION (owner ask 2026-08-10:
/// "poder draggear hacia el queue, definiendo la posición desde el comienzo").
///
/// `to_slot` is a slot in the VISIBLE page of upcoming — the same coordinate
/// [`move_track`] takes — so search and pagination are honoured for free.
///
/// ## Why append-then-move rather than an insert
///
/// `QueueManager` has no insert-at-index, and the three adds it does have are
/// not interchangeable here:
///   `add_track_next` / `add_track_later` both bump `manual_next_count` and
///   place the row inside the manual block. Moving it afterwards would leave
///   that counter describing a track that is no longer there.
///   `add_track` is the clean one: a plain push onto `tracks` (plus the
///   shuffle-order push), no counters touched.
/// So: `add_track` to land at the END of upcoming, then one `move_track` into
/// place. Both are existing public API — nothing is added to the shared
/// `qbz-player` crate for this.
///
/// `move_track` takes UPCOMING-relative indices in both modes (the shuffle
/// branch maps them into `shuffle_order`; the plain branch adds
/// `current_index + 1`), which is why the page mapping below can be handed to
/// it directly. Moving UP (append -> earlier slot) takes its `Up` arm, so the
/// row lands exactly at `to_slot`, i.e. before the row that was there.
pub async fn insert_dragged_track(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    track_id: u64,
    to_slot: usize,
) -> Result<(), String> {
    let qt = crate::playback_qt::stamped(
        vec![crate::playback_qt::queue_track_for(runtime, track_id).await?],
        None,
    )
    .pop()
    .expect("one track in, one track out");

    // QConnect CONTROLLER mode (contract §7): the peer owns the queue, so the
    // insert is routed there — AT THE DROPPED POSITION. `CtrlSrvrQueueInsertTracks`
    // carries `insert_after`, the same field play-next and play-later steer
    // with, so the slot survives the routing instead of collapsing to an
    // append. `slot` is already in the visible-upcoming space the peer
    // projection uses, so it crosses unchanged.
    if let Some(svc) = crate::qconnect_qt::service() {
        if svc
            .insert_at_slot_on_peer_if_active(track_id, qt.source.as_deref(), to_slot)
            .await
        {
            return Ok(());
        }
    }

    // Resolve the drop slot BEFORE the add, while the page still describes the
    // queue the user was looking at. Past the last visible row means "after
    // everything on this page".
    let page_rows = current_page_indices(runtime).await;
    let to_upcoming = match page_rows.get(to_slot) {
        Some(&idx) => idx,
        None => match page_rows.last() {
            Some(&last) => last + 1,
            None => 0,
        },
    };

    let added_castable = crate::playback_qt::batch_all_qconnect_castable(std::slice::from_ref(&qt));
    runtime.core().add_track(qt).await;

    // The appended row is the last of upcoming; re-read rather than assume,
    // because `add_track` lands relative to `tracks` and the upcoming window
    // depends on `current_index`.
    let upcoming_len = runtime.core().get_queue_state_full().await.upcoming.len();
    if upcoming_len == 0 {
        return Ok(());
    }
    let from_upcoming = upcoming_len - 1;
    if from_upcoming != to_upcoming {
        runtime.core().move_track(from_upcoming, to_upcoming).await;
    }

    crate::playback_qt::sync_qconnect_after_add(added_castable).await;
    publish(runtime).await;
    Ok(())
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

/// Empty the queue — `queue.rs::clear` verbatim.
///
/// `keep_current` is the LIVE `is_playing` flag, not a constant: the reference
/// keeps the now-playing slot only while it is audible, so clearing the queue
/// mid-track empties Up Next and lets the current track finish, and clearing it
/// while paused/stopped wipes the card too. This port used to pass `false` and
/// then call `stop()` on top, which killed playback on every Clear — the one
/// footer button that WAS wired was also the one that diverged.
///
/// The view state resets with it (page 0, search cleared), or the panel would
/// come back showing "page 3 of 1" filtered by a query with nothing left to
/// match.
pub async fn clear_queue(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let playing = runtime.core().get_playback_state().is_playing;
    runtime.core().clear_queue(playing).await;
    {
        let mut view = VIEW.lock().unwrap();
        view.page = 0;
        view.search.clear();
    }
    publish(runtime).await;
    crate::playback_qt::refresh_now_playing(runtime).await;
}

/// Toggle the "stop after this song" marker on the queue track with `id` (a
/// decimal string, matching `QueueRow.id`). Idempotent — tapping the same track
/// clears it. The core auto-clears the marker on queue mutation, and `publish`
/// reflects whatever it currently holds. `queue.rs::toggle_stop_after` 1:1.
pub async fn toggle_stop_after(runtime: &Arc<AppRuntime<LoggingAdapter>>, id: &str) {
    let Ok(track_id) = id.parse::<u64>() else {
        return;
    };
    if runtime.core().get_stop_after().await == Some(track_id) {
        runtime.core().clear_stop_after().await;
    } else {
        runtime.core().set_stop_after(track_id).await;
    }
    publish(runtime).await;
}

/// Whether `InfiniteRadio` autoplay is on (the queue auto-refills with a smart
/// radio when it runs out). Read by the footer's ∞ tint AND by the refill
/// engine in `playback_qt`'s end-of-track arm — same single truth as the
/// reference's `QueueController::is_infinite_play`.
pub fn is_infinite_play() -> bool {
    crate::settings_qt::is_infinite_play()
}

/// Toggle infinite play: flips the persisted autoplay mode between
/// `InfiniteRadio` and `ContinueWithinSource` (`queue.rs::toggle_infinite_play`).
///
/// Note the asymmetry with Settings > Playback's "Continue playback" switch,
/// which flips `ContinueWithinSource` <-> `PlayTrackOnly`: the three modes share
/// ONE preference, so turning infinite play off here lands on
/// `ContinueWithinSource` and that switch reads as ON. That is the reference's
/// behaviour, not a rounding of it.
pub async fn toggle_infinite_play(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    crate::settings_qt::set_infinite_play(!is_infinite_play());
    publish(runtime).await;
}

/// The panel became visible — re-pull the queue (`QueueState.panel-opened`).
///
/// The bridge property still holds the last document, so the panel does not
/// mount blank without this; what it buys is the case the reference names —
/// the queue changed while the panel was closed (session restore, a play from
/// a view that does not republish) — and it is nearly free, because `publish`
/// drops a byte-identical document before it reaches Qt.
pub async fn panel_opened(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    publish(runtime).await;
}

/// Open the Add-to-Playlist picker seeded with the queue's tracks — current
/// first, then upcoming, de-duplicated, in PLAY order (`queue.rs::save_as_playlist`).
///
/// The picker's inline "Create new playlist" row is what turns this into "save
/// the queue as a playlist"; picking an existing one appends. There is no
/// separate save-as-playlist endpoint in the reference either — same modal.
///
/// LOCAL/Plex and ephemeral rows are skipped: their `id` is a library.db row id
/// (or a synthetic ephemeral id), and the picker's Qobuz arm would add an
/// unrelated catalog track under that number.
///
/// The local-mode ARM now exists (`playlist_picker_qt::open_for_local_refs`),
/// so the remaining blocker is narrower than the old note claimed: what this
/// module lacks is a source-aware row -> ref RESOLVER. A `QueueTrack`'s
/// `source_item_id_hint` is overloaded (`local_playback` documents it as the
/// album id on the non-mixtape enqueue paths), so it is not a safe Plex rating
/// key outside the local queue builder's own rows. PARITY-DEBT, recorded
/// rather than silently mixing the id spaces.
pub async fn save_as_playlist(runtime: &Arc<AppRuntime<LoggingAdapter>>) {
    let state = runtime.core().get_queue_state_full().await;
    let mut ids: Vec<String> = Vec::new();
    let mut seen: HashSet<u64> = HashSet::new();
    let mut skipped = 0usize;
    for track in state.current_track.iter().chain(state.upcoming.iter()) {
        if track.is_local || crate::local_ephemeral::is_ephemeral_id(track.id as i64) {
            skipped += 1;
            continue;
        }
        if seen.insert(track.id) {
            ids.push(track.id.to_string());
        }
    }
    if skipped > 0 {
        log::info!(
            "[qbz-qt] queue save-as-playlist: skipped {skipped} local/ephemeral row(s) — \
             no local-mode picker in this port"
        );
    }
    if ids.is_empty() {
        return;
    }
    crate::playlist_picker_qt::open_for_ids(runtime, ids);
}

/// Open the picker seeded with ONE upcoming row (page-local `page_index`) —
/// the per-track "Add to playlist" entry in the row menu
/// (`queue.rs::add_to_playlist`). Resolves through the same page-local ->
/// queue-wide mapping as every other row action, so it is correct under a
/// search filter and on any page.
pub async fn add_to_playlist(runtime: &Arc<AppRuntime<LoggingAdapter>>, page_index: usize) {
    let Some(&upcoming_index) = current_page_indices(runtime).await.get(page_index) else {
        log::warn!("[qbz-qt] queue: add_to_playlist {page_index} out of range");
        return;
    };
    let state = runtime.core().get_queue_state_full().await;
    let Some(track) = state.upcoming.get(upcoming_index) else {
        log::warn!("[qbz-qt] queue: add_to_playlist {page_index} -> no upcoming track");
        return;
    };
    if track.is_local || crate::local_ephemeral::is_ephemeral_id(track.id as i64) {
        // Defensive: QueuePanel.qml drops the entry for `isLocal` and
        // `isEphemeral` rows, so this is only reachable from a stale page
        // index. Not "no local-mode picker" (that arm exists) — no
        // source-aware row -> ref resolver here; see `save_as_playlist`.
        log::warn!(
            "[qbz-qt] queue: add_to_playlist {page_index} is a local/ephemeral row — \
             no source-aware ref resolver on the queue"
        );
        return;
    }
    crate::playlist_picker_qt::open_for_ids(runtime, vec![track.id.to_string()]);
}

/// Toggle a heart for a queue row (Qobuz favorite), then republish.
///
/// GUARD: a LOCAL/Plex row's `id` is a library.db row id which is NUMERIC,
/// so `library_qt::is_local_feed_id` (which recognizes local tracks only by
/// their non-numeric FILE-PATH feed ids) would classify it as a Qobuz id and
/// send `add_favorite("track", "<db row id>")` — favoriting an unrelated
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
    // Through the shared settle point, not `library_qt::toggle_favorite`
    // alone: the queue panel used to swallow the settled value, so a heart
    // set from here never reached `libraryFavoriteChanged` (LibraryView /
    // AlbumView / ArtistView kept the old glyph) nor the document caches
    // (Home / Search / Recommendations re-published the pre-toggle heart on
    // their next republish). The panel itself still repaints from `publish`.
    if let Some(value) = crate::library_qt::toggle_favorite(runtime, kind, id).await {
        crate::emit_library_favorite(kind, id, value);
    }
    publish(runtime).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: u64, title: &str) -> QueueTrack {
        // Only the fields the coverflow reads matter; the rest are inert.
        // (QueueTrack has no Default impl — every field is spelled out.)
        QueueTrack {
            id,
            title: title.to_string(),
            version: None,
            artist: format!("artist-{id}"),
            album: String::new(),
            album_version: None,
            duration_secs: 0,
            artwork_url: Some(format!("https://img/{id}.jpg")),
            hires: false,
            bit_depth: None,
            sample_rate: None,
            is_local: false,
            album_id: None,
            artist_id: None,
            streamable: true,
            source: None,
            parental_warning: false,
            source_item_id_hint: None,
            context_kind: None,
            context_id: None,
        }
    }

    /// §4.4 JSON shape: {index, tracks:[{id,title,artist,artUrl}]} over the
    /// FULL flat queue, history OLDEST-first.
    #[test]
    fn coverflow_doc_shape_and_flat_order() {
        let history = vec![track(3, "h-new"), track(2, "h-mid"), track(1, "h-old")]; // core: most-recent-first
        let current = track(4, "now");
        let upcoming = vec![track(5, "up-0"), track(6, "up-1")];
        let (flat, index) = coverflow_flat(&history, Some(&current), &upcoming);

        // Flat = [h-old, h-mid, h-new, NOW, up-0, up-1]; NOW at history.len.
        let ids: Vec<&str> = flat.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["1", "2", "3", "4", "5", "6"]);
        assert_eq!(index, 3);

        let doc = format!(
            "{{\"index\":{index},\"tracks\":{}}}",
            serde_json::to_string(&flat).unwrap()
        );
        let v: serde_json::Value = serde_json::from_str(&doc).unwrap();
        assert_eq!(v["index"], 3);
        let rows = v["tracks"].as_array().unwrap();
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[3]["id"], "4");
        assert_eq!(rows[3]["title"], "now");
        assert_eq!(rows[3]["artist"], "artist-4");
        assert_eq!(rows[3]["artUrl"], "https://img/4.jpg");
        // Exactly the four contract fields, nothing else.
        assert_eq!(rows[3].as_object().unwrap().len(), 4);
    }

    /// NOW index with no current track: history.len (the first upcoming
    /// slot), 0 when the whole queue is empty (Slint queue.rs:315-318).
    #[test]
    fn coverflow_index_without_current() {
        let history = vec![track(2, "b"), track(1, "a")];
        let upcoming = vec![track(3, "c")];
        let (flat, index) = coverflow_flat(&history, None, &upcoming);
        let ids: Vec<&str> = flat.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["1", "2", "3"]); // no NOW slot
        assert_eq!(index, 2);

        let (flat, index) = coverflow_flat(&[], None, &[]);
        assert!(flat.is_empty());
        assert_eq!(index, 0);
    }

    /// Id-sequence gate: a pure advance keeps the flat id-sequence (tracks
    /// array reused, only `index` moves); any membership/order change
    /// rebuilds.
    #[test]
    fn coverflow_seq_hash_gates_the_rebuild() {
        let h1 = vec![track(2, "b"), track(1, "a")]; // most-recent-first
        let cur = track(3, "c");
        let up = vec![track(4, "d")];
        let (flat1, idx1) = coverflow_flat(&h1, Some(&cur), &up);
        let hash1 = coverflow_seq_hash(&flat1);
        assert_eq!(idx1, 2);

        // PURE ADVANCE materialized: track 3 moved to history, 4 is NOW —
        // the FLAT id-sequence is IDENTICAL ([1,2,3,4]), so the gate skips
        // the tracks rebuild and only `index` moves (state.slint:4857-4864).
        let h2 = vec![track(3, "c"), track(2, "b"), track(1, "a")];
        let cur2 = track(4, "d");
        let (flat2, idx2) = coverflow_flat(&h2, Some(&cur2), &[]);
        assert_eq!(coverflow_seq_hash(&flat2), hash1);
        assert_eq!(idx2, 3);
        assert_ne!(idx2, idx1);

        // Membership change (enqueue): different sequence -> rebuild.
        let up_added = vec![track(4, "d"), track(5, "e")];
        let (flat3, _) = coverflow_flat(&h1, Some(&cur), &up_added);
        assert_ne!(coverflow_seq_hash(&flat3), hash1);

        // Reorder with identical membership: also a rebuild (order-sensitive).
        let up_swap = vec![track(5, "e"), track(4, "d")];
        let (flat4, _) = coverflow_flat(&h1, Some(&cur), &up_swap);
        assert_ne!(coverflow_seq_hash(&flat4), coverflow_seq_hash(&flat3));
    }
}
