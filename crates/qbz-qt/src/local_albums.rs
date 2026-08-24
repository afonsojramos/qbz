//! Local Library queries: albums, folder-albums, artists, the paginated
//! Tracks tab, the tab badges and the album-detail pane.
//!
//! Split out of `local_library_qt.rs` (phase-24 modularization). Every query
//! is a BLOCKING body (the bridge wraps it in `spawn_blocking`) and every one
//! is Plex-aware:
//!
//!  - Albums / badges: `get_albums_metadata_page(..., plex_cache_path, ...)`
//!    ATTACHes the Plex cache DB, so the returned page is the local+Plex
//!    UNION (sort, search and pagination apply to the union as one set).
//!  - Tracks: each enabled source contributes one bounded candidate page.
//!    A stable global merge advances only the offsets actually consumed.
//!  - Album detail: a `plex:`-prefixed group key is served from the Plex
//!    cache instead of `library.db`.

use std::collections::HashMap;

use qbz_library::album_grouping::AlbumGroupMode;
use qbz_library::LocalTrack;

use crate::local_album_actions::AlbumDetailDoc;
use crate::local_artist_match::{
    album_matches_artist_with_aliases, build_artist_family_aliases, merge_artists,
    normalize_artist, AlbumCredit, ArtistInput,
};
use crate::local_rows::{
    artist_key, map_album, map_track, AlbumRow, ArtistRow, LocalCounts, TrackRow,
};
use crate::local_state::{
    commit_tracks_page, group_mode, state, tracks_generation, with_art, with_db,
    TrackSourceOffsets, TracksLoadRequest, TRACKS_PAGE,
};

/// Albums tab: the FULL metadata/folder-grouped set (local + Plex) in one
/// page. Search, sort and grouping derive QML-side over the parsed array.
pub fn load_albums_blocking() -> Result<Vec<AlbumRow>, String> {
    // TIMED, in three segments, because this document is republished on EVERY
    // navigation (5 times in a 37-second screencast) and there was no
    // instrument on the path at all — so "the grid got slower" could only be
    // argued about. The segments are the three things that can actually cost:
    // the SQL (which now ATTACHes two mirrors and UNIONs three sources), the
    // row mapping (which resolves an ArtRef per row), and the JSON the bridge
    // hands to QML.
    let t0 = std::time::Instant::now();
    let mode = group_mode();
    let plex_path = crate::local_plex::cache_db_path();
    // The SHARED remote mirror, and which of its sources may show. Both gates
    // matter: the path short-circuits the ATTACH for a user with no media
    // server, and the words are what make the master toggle actually remove a
    // server's rows from the grid (the mirror holds them all).
    let remote_path = crate::media_servers_qt::remote_cache_path();
    let remote_words = crate::media_servers_qt::configured_words();
    let page = with_db(|db| {
        db.get_albums_metadata_page(
            0,
            100_000,
            None,
            "artist",
            "asc",
            /* include_qobuz_downloads */ true,
            /* exclude_network_folders */ false,
            plex_path.as_deref(),
            remote_path.as_deref(),
            &remote_words,
            mode,
        )
    })
    .ok_or_else(|| "local library not available".to_string())?;
    let t_sql = t0.elapsed();
    let total = page.total;
    let albums = page.albums;
    let n = albums.len();
    let t1 = std::time::Instant::now();
    let rows = with_art(|art| {
        albums
            .into_iter()
            .map(|a| map_album(a, art))
            .collect::<Vec<AlbumRow>>()
    });
    log::info!(
        "[qbz-qt][perf] albums load: {n} rows — sql {t_sql:?}, map {:?} (plex={} remote={:?})",
        t1.elapsed(),
        plex_path.is_some(),
        remote_words,
    );
    state(|s| {
        s.counts.albums = total as i64;
        s.albums = rows.clone();
    });
    Ok(rows)
}

/// Folders tab, FLAT mode: the album grid grouped by DIRECTORY. Local-only
/// by definition — Plex rows have no filesystem folder.
pub fn load_folders_blocking() -> Result<Vec<AlbumRow>, String> {
    let albums = with_db(|db| {
        db.get_albums_with_full_filter(
            /* include_hidden */ false,
            /* include_qobuz_downloads */ true,
            /* exclude_network_folders */ false,
        )
    })
    .ok_or_else(|| "local library not available".to_string())?;
    let rows = with_art(|art| {
        albums
            .into_iter()
            .map(|a| map_album(a, art))
            .collect::<Vec<AlbumRow>>()
    });
    state(|s| {
        s.counts.folders = rows.len() as i64;
        s.folders = rows.clone();
    });
    Ok(rows)
}

/// Artists tab: the on-disk artists merged by NORMALIZED name, with the
/// aggregated Plex artists folded into the same buckets.
///
/// PARITY-DEBT #7 — this used to key the local<->Plex merge on a raw
/// `trim().to_lowercase()`, so "Beyoncé" and "Beyonce" (or "Sigur Rós" and
/// "Sigur Ros") were two rows with the user's albums and tracks split between
/// them. The key is `local_artist_match::normalize_artist` now (lowercase +
/// diacritic fold + punctuation collapse), which is also what lets the whole
/// reference merge come across: canonical spelling, summed track counts,
/// album counts recomputed from the album set (so a cross-credited album
/// counts for every contributor) and the custom -> Plex thumb -> album cover
/// portrait chain. Reference: `local_library.rs:3261-3358 merge_artists` and
/// its caller at :3540-3607.
pub fn load_artists_blocking() -> Result<Vec<ArtistRow>, String> {
    let plex_on = crate::local_plex::is_enabled();
    let mode = group_mode();
    let plex_path = crate::local_plex::cache_db_path();
    // The SHARED remote mirror, and which of its sources may show. Both gates
    // matter: the path short-circuits the ATTACH for a user with no media
    // server, and the words are what make the master toggle actually remove a
    // server's rows from the grid (the mirror holds them all).
    let remote_path = crate::media_servers_qt::remote_cache_path();
    let remote_words = crate::media_servers_qt::configured_words();
    let remote_on = !remote_words.is_empty();

    // ONE db open for the three reads the merge needs. The album set is the
    // SAME query the Albums tab runs (Plex-aware union when the toggle is on),
    // so an artist's album count matches the grid the user sees.
    let (artists, albums, custom) = with_db(|db| {
        let artists = db.get_artists_with_filter(
            /* include_qobuz_downloads */ true,
            /* exclude_network_folders */ false,
        )?;
        let albums = if plex_on || remote_on {
            db.get_albums_metadata_page(
                0,
                100_000,
                None,
                "artist",
                "asc",
                /* include_qobuz_downloads */ true,
                /* exclude_network_folders */ false,
                plex_path.as_deref(),
                remote_path.as_deref(),
                &remote_words,
                mode,
            )?
            .albums
        } else {
            db.get_albums_with_full_filter(
                /* include_hidden */ false,
                /* include_qobuz_downloads */ true,
                /* exclude_network_folders */ false,
            )?
        };
        // Custom AND previously-cached (Qobuz-fetched) portraits. A missing
        // `artist_images` table must not fail the whole tab.
        let custom = db.get_all_artist_image_urls().unwrap_or_default();
        Ok((artists, albums, custom))
    })
    .unwrap_or_default();

    let mut inputs: Vec<ArtistInput> = artists
        .into_iter()
        .map(|a| ArtistInput {
            name: a.name,
            album_count: a.album_count,
            track_count: a.track_count,
            source: "local",
        })
        .collect();

    // Plex artists join the SAME merge input (not a second pass keyed by a
    // different rule), so a local and a Plex "Radiohead" collapse to one row.
    let mut plex_portraits: HashMap<String, String> = HashMap::new();
    if plex_on {
        for pa in crate::local_plex::cached_artists() {
            let n = normalize_artist(&pa.name);
            if n.is_empty() {
                continue;
            }
            if let Some(path) = pa.artwork_path.clone().filter(|p| !p.is_empty()) {
                plex_portraits.entry(n).or_insert(path);
            }
            inputs.push(ArtistInput {
                name: pa.name,
                album_count: pa.album_count,
                track_count: pa.track_count,
                source: "plex",
            });
        }
    }

    for (source, remote) in crate::media_servers_qt::cached_artists() {
        inputs.push(ArtistInput {
            name: remote.name,
            album_count: remote.album_count,
            track_count: remote.track_count,
            source,
        });
    }

    let credits: Vec<AlbumCredit<'_>> = albums
        .iter()
        .map(|a| AlbumCredit {
            source: a.source.as_str(),
            id: a.id.as_str(),
            artist: a.artist.as_str(),
            all_artists: a.all_artists.as_str(),
            artwork_path: a.artwork_path.as_deref().unwrap_or(""),
        })
        .collect();

    let merged = merge_artists(
        inputs,
        &credits,
        &custom,
        &plex_portraits,
        plex_on || remote_on,
    );

    // The artwork index is keyed on the DISPLAY name (`artist:{name}`), so the
    // portrait is registered under the CANONICAL spelling the row carries —
    // registering it under the Plex spelling is what left a merged row with a
    // key nobody had a source for. Overwrite rather than `or_insert`: this
    // pass has just recomputed the chain, and a stale entry from a previous
    // album-identity mode must not win.
    let rows: Vec<ArtistRow> = with_art(|art| {
        merged
            .into_iter()
            .map(|m| {
                let key = artist_key(&m.name);
                if !m.image_path.is_empty() {
                    if let Some(t) =
                        crate::local_rows::art_token(Some(&m.image_source), &m.image_path)
                    {
                        art.insert(key.clone(), t);
                    }
                }
                ArtistRow {
                    art_key: key,
                    name: m.name,
                    album_count: m.album_count,
                    track_count: m.track_count,
                    source: m.source,
                }
            })
            .collect()
    });

    state(|s| {
        s.counts.artists = rows.len() as i64;
        s.artists = rows.clone();
    });
    Ok(rows)
}

/// The ids of the CACHED album rows that credit `artist`, as a JSON array —
/// the Artists tab's right pane (PARITY-DEBT #8).
///
/// The match lives here, not in QML, because it is the same normalized-part
/// rule the merge above uses and there must be exactly one of it. The QML did
/// a lowercase equality on `artist` OR a SUBSTRING `indexOf` on `allArtists`,
/// which listed "Airbourne" and "Blair" under "Air" and hid an album credited
/// "A & B" from "B". Reference: `local_library.rs:3368-3390`.
///
/// Reads `state.albums` — the very document the QML renders — so the ids it
/// returns always resolve to rows the view already has.
pub fn artist_album_ids(artist: &str) -> String {
    let nsel = normalize_artist(artist);
    if nsel.is_empty() {
        return "[]".to_string();
    }
    let ids: Vec<String> = state(|s| {
        let mut names = s
            .albums
            .iter()
            .map(|album| album.artist.as_str())
            .collect::<Vec<_>>();
        for album in &s.albums {
            names.extend(album.all_artists.split(',').filter(|name| !name.is_empty()));
        }
        let aliases = build_artist_family_aliases(&names);
        s.albums
            .iter()
            .filter(|a| {
                album_matches_artist_with_aliases(&a.artist, &a.all_artists, &nsel, &aliases)
            })
            .map(|a| a.id.clone())
            .collect()
    });
    serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string())
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TrackSourceCounts {
    pub local: usize,
    pub plex: usize,
    pub jellyfin: usize,
    pub subsonic: usize,
}

pub struct TracksPageLoad {
    pub generation: u64,
    pub rows: Vec<TrackRow>,
    pub has_more: bool,
    pub page_rows: usize,
    pub candidates: TrackSourceCounts,
    pub published: TrackSourceCounts,
    pub query_time: std::time::Duration,
    pub merge_time: std::time::Duration,
    pub map_time: std::time::Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum TrackSourcePage {
    Local,
    Plex,
    Jellyfin,
    Subsonic,
}

impl TrackSourcePage {
    fn bump(self, offsets: &mut TrackSourceOffsets) {
        match self {
            Self::Local => offsets.local += 1,
            Self::Plex => offsets.plex += 1,
            Self::Jellyfin => offsets.jellyfin += 1,
            Self::Subsonic => offsets.subsonic += 1,
        }
    }

    fn bump_count(self, counts: &mut TrackSourceCounts) {
        match self {
            Self::Local => counts.local += 1,
            Self::Plex => counts.plex += 1,
            Self::Jellyfin => counts.jellyfin += 1,
            Self::Subsonic => counts.subsonic += 1,
        }
    }
}

struct CandidatePage {
    source: TrackSourcePage,
    rows: Vec<LocalTrack>,
}

struct MergedTrackPage {
    rows: Vec<LocalTrack>,
    consumed: TrackSourceOffsets,
    published: TrackSourceCounts,
    has_more: bool,
}

/// One globally ordered Tracks page. Each source query is bounded to one page
/// plus a look-ahead row; no source is materialized in full.
pub fn load_tracks_page_blocking(
    request: TracksLoadRequest,
) -> Result<Option<TracksPageLoad>, String> {
    let candidate_limit = TRACKS_PAGE + 1;
    let effective_sort = effective_tracks_sort(&request.sort, &request.group);
    let query_started = std::time::Instant::now();
    // No library.db is valid for a remote-only installation.
    let local = with_db(|db| {
        db.search_with_filter_page(
            request.query.trim(),
            request.offsets.local,
            candidate_limit,
            true,
            false,
            effective_sort,
        )
    })
    .unwrap_or_default();
    if tracks_generation() != request.generation {
        return Ok(None);
    }
    let plex = if crate::local_plex::is_enabled() {
        crate::local_plex::search_tracks_page(
            &request.query,
            request.offsets.plex,
            candidate_limit,
            effective_sort,
        )
    } else {
        Vec::new()
    };
    if tracks_generation() != request.generation {
        return Ok(None);
    }
    let jellyfin = crate::media_servers_qt::search_tracks_page(
        "jellyfin",
        &request.query,
        request.offsets.jellyfin,
        candidate_limit,
        effective_sort,
    );
    if tracks_generation() != request.generation {
        return Ok(None);
    }
    let subsonic = crate::media_servers_qt::search_tracks_page(
        "subsonic",
        &request.query,
        request.offsets.subsonic,
        candidate_limit,
        effective_sort,
    );
    let query_time = query_started.elapsed();
    let candidates = TrackSourceCounts {
        local: local.len(),
        plex: plex.len(),
        jellyfin: jellyfin.len(),
        subsonic: subsonic.len(),
    };
    if tracks_generation() != request.generation {
        return Ok(None);
    }

    let merge_started = std::time::Instant::now();
    let merged = merge_track_pages(
        vec![
            CandidatePage { source: TrackSourcePage::Local, rows: local },
            CandidatePage { source: TrackSourcePage::Plex, rows: plex },
            CandidatePage { source: TrackSourcePage::Jellyfin, rows: jellyfin },
            CandidatePage { source: TrackSourcePage::Subsonic, rows: subsonic },
        ],
        effective_sort,
        TRACKS_PAGE as usize,
    );
    let merge_time = merge_started.elapsed();
    if tracks_generation() != request.generation {
        return Ok(None);
    }

    let page_rows = merged.rows.len();
    let raw = merged.rows;
    let map_started = std::time::Instant::now();
    let mut page_art = HashMap::new();
    let rows = raw
        .iter()
        .map(|track| map_track(track, &mut page_art))
        .collect::<Vec<TrackRow>>();
    let map_time = map_started.elapsed();
    let Some(all) = commit_tracks_page(
        &request,
        rows,
        raw,
        page_art,
        merged.consumed,
        merged.has_more,
    ) else {
        return Ok(None);
    };

    Ok(Some(TracksPageLoad {
        generation: request.generation,
        rows: all,
        has_more: merged.has_more,
        page_rows,
        candidates,
        published: merged.published,
        query_time,
        merge_time,
        map_time,
    }))
}

/// Group headers require their key to be globally contiguous. Grouping thus
/// owns the query order (matching the native catalog descriptor) while the
/// toolbar sort remains persisted for when grouping is switched off.
fn effective_tracks_sort<'a>(sort: &'a str, group: &str) -> &'a str {
    match group {
        "album" => "default",
        // Unlike artist-asc, grouping is by the performing track artist, not
        // album_artist. This is also the key printed in the group header.
        "artist" => "group-artist",
        "name" => "title-asc",
        _ => sort,
    }
}

fn merge_track_pages(pages: Vec<CandidatePage>, sort: &str, limit: usize) -> MergedTrackPage {
    let mut candidates: Vec<(TrackSourcePage, LocalTrack)> = pages
        .into_iter()
        .flat_map(|page| page.rows.into_iter().map(move |row| (page.source, row)))
        .collect();
    candidates.sort_by(|(source_a, a), (source_b, b)| {
        compare_tracks(a, b, sort)
            .then(source_a.cmp(source_b))
            .then_with(|| match source_a {
                // Plex SQL ends in rating_key; its published numeric id can
                // be a hash and therefore is not an equivalent tie-breaker.
                TrackSourcePage::Plex => a.file_path.cmp(&b.file_path),
                _ => a.id.cmp(&b.id),
            })
            .then(a.file_path.cmp(&b.file_path))
    });
    let has_more = candidates.len() > limit;
    candidates.truncate(limit);

    let mut consumed = TrackSourceOffsets::default();
    let mut published = TrackSourceCounts::default();
    let mut rows = Vec::with_capacity(candidates.len());
    for (source, row) in candidates {
        source.bump(&mut consumed);
        source.bump_count(&mut published);
        rows.push(row);
    }
    MergedTrackPage { rows, consumed, published, has_more }
}

/// The tab badges. Cheap: the Tracks count never materializes the table.
pub fn load_counts_blocking() -> LocalCounts {
    let local_tracks = with_db(|db| db.count_all_local_tracks()).unwrap_or(0) as i64;
    let plex_tracks = if crate::local_plex::is_enabled() {
        crate::local_plex::cached_track_count()
    } else {
        0
    };
    let (jellyfin_tracks, subsonic_tracks) = crate::media_servers_qt::cached_track_counts();
    let total_tracks = local_tracks + plex_tracks + jellyfin_tracks + subsonic_tracks;
    log::info!(
        "[qbz-qt][library] source counts: local={local_tracks} plex={plex_tracks} jellyfin={jellyfin_tracks} subsonic={subsonic_tracks} total={total_tracks}"
    );
    state(|s| {
        s.counts.tracks = total_tracks;
        s.counts.plex_tracks = plex_tracks;
        s.counts.jellyfin_tracks = jellyfin_tracks;
        s.counts.subsonic_tracks = subsonic_tracks;
        s.counts.clone()
    })
}

/// An album's tracks by group key: the Plex cache for a legacy content hash
/// or native edition key, else the ACTIVE identity mode's query against
/// `library.db`.
pub fn fetch_album_tracks_blocking(id: &str) -> Vec<LocalTrack> {
    if id.starts_with("plex:") {
        return crate::local_plex::album_tracks(id);
    }
    // A media-server key (`jellyfin:<albumId>` / `subsonic:<albumId>`). WITHOUT
    // this arm the key fell through to `library.db`, where it matches nothing:
    // the album page opened EMPTY, and in metadata mode it paid two fruitless
    // full queries first — which is why leaving a Jellyfin album was slower
    // than leaving a local one.
    if let Some(rows) = crate::media_servers_qt::album_tracks(id) {
        return rows;
    }
    let mode = group_mode();
    with_db(|db| match mode {
        AlbumGroupMode::Metadata => {
            let meta = db.get_album_tracks_metadata(id)?;
            if meta.is_empty() {
                db.get_album_tracks(id)
            } else {
                Ok(meta)
            }
        }
        AlbumGroupMode::Folder => db.get_album_tracks(id),
    })
    .unwrap_or_default()
}

/// The local album-detail document (header + versions + track list). `id` is
/// the group key of the ACTIVE identity mode, or a Plex legacy/native key.
///
/// The tracks are split into VERSIONS by source directory before the header is
/// built — two physical copies of the same album must not merge into one
/// duplicated track list (album/LocalAlbumView.slint `open_local_album`). The
/// header, the picker entries and the raw-row cache are all owned by
/// `local_album_actions`, which is also what the version picker and the
/// per-disc menu read back.
pub fn load_album_detail_blocking(id: &str) -> Option<AlbumDetailDoc> {
    let mut tracks = fetch_album_tracks_blocking(id);
    if tracks.is_empty() {
        return None;
    }
    // Backfill covers from cover.jpg/folder.jpg on disk (the DB may not have an
    // artwork_path even when a cover sits in the folder) — the reference does
    // exactly this here, `local_library.rs:1826`. It is what makes the detail
    // rows AND everything that later reads `detail_raw` (the per-disc menu, the
    // bulk bar, `find_track_blocking`) carry the cover the folder has.
    crate::local_playback::fill_missing_covers(&mut tracks);
    crate::local_album_actions::open_versions(id, tracks)
}

/// Comparator shared by the bounded candidate merge and its deterministic
/// tests. It mirrors every source query's allowlisted ORDER BY. Source rank
/// and the source-native identity are appended by `merge_track_pages`.
fn compare_tracks(a: &LocalTrack, b: &LocalTrack, sort: &str) -> std::cmp::Ordering {
    // SQLite NOCASE folds ASCII only. Unicode lowercasing would disagree with
    // SQLite at a page boundary and could skip a row.
    let lc = |s: &str| s.to_ascii_lowercase();
    let artist_key = |t: &LocalTrack| lc(t.album_artist.as_deref().unwrap_or(&t.artist));
    let album_tail = |a: &LocalTrack, b: &LocalTrack| {
        lc(&a.album)
            .cmp(&lc(&b.album))
            .then(a.disc_number.cmp(&b.disc_number))
            .then(a.track_number.cmp(&b.track_number))
    };
    let year_cmp = |a: &LocalTrack, b: &LocalTrack, desc: bool| match (a.year, b.year) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(ya), Some(yb)) => {
            if desc {
                yb.cmp(&ya)
            } else {
                ya.cmp(&yb)
            }
        }
    };
    match sort {
        "title-asc" => lc(&a.title)
            .cmp(&lc(&b.title))
            .then(lc(&a.artist).cmp(&lc(&b.artist))),
        "title-desc" => lc(&b.title)
            .cmp(&lc(&a.title))
            .then(lc(&a.artist).cmp(&lc(&b.artist))),
        "artist-asc" => artist_key(a).cmp(&artist_key(b)).then(album_tail(a, b)),
        "artist-desc" => artist_key(b).cmp(&artist_key(a)).then(album_tail(a, b)),
        "group-artist" => lc(&a.artist)
            .cmp(&lc(&b.artist))
            .then(lc(&a.album).cmp(&lc(&b.album)))
            .then(lc(&a.title).cmp(&lc(&b.title))),
        "year-desc" => year_cmp(a, b, true).then(album_tail(a, b)),
        "year-asc" => year_cmp(a, b, false).then(album_tail(a, b)),
        "added-desc" => b.indexed_at.cmp(&a.indexed_at).then(album_tail(a, b)),
        _ => lc(&a.album)
            .cmp(&lc(&b.album))
            .then(artist_key(a).cmp(&artist_key(b)))
            .then(a.disc_number.cmp(&b.disc_number))
            .then(a.track_number.cmp(&b.track_number))
            .then(lc(&a.title).cmp(&lc(&b.title))),
    }
}

#[cfg(test)]
mod phase_a_tests {
    use std::collections::{HashMap, HashSet};

    use super::*;

    struct Fixture {
        local: Vec<LocalTrack>,
        plex: Vec<LocalTrack>,
        jellyfin: Vec<LocalTrack>,
        subsonic: Vec<LocalTrack>,
    }

    fn source_rank(source: TrackSourcePage) -> u64 {
        match source {
            TrackSourcePage::Local => 0,
            TrackSourcePage::Plex => 1,
            TrackSourcePage::Jellyfin => 2,
            TrackSourcePage::Subsonic => 3,
        }
    }

    fn source_of(row: &LocalTrack) -> TrackSourcePage {
        match row.source.as_deref() {
            Some("plex") => TrackSourcePage::Plex,
            Some("jellyfin") => TrackSourcePage::Jellyfin,
            Some("subsonic") => TrackSourcePage::Subsonic,
            _ => TrackSourcePage::Local,
        }
    }

    fn native_tie(
        source: TrackSourcePage,
        a: &LocalTrack,
        b: &LocalTrack,
    ) -> std::cmp::Ordering {
        match source {
            TrackSourcePage::Plex => a.file_path.cmp(&b.file_path),
            _ => a.id.cmp(&b.id),
        }
        .then(a.file_path.cmp(&b.file_path))
    }

    fn global_cmp(a: &LocalTrack, b: &LocalTrack, sort: &str) -> std::cmp::Ordering {
        let source_a = source_of(a);
        let source_b = source_of(b);
        compare_tracks(a, b, sort)
            .then(source_a.cmp(&source_b))
            .then_with(|| native_tie(source_a, a, b))
    }

    fn source_rows(source: TrackSourcePage, count: usize, sort: &str) -> Vec<LocalTrack> {
        let rank = source_rank(source) as usize;
        let mut rows = (0..count)
            .map(|i| {
                let global = i * 4 + rank;
                let (id, file_path, source_word, album_artist) = match source {
                    TrackSourcePage::Local => (
                        i as i64 + 1,
                        format!("/fixture/local-{i:05}.flac"),
                        None,
                        Some(format!("Artist {:02}", global % 17)),
                    ),
                    TrackSourcePage::Plex => (
                        (crate::local_plex::PLEX_TRACK_ID_FLOOR | (i as u64 + 1)) as i64,
                        format!("plex-{i:05}"),
                        Some("plex".to_string()),
                        None,
                    ),
                    TrackSourcePage::Jellyfin => (
                        qbz_media_cache::RemoteSource::Jellyfin.namespace(i as i64 + 1),
                        format!("jellyfin-{i:05}"),
                        Some("jellyfin".to_string()),
                        Some(format!("Artist {:02}", global % 17)),
                    ),
                    TrackSourcePage::Subsonic => (
                        qbz_media_cache::RemoteSource::Subsonic.namespace(i as i64 + 1),
                        format!("subsonic-{i:05}"),
                        Some("subsonic".to_string()),
                        Some(format!("Artist {:02}", global % 17)),
                    ),
                };
                LocalTrack {
                    id,
                    file_path,
                    title: format!("Track {:05}", global / 20),
                    artist: format!("Artist {:02}", global % 17),
                    album: format!("Album {:04}", global / 40),
                    album_artist,
                    album_group_key: format!("album-{}-{}", rank, global / 40),
                    album_group_title: format!("Album {:04}", global / 40),
                    track_number: Some((global % 10 + 1) as u32),
                    disc_number: Some(1),
                    year: Some(1980 + (global % 47) as u32),
                    indexed_at: global as i64,
                    source: source_word,
                    ..Default::default()
                }
            })
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| compare_tracks(a, b, sort).then_with(|| native_tie(source, a, b)));
        rows
    }

    fn fixture(local: usize, plex: usize, jellyfin: usize, subsonic: usize, sort: &str) -> Fixture {
        Fixture {
            local: source_rows(TrackSourcePage::Local, local, sort),
            plex: source_rows(TrackSourcePage::Plex, plex, sort),
            jellyfin: source_rows(TrackSourcePage::Jellyfin, jellyfin, sort),
            subsonic: source_rows(TrackSourcePage::Subsonic, subsonic, sort),
        }
    }

    fn candidate(rows: &[LocalTrack], offset: u64) -> Vec<LocalTrack> {
        rows.iter()
            .skip(offset as usize)
            .take(TRACKS_PAGE as usize + 1)
            .cloned()
            .collect()
    }

    fn page(fixture: &Fixture, offsets: TrackSourceOffsets, sort: &str) -> MergedTrackPage {
        merge_track_pages(
            vec![
                CandidatePage {
                    source: TrackSourcePage::Local,
                    rows: candidate(&fixture.local, offsets.local),
                },
                CandidatePage {
                    source: TrackSourcePage::Plex,
                    rows: candidate(&fixture.plex, offsets.plex),
                },
                CandidatePage {
                    source: TrackSourcePage::Jellyfin,
                    rows: candidate(&fixture.jellyfin, offsets.jellyfin),
                },
                CandidatePage {
                    source: TrackSourcePage::Subsonic,
                    rows: candidate(&fixture.subsonic, offsets.subsonic),
                },
            ],
            sort,
            TRACKS_PAGE as usize,
        )
    }

    fn collect_union(fixture: &Fixture, sort: &str) -> Vec<LocalTrack> {
        let mut offsets = TrackSourceOffsets::default();
        let mut out = Vec::new();
        loop {
            let merged = page(fixture, offsets, sort);
            offsets.add_assign(merged.consumed);
            let done = !merged.has_more;
            out.extend(merged.rows);
            if done {
                break;
            }
        }
        out
    }

    #[test]
    fn mixed_pages_are_globally_ordered_and_every_id_occurs_once() {
        let sort = "title-asc";
        let input = fixture(624, 5_137, 701, 650, sort);
        let rows = collect_union(&input, sort);
        assert_eq!(rows.len(), 7_112);
        let ids: HashSet<i64> = rows.iter().map(|row| row.id).collect();
        assert_eq!(ids.len(), rows.len());
        assert!(rows
            .windows(2)
            .all(|pair| global_cmp(&pair[0], &pair[1], sort).is_le()));
    }

    #[test]
    fn every_supported_sort_stays_global_across_page_boundaries() {
        for sort in [
            "default",
            "title-asc",
            "title-desc",
            "artist-asc",
            "artist-desc",
            "group-artist",
            "year-asc",
            "year-desc",
            "added-desc",
        ] {
            let input = fixture(377, 513, 421, 389, sort);
            let rows = collect_union(&input, sort);
            assert_eq!(rows.len(), 1_700, "sort {sort}");
            let ids: HashSet<i64> = rows.iter().map(|row| row.id).collect();
            assert_eq!(ids.len(), rows.len(), "sort {sort}");
            assert!(
                rows.windows(2)
                    .all(|pair| global_cmp(&pair[0], &pair[1], sort).is_le()),
                "sort {sort}"
            );
        }
    }

    #[test]
    fn grouped_pages_keep_the_first_page_as_an_immutable_prefix() {
        let effective = effective_tracks_sort("added-desc", "artist");
        assert_eq!(effective, "group-artist");
        let input = fixture(624, 1_337, 701, 650, effective);
        let first = page(&input, TrackSourceOffsets::default(), effective);
        assert_eq!(first.rows.len(), TRACKS_PAGE as usize);
        assert!(first.has_more);

        let all = collect_union(&input, effective);
        let first_ids = first.rows.iter().map(|row| row.id).collect::<Vec<_>>();
        let all_prefix = all
            .iter()
            .take(first_ids.len())
            .map(|row| row.id)
            .collect::<Vec<_>>();
        assert_eq!(all_prefix, first_ids);
        assert!(all
            .windows(2)
            .all(|pair| global_cmp(&pair[0], &pair[1], effective).is_le()));

        assert_eq!(effective_tracks_sort("year-desc", "album"), "default");
        assert_eq!(effective_tracks_sort("year-desc", "name"), "title-asc");
        assert_eq!(effective_tracks_sort("year-desc", "off"), "year-desc");
    }

    #[test]
    fn local_rows_remaining_after_a_mixed_first_page_are_not_skipped() {
        let sort = "title-asc";
        let input = fixture(624, 0, 800, 800, sort);
        let first = page(&input, TrackSourceOffsets::default(), sort);
        assert_eq!(first.rows.len(), TRACKS_PAGE as usize);
        assert!(first.consumed.local > 0);
        assert!(first.consumed.local < 624);
        assert_eq!(
            first.consumed.local as usize,
            first
                .rows
                .iter()
                .filter(|row| source_of(row) == TrackSourcePage::Local)
                .count()
        );

        let all = collect_union(&input, sort);
        assert_eq!(all.len(), 2_224);
        assert_eq!(
            all.iter()
                .filter(|row| source_of(row) == TrackSourcePage::Local)
                .count(),
            624
        );
    }

    #[test]
    fn remote_only_union_is_paged_and_complete() {
        let sort = "title-asc";
        let input = fixture(0, 0, 1_201, 0, sort);
        let first = page(&input, TrackSourceOffsets::default(), sort);
        assert_eq!(first.rows.len(), TRACKS_PAGE as usize);
        assert!(first.has_more);
        assert_eq!(first.consumed.local, 0);
        assert_eq!(collect_union(&input, sort).len(), 1_201);
    }

    #[test]
    fn local_only_and_plex_only_unions_are_complete() {
        let sort = "title-asc";
        let local = fixture(1_201, 0, 0, 0, sort);
        let plex = fixture(0, 5_137, 0, 0, sort);
        assert_eq!(collect_union(&local, sort).len(), 1_201);
        assert_eq!(collect_union(&plex, sort).len(), 5_137);
    }

    #[test]
    fn phase_a_first_page_metrics_before_and_after() {
        let sort = "title-asc";
        let input = fixture(624, 17_145, 4_924, 6_678, sort);

        let before_query_started = std::time::Instant::now();
        let mut before = input.plex.iter().take(5_000).cloned().collect::<Vec<_>>();
        before.extend(input.jellyfin.iter().cloned());
        before.extend(input.subsonic.iter().cloned());
        before.extend(input.local.iter().take(TRACKS_PAGE as usize).cloned());
        let before_query = before_query_started.elapsed();
        let before_merge_started = std::time::Instant::now();
        before.sort_by(|a, b| global_cmp(a, b, sort));
        let before_merge = before_merge_started.elapsed();
        let before_map_started = std::time::Instant::now();
        let mut before_art = HashMap::new();
        let before_rows = before
            .iter()
            .map(|row| map_track(row, &mut before_art))
            .collect::<Vec<_>>();
        let before_map = before_map_started.elapsed();
        let before_serialize_started = std::time::Instant::now();
        let before_json = serde_json::to_vec(&before_rows).unwrap();
        let before_serialize = before_serialize_started.elapsed();

        let after_query_started = std::time::Instant::now();
        let candidates = vec![
            CandidatePage {
                source: TrackSourcePage::Local,
                rows: candidate(&input.local, 0),
            },
            CandidatePage {
                source: TrackSourcePage::Plex,
                rows: candidate(&input.plex, 0),
            },
            CandidatePage {
                source: TrackSourcePage::Jellyfin,
                rows: candidate(&input.jellyfin, 0),
            },
            CandidatePage {
                source: TrackSourcePage::Subsonic,
                rows: candidate(&input.subsonic, 0),
            },
        ];
        let after_query = after_query_started.elapsed();
        let after_merge_started = std::time::Instant::now();
        let after = merge_track_pages(candidates, sort, TRACKS_PAGE as usize);
        let after_merge = after_merge_started.elapsed();
        let after_map_started = std::time::Instant::now();
        let mut after_art = HashMap::new();
        let after_rows = after
            .rows
            .iter()
            .map(|row| map_track(row, &mut after_art))
            .collect::<Vec<_>>();
        let after_map = after_map_started.elapsed();
        let after_serialize_started = std::time::Instant::now();
        let after_json = serde_json::to_vec(&after_rows).unwrap();
        let after_serialize = after_serialize_started.elapsed();

        assert_eq!(before_rows.len(), 17_102);
        assert_eq!(after_rows.len(), TRACKS_PAGE as usize);
        assert!(after.has_more);
        assert!(after_json.len() < before_json.len());
        println!(
            "[phase-a-metrics] broad='Track' counts local=624 plex=17145 jellyfin=4924 subsonic=6678 total=29371; before rows={} json_bytes={} query_us={} merge_us={} map_us={} serialize_us={}; after rows={} json_bytes={} query_us={} merge_us={} map_us={} serialize_us={} selected local={} plex={} jellyfin={} subsonic={}",
            before_rows.len(),
            before_json.len(),
            before_query.as_micros(),
            before_merge.as_micros(),
            before_map.as_micros(),
            before_serialize.as_micros(),
            after_rows.len(),
            after_json.len(),
            after_query.as_micros(),
            after_merge.as_micros(),
            after_map.as_micros(),
            after_serialize.as_micros(),
            after.published.local,
            after.published.plex,
            after.published.jellyfin,
            after.published.subsonic,
        );
    }
}
