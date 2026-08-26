//! "Find available version" — replace a track Qobuz PULLED from the catalogue
//! with a live one, in a playlist the user owns (contract §6).
//!
//! The reference (Tauri's `TrackReplacementModal.svelte`) had this feature and
//! shipped it with four defects; Slint never had it at all. All four are fixed
//! here rather than ported:
//!
//!  1. **It was unranked.** Tauri showed Qobuz's raw relevance order and let the
//!     human sort it out. The weighted matcher that already lives in
//!     `qbz-playlist-import` — ISRC short-circuit, title 0.6 / artist 0.3 /
//!     album 0.1, duration bonus, stop-word normalisation, quality tie-break —
//!     does the ranking here, through its `rank_candidates` entry point.
//!  2. **It never tried the ISRC.** Licensing churn most often re-publishes the
//!     SAME recording under a new track id with the SAME ISRC, and the dead row
//!     keeps its `isrc` (confirmed in the 2026-08-17 capture of album
//!     `0886443985094`). That case needs no human judgement at all, so
//!     `find_by_isrc` runs FIRST and its hit is pinned to the top of the list.
//!  3. **It removed before it added**, so a failed add lost the track outright.
//!     The order here is add -> reposition -> remove: if the add fails nothing
//!     was destroyed and the user simply still has a dead row.
//!  4. **It did not preserve position** — the replacement landed at the end and
//!     the computed index was used only in a `console.log`. Owner ruling §C.1:
//!     preserve it, through `/playlist/updateTracksPosition`.
//!
//! # The two guards, and why each exists
//!
//! **Same-id (§A F10).** If the ISRC lookup answers with the DEAD track itself
//! — which it can, because a pulled track keeps its metadata and Qobuz's search
//! still indexes it — then "replace" would be `add` (a `no_duplicate` no-op)
//! followed by `remove`, and the user would lose the row outright while being
//! told it was repaired. Any candidate whose id equals the dead track's is
//! dropped before it can ever be selected, and [`apply`] re-checks it.
//!
//! **Failed remove (§A F10, second half).** Add-then-remove trades "lose the
//! track" for "transient duplicate", and the duplicate is transient only if the
//! remove succeeds. When it does not, the playlist permanently holds BOTH rows.
//! This is not rolled back: a rollback is a second write that can fail the same
//! way, and it would delete the only good copy. Instead the playlist is
//! refreshed so the user SEES both rows, and the toast says plainly that the
//! old one is still there and has to go by hand — never that a reload will fix
//! it, because nothing will.
//!
//! # Position is a nicety, never a reason to abort
//!
//! The reposition step needs the membership id the server minted for the row we
//! just appended, which only a re-fetch can tell us, and `insert_before`'s exact
//! encoding is the one part of the new verb no capture in the repo pins down. So
//! every failure in that step is a `log::warn!` and a different success toast —
//! the repair still stands, at the end of the playlist, exactly where the
//! reference always put it.

use std::sync::{LazyLock, Mutex};

use cxx_qt_lib::QString;
use qbz_models::Track;
use qbz_playlist_import::{rank_candidates, ImportTrack};
use serde::{Deserialize, Serialize};

/// The reference's limit, and the matcher's own `SEARCH_LIMIT`. Twenty rows is
/// as many as a human will actually read.
const SEARCH_LIMIT: u32 = 20;

// ---------------------------------------------------------------------------
// D3 — the session memory of tracks that died under the player
// ---------------------------------------------------------------------------

/// Ids the player found terminally unavailable DURING THIS RUN (contract §5.1
/// clause 2, ruling D3).
///
/// SESSION-ONLY on purpose. Tauri kept the equivalent set in `localStorage`
/// under `'qbz-unavailable-tracks'`, never expired it and never revalidated it,
/// so a track Qobuz restored stayed dead forever. A process-lifetime set gives
/// the same protection within a session — the reactive skip walk does not have
/// to re-learn the same dead track on every pass — and costs nothing on the next
/// launch, where `streamable` is re-read from the API anyway.
///
/// Two writers, both wired: [`forget`], on a successful replacement (the old id
/// must not keep poisoning a row the user just repaired), and [`mark`] from
/// `playback_qt::auto_skip_unavailable`, which is where a terminal
/// `TrackUnavailable` is recognised. [`contains`] is read by the queue-drop
/// predicate, so a track that dies under the player is never offered to the
/// player again this run.
///
/// KNOWN LIMIT, stated rather than papered over: the ROWS already on screen do
/// not re-render when a track dies this way — they were serialized from an API
/// response that said `streamable: true`. The row greys out on the next publish
/// of that view. The queue half is immediate; the visual half is eventually
/// consistent, and the up-front API flag is what covers the common case.
pub(crate) mod session_unavailable {
    use std::collections::HashSet;
    use std::sync::{LazyLock, Mutex};

    static IDS: LazyLock<Mutex<HashSet<u64>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

    /// Remember that `track_id` failed terminally. Called by the reactive skip
    /// walk when it recognises a `TrackUnavailable`.
    pub(crate) fn mark(track_id: u64) {
        IDS.lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(track_id);
    }

    /// Did this track already die under the player this run? The second half of
    /// the detection predicate, beside `Track::is_streamable()`.
    pub(crate) fn contains(track_id: u64) -> bool {
        IDS.lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&track_id)
    }

    /// Drop the memory of one id. The replacement flow calls this on success:
    /// the row is gone from the playlist, and were the same catalog id to come
    /// back (rights restored), a stale entry would keep marking it dead for the
    /// rest of the session.
    pub(crate) fn forget(track_id: u64) {
        IDS.lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&track_id);
    }
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

/// The dead row the modal was opened for. Built by the host view, which is the
/// only place that knows the playlist and the row's membership id.
///
/// No row INDEX: the host's index is into the DISPLAYED list, which under a
/// search filter or a non-default sort is not the playlist's own order. The
/// slot is derived from the authoritative playlist in [`reposition`].
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadRow {
    pub playlist_id: String,
    /// The MEMBERSHIP id — what `remove_tracks_from_playlist` takes.
    pub playlist_track_id: String,
    /// The CATALOG id — what the same-id guard compares against.
    pub track_id: String,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: String,
    /// Present on a pulled track (the capture confirms it) and the reason the
    /// exact relink is reachable at all. Empty degrades the flow to text
    /// matching, which is still better than the reference.
    #[serde(default)]
    pub isrc: String,
    #[serde(default)]
    pub duration_secs: u64,
}

#[derive(Clone, Default, Serialize)]
pub struct CandidateRow {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    #[serde(rename = "artUrl")]
    pub art_url: String,
    #[serde(rename = "artPath")]
    pub art_path: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    pub duration: String,
    pub score: f32,
    /// ISRC-identical to the dead row: the SAME recording under a new id. The
    /// modal says so, because it is the difference between a certainty and a
    /// good guess.
    pub exact: bool,
    /// Below `MIN_MATCH_SCORE`, the floor the importer refuses to auto-match on.
    /// The row is still OFFERED — a human is confirming, and a weak candidate is
    /// often the only one that exists — but it is labelled, so "the modal
    /// suggested it" never reads as "the app is sure".
    pub weak: bool,
}

#[derive(Clone, Default, Serialize)]
pub struct ReplaceDoc {
    pub open: bool,
    pub loading: bool,
    pub applying: bool,
    pub query: String,
    #[serde(rename = "deadTitle")]
    pub dead_title: String,
    #[serde(rename = "deadArtist")]
    pub dead_artist: String,
    pub candidates: Vec<CandidateRow>,
    #[serde(rename = "selectedId")]
    pub selected_id: String,
    #[serde(rename = "hasExact")]
    pub has_exact: bool,
}

#[derive(Default)]
struct ReplaceState {
    open: bool,
    loading: bool,
    applying: bool,
    dead: DeadRow,
    query: String,
    candidates: Vec<CandidateRow>,
    selected_id: String,
    /// Bumped on every open and every re-search. A search that lands AFTER the
    /// user retyped must not overwrite the newer results — and the picker's
    /// `if !open { return }` guard is not enough here, because a re-search
    /// leaves the modal open the whole time.
    generation: u64,
}

static STATE: LazyLock<Mutex<ReplaceState>> = LazyLock::new(|| Mutex::new(ReplaceState::default()));

fn with_state<R>(f: impl FnOnce(&mut ReplaceState) -> R) -> R {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(&mut guard)
}

fn publish() {
    let doc = with_state(|st| ReplaceDoc {
        open: st.open,
        loading: st.loading,
        applying: st.applying,
        query: st.query.clone(),
        dead_title: st.dead.title.clone(),
        dead_artist: st.dead.artist.clone(),
        candidates: st.candidates.clone(),
        selected_id: st.selected_id.clone(),
        has_exact: st.candidates.iter().any(|c| c.exact),
    });
    let json = serde_json::to_string(&doc).unwrap_or_else(|_| "{}".into());
    crate::track_replace_bridge::ui(move |mut b| {
        b.as_mut().set_replace_json(QString::from(json.as_str()));
    });
}

// ---------------------------------------------------------------------------
// Invokables
// ---------------------------------------------------------------------------

/// Open the modal for one dead row. `payload_json` is the host's [`DeadRow`].
pub(crate) fn open(payload_json: &str) {
    let dead: DeadRow = match serde_json::from_str(payload_json) {
        Ok(d) => d,
        Err(e) => {
            log::warn!("[qbz-qt] track replace: unreadable payload: {e}");
            return;
        }
    };
    if dead.playlist_id.is_empty() || dead.track_id.is_empty() {
        log::warn!("[qbz-qt] track replace: payload without a playlist or track id, ignored");
        return;
    }

    // The reference's query, verbatim: "title artist", nothing clever. The
    // matcher does the work; the query only has to reach the right 20 rows.
    let query = format!("{} {}", dead.title, dead.artist);
    let generation = with_state(|st| {
        st.open = true;
        st.loading = true;
        st.applying = false;
        st.dead = dead.clone();
        st.query = query.clone();
        // A stale list would show the PREVIOUS row's candidates under this
        // row's title for as long as the search takes.
        st.candidates.clear();
        st.selected_id.clear();
        st.generation = st.generation.wrapping_add(1);
        st.generation
    });
    publish();

    crate::spawn(async move { run_search(dead, query, generation).await });
}

/// The query is editable and re-searchable (the reference's one good idea).
pub(crate) fn search(query: &str) {
    let query = query.trim().to_string();
    if query.is_empty() {
        return;
    }
    let Some((dead, generation)) = with_state(|st| {
        if !st.open || st.applying {
            return None;
        }
        st.query = query.clone();
        st.loading = true;
        st.generation = st.generation.wrapping_add(1);
        Some((st.dead.clone(), st.generation))
    }) else {
        return;
    };
    publish();

    crate::spawn(async move { run_search(dead, query, generation).await });
}

/// Pick a candidate. An id that is not in the list is ignored rather than
/// stored: the apply path trusts this field, and a selection the user cannot
/// see is exactly the state the same-id guard exists to keep out.
pub(crate) fn select(track_id: &str) {
    let changed = with_state(|st| {
        if st.applying || !st.candidates.iter().any(|c| c.id == track_id) {
            return false;
        }
        st.selected_id = track_id.to_string();
        true
    });
    if changed {
        publish();
    }
}

pub(crate) fn close() {
    with_state(|st| {
        st.open = false;
        st.loading = false;
        st.applying = false;
        st.dead = DeadRow::default();
        st.query.clear();
        st.candidates.clear();
        st.selected_id.clear();
    });
    publish();
}

/// Take the applying latch back down without touching anything else — the
/// shared tail of every refusal and every failure in [`apply`], so no path can
/// leave the modal wedged with a spinning confirm button.
fn stop_applying() {
    with_state(|st| st.applying = false);
    publish();
}

// ---------------------------------------------------------------------------
// The search: ISRC first, then the ranked text search
// ---------------------------------------------------------------------------

/// Both paths drop the dead track's own id (the same-id guard) and both keep
/// only streamable candidates — offering a replacement that is itself dead is
/// the one outcome this whole feature exists to prevent.
async fn run_search(dead: DeadRow, query: String, generation: u64) {
    let runtime = crate::app();
    let dead_id: u64 = dead.track_id.parse().unwrap_or(0);

    // 1. The exact relink. A hit here is the "same recording, new album id"
    //    case and needs no human judgement, so it is pinned at the top and
    //    preselected — but it is still SHOWN, never applied silently.
    let mut exact: Option<Track> = None;
    if !dead.isrc.is_empty() {
        let catalog = crate::external_reco_qt::CoreRecoCatalog {
            runtime: runtime.clone(),
        };
        if let Some(hit) = qbz_external_reco::find_by_isrc(&catalog, &dead.isrc).await {
            if hit.id == dead_id {
                // THE SAME-ID GUARD. Qobuz's search still indexes the pulled
                // track, so the ISRC lookup can hand back the very row that is
                // dead. Accepting it would make `add` a no_duplicate no-op and
                // the following `remove` would then delete the track outright —
                // the user loses the row and is told it was repaired.
                log::info!(
                    "[qbz-qt] track replace: ISRC {} resolved to the dead track {} itself, ignored",
                    dead.isrc,
                    dead_id
                );
            } else {
                exact = Some(hit);
            }
        }
    }

    // 2. The ranked text search. `ImportTrack` is the matcher's input shape and
    //    a playlist row fills every field of it that scores.
    let source = ImportTrack {
        title: dead.title.clone(),
        artist: dead.artist.clone(),
        album: (!dead.album.is_empty()).then(|| dead.album.clone()),
        duration_ms: (dead.duration_secs > 0).then(|| dead.duration_secs * 1000),
        isrc: (!dead.isrc.is_empty()).then(|| dead.isrc.clone()),
        provider_id: None,
        provider_url: None,
    };
    let found = runtime
        .core()
        .search_tracks(&query, SEARCH_LIMIT, 0, None)
        .await
        .map(|page| page.items)
        .unwrap_or_else(|e| {
            log::warn!("[qbz-qt] track replace: search '{query}' failed: {e}");
            Vec::new()
        });

    let mut rows: Vec<CandidateRow> = Vec::new();
    if let Some(hit) = exact.as_ref() {
        rows.push(map_candidate(hit, 1.0, true));
    }
    for (track, score) in rank_candidates(&source, &found) {
        // The same-id guard again, on the text path: the pulled track is still
        // indexed and will usually be the TOP text hit for its own title.
        if track.id == dead_id {
            continue;
        }
        if exact.as_ref().map(|hit| hit.id) == Some(track.id) {
            continue;
        }
        rows.push(map_candidate(&track, score, false));
    }

    let art_urls: Vec<String> = rows
        .iter()
        .filter(|row| row.art_path.is_empty() && !row.art_url.is_empty())
        .map(|row| row.art_url.clone())
        .collect();

    let landed = with_state(|st| {
        // Landed late: the user retyped, or closed the modal. Dropping the
        // result is correct — the newer search owns the list.
        if !st.open || st.generation != generation {
            return false;
        }
        st.selected_id = rows.first().map(|row| row.id.clone()).unwrap_or_default();
        st.candidates = rows;
        st.loading = false;
        true
    });
    if !landed {
        return;
    }
    publish();

    // Covers arrive after the list, exactly like every other rail in this port:
    // the decision aids the human actually reads (title, artist, album, tier,
    // duration) are already on screen.
    if !art_urls.is_empty() {
        crate::artwork_qt::download_missing(art_urls).await;
        let still_ours = with_state(|st| {
            if !st.open || st.generation != generation {
                return false;
            }
            for row in st.candidates.iter_mut() {
                if row.art_path.is_empty() && !row.art_url.is_empty() {
                    row.art_path = crate::artwork_qt::cached_path(&row.art_url);
                }
            }
            true
        });
        if still_ours {
            publish();
        }
    }
}

fn map_candidate(track: &Track, score: f32, exact: bool) -> CandidateRow {
    let album = track.album.as_ref();
    let art_url = album
        .and_then(|a| a.image.best().cloned())
        .unwrap_or_default();
    CandidateRow {
        id: track.id.to_string(),
        // The version suffix is load-bearing HERE above anywhere else: "(2011
        // Remaster)" is often the entire difference between the candidates.
        title: match track.version.as_ref().filter(|v| !v.is_empty()) {
            Some(version) => format!("{} ({version})", track.title),
            None => track.title.clone(),
        },
        artist: track
            .performer
            .as_ref()
            .map(|a| a.name.clone())
            .unwrap_or_default(),
        album: album.map(|a| a.title.clone()).unwrap_or_default(),
        art_path: crate::artwork_qt::cached_path(&art_url),
        art_url,
        quality_tier: crate::playlist_qt::tier(track.maximum_bit_depth).to_string(),
        quality_detail: crate::home_qt::quality_detail_from_parts(
            track.maximum_bit_depth,
            track.maximum_sampling_rate,
        ),
        duration: crate::playlist_qt::mmss(track.duration),
        score,
        exact,
        // An ISRC hit is never weak, whatever the text says about it.
        weak: !exact && score < qbz_playlist_import::MIN_MATCH_SCORE,
    }
}

// ---------------------------------------------------------------------------
// Apply: ADD -> REPOSITION -> REMOVE
// ---------------------------------------------------------------------------

/// See the module header for why that order and why neither failure path rolls
/// anything back.
pub(crate) fn apply() {
    let Some((dead, selected)) = with_state(|st| {
        if st.applying || st.selected_id.is_empty() || !st.open {
            return None;
        }
        st.applying = true;
        Some((st.dead.clone(), st.selected_id.clone()))
    }) else {
        return;
    };
    publish();

    let (Ok(pid), Ok(new_id), Ok(dead_ptid)) = (
        dead.playlist_id.parse::<u64>(),
        selected.parse::<u64>(),
        dead.playlist_track_id.parse::<u64>(),
    ) else {
        log::error!("[qbz-qt] track replace: unparseable ids, refusing to write");
        stop_applying();
        return;
    };
    let dead_id = dead.track_id.parse::<u64>().unwrap_or(0);

    // Belt and braces on the same-id guard: the list already drops it, so
    // reaching here means something upstream changed and the write must not go
    // out — `add` would be a no-op and `remove` would then delete the track.
    if new_id == dead_id {
        log::error!(
            "[qbz-qt] track replace: candidate {new_id} IS the dead track — \
             the add would no-op and the remove would delete it, refusing"
        );
        stop_applying();
        return;
    }

    let runtime = crate::app();
    crate::spawn(async move {
        // STEP 1 — ADD. First, so a failure here destroys nothing: the user is
        // left with the dead row they already had.
        if let Err(e) = runtime.core().add_tracks_to_playlist(pid, &[new_id]).await {
            log::error!("[qbz-qt] track replace: add {new_id} to playlist {pid} failed: {e}");
            stop_applying();
            crate::toast_qt::error(qbz_i18n::t("Could not replace the track"));
            return;
        }

        // STEP 2 — REPOSITION (owner ruling §C.1). Best-effort by contract: a
        // failure here leaves a GOOD repair at the end of the playlist, which
        // is exactly what the reference always shipped.
        let positioned = reposition(&runtime, pid, new_id, dead_ptid).await;

        // STEP 3 — REMOVE the dead row. The one write whose failure the user
        // has to act on: the playlist then holds BOTH rows, permanently, and no
        // reload will resolve it.
        if let Err(e) = runtime
            .core()
            .remove_tracks_from_playlist(pid, &[dead_ptid])
            .await
        {
            log::error!(
                "[qbz-qt] track replace: the replacement landed but removing the dead row \
                 {dead_ptid} from playlist {pid} failed: {e}"
            );
            // Refresh FIRST, so the sentence the user reads is already true on
            // screen: both rows are there, and one of them is theirs to delete.
            crate::playlist_qt::refresh_after_membership_change(&runtime, pid).await;
            close();
            crate::toast_qt::error(qbz_i18n::t(
                "The replacement was added, but the unavailable track could not be removed — remove it manually",
            ));
            return;
        }

        // The row is gone and the repair stands: a stale session entry for the
        // old id would keep marking it dead for the rest of the run (D3).
        session_unavailable::forget(dead_id);

        crate::playlist_qt::refresh_after_membership_change(&runtime, pid).await;
        close();
        // Two msgids, not one with a spliced clause: the position outcome
        // changes what the sentence CLAIMS, and every locale must be free to
        // order it its own way.
        crate::toast_qt::success(if positioned {
            qbz_i18n::t("Track replaced")
        } else {
            qbz_i18n::t("The replacement was added at the end of the playlist")
        });
    });
}

/// Move the freshly-appended replacement into the dead row's slot.
///
/// Returns whether the move actually happened. EVERY exit is a `false` plus a
/// `log::warn!`, never an error the caller propagates: the add already landed,
/// and the repair must not be abandoned over a cosmetic ordering.
///
/// The membership id of the appended row is not knowable without a re-fetch —
/// `addTracks` answers with the playlist envelope, not with the id it minted —
/// so the authoritative playlist is read back and the row is found by
/// `(catalog id == new_id)`, taking the LAST such row: the user may legitimately
/// already own another copy of that recording earlier in the list, and the one
/// we just appended is the final one.
///
/// `insert_before` is the dead row's own 0-based index, so that after step 3
/// removes it the replacement occupies exactly the slot it vacated.
async fn reposition(
    runtime: &std::sync::Arc<qbz_app::shell::AppRuntime<qbz_core::LoggingAdapter>>,
    playlist_id: u64,
    new_id: u64,
    dead_ptid: u64,
) -> bool {
    let playlist = match runtime.core().get_playlist(playlist_id).await {
        Ok(p) => p,
        Err(e) => {
            log::warn!(
                "[qbz-qt] track replace: re-fetch of playlist {playlist_id} failed ({e}) — \
                 the replacement stays at the end"
            );
            return false;
        }
    };
    let Some(items) = playlist.tracks.as_ref().map(|c| &c.items) else {
        log::warn!(
            "[qbz-qt] track replace: playlist {playlist_id} came back without its tracks — \
             the replacement stays at the end"
        );
        return false;
    };

    let Some(dead_slot) = items
        .iter()
        .position(|t| t.playlist_track_id == Some(dead_ptid))
    else {
        // The dead row is already gone (a second client, or a retry after a
        // partially-applied attempt). Nothing to slot in front of.
        log::warn!(
            "[qbz-qt] track replace: the dead membership {dead_ptid} is no longer in playlist \
             {playlist_id} — skipping the reposition"
        );
        return false;
    };
    let Some(new_ptid) = items
        .iter()
        .rev()
        .find(|t| t.id == new_id)
        .and_then(|t| t.playlist_track_id)
    else {
        log::warn!(
            "[qbz-qt] track replace: the appended track {new_id} carries no membership id in \
             playlist {playlist_id} — the replacement stays at the end"
        );
        return false;
    };

    if let Err(e) = runtime
        .core()
        .update_playlist_tracks_position(playlist_id, &[new_ptid], dead_slot as u32)
        .await
    {
        log::warn!(
            "[qbz-qt] track replace: updateTracksPosition({playlist_id}, {new_ptid}, \
             insert_before={dead_slot}) failed ({e}) — the replacement stays at the end"
        );
        return false;
    }
    true
}
