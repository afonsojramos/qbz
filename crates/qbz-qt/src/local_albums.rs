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
//!  - Tracks: `search_with_filter_page` is `local_tracks`-only, so the FULL
//!    Plex search set is merged ONCE on page 1 and later pages stay pure
//!    local — the local LIMIT/OFFSET path is untouched and `has_more` is
//!    driven by the LOCAL page only, so pagination never over-reports.
//!  - Album detail: a `plex:`-prefixed group key is served from the Plex
//!    cache instead of `library.db`.

use std::collections::HashMap;

use qbz_library::album_grouping::AlbumGroupMode;
use qbz_library::LocalTrack;

use crate::local_album_actions::AlbumDetailDoc;
use crate::local_artist_match::{
    album_matches_artist, merge_artists, normalize_artist, AlbumCredit, ArtistInput,
};
use crate::local_rows::{
    artist_key, map_album, map_track, AlbumRow, ArtistRow, LocalCounts, TrackRow,
};
use crate::local_state::{group_mode, state, with_art, with_db, TRACKS_PAGE};

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

    // ONE db open for the three reads the merge needs. The album set is the
    // SAME query the Albums tab runs (Plex-aware union when the toggle is on),
    // so an artist's album count matches the grid the user sees.
    let (artists, albums, custom) = with_db(|db| {
        let artists = db.get_artists_with_filter(
            /* include_qobuz_downloads */ true,
            /* exclude_network_folders */ false,
        )?;
        let albums = if plex_on {
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
    .ok_or_else(|| "local library not available".to_string())?;

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

    let merged = merge_artists(inputs, &credits, &custom, &plex_portraits, plex_on);

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
        s.albums
            .iter()
            .filter(|a| album_matches_artist(&a.artist, &a.all_artists, &nsel))
            .map(|a| a.id.clone())
            .collect()
    });
    serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string())
}

/// Tracks tab, ONE page. `reset` clears the accumulator (a new search or
/// sort); otherwise the page is appended (load-more on scroll).
pub fn load_tracks_page_blocking(reset: bool) -> Result<(Vec<TrackRow>, bool), String> {
    let (offset, query, sort) = state(|s| {
        if reset {
            s.tracks.clear();
            s.tracks_raw.clear();
            s.tracks_offset = 0;
        }
        (s.tracks_offset, s.tracks_query.clone(), s.tracks_sort.clone())
    });
    let mut page = with_db(|db| {
        db.search_with_filter_page(
            query.trim(),
            offset,
            TRACKS_PAGE,
            /* include_qobuz_downloads */ true,
            /* exclude_network_folders */ false,
            &sort,
        )
    })
    .ok_or_else(|| "local library not available".to_string())?;
    // `has_more` follows the LOCAL page only — merged Plex rows must never
    // make pagination over-report.
    let has_more = page.len() as u64 == TRACKS_PAGE;

    if crate::local_plex::is_enabled() && offset == 0 {
        let mut merged = crate::local_plex::search_tracks(&query);
        if !merged.is_empty() {
            merged.append(&mut page);
            if sort != "default" {
                sort_tracks_like_sql(&mut merged, &sort);
            }
            page = merged;
        }
    }

    let rows = with_art(|art| page.iter().map(|t| map_track(t, art)).collect::<Vec<TrackRow>>());
    let all = state(|s| {
        // The offset only ever advances by the LOCAL page length.
        s.tracks_offset += page
            .iter()
            .filter(|t| t.source.as_deref() != Some("plex"))
            .count() as u64;
        s.tracks.extend(rows);
        s.tracks_raw.extend(page);
        s.tracks_has_more = has_more;
        s.tracks.clone()
    });
    Ok((all, has_more))
}

/// The tab badges. Cheap: the Tracks count never materializes the table.
pub fn load_counts_blocking() -> LocalCounts {
    let local_tracks = with_db(|db| db.count_all_local_tracks()).unwrap_or(0) as i64;
    let plex_tracks = if crate::local_plex::is_enabled() {
        crate::local_plex::cached_track_count()
    } else {
        0
    };
    state(|s| {
        s.counts.tracks = local_tracks + plex_tracks;
        s.counts.plex_tracks = plex_tracks;
        s.counts.clone()
    })
}

/// An album's tracks by group key: the Plex cache for a `plex:` key, else
/// the ACTIVE identity mode's query against `library.db`.
pub fn fetch_album_tracks_blocking(id: &str) -> Vec<LocalTrack> {
    if id.starts_with("plex:") {
        return crate::local_plex::album_tracks(id);
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
/// the group key of the ACTIVE identity mode, or a Plex `plex:<hash>` key.
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

/// Client-side comparator mirroring `search_with_filter_page`'s ORDER BY over
/// `LocalTrack` — only used to re-sort the Plex-merged page 1. NULL years
/// sort last in both directions (SQL's `year IS NULL` prefix); `sort_by` is
/// stable, like SQLite pagination over the same ORDER BY.
fn sort_tracks_like_sql(rows: &mut [LocalTrack], sort: &str) {
    let lc = |s: &str| s.to_lowercase();
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
        "title-asc" => rows.sort_by(|a, b| {
            lc(&a.title)
                .cmp(&lc(&b.title))
                .then(lc(&a.artist).cmp(&lc(&b.artist)))
        }),
        "title-desc" => rows.sort_by(|a, b| {
            lc(&b.title)
                .cmp(&lc(&a.title))
                .then(lc(&a.artist).cmp(&lc(&b.artist)))
        }),
        "artist-asc" => rows.sort_by(|a, b| artist_key(a).cmp(&artist_key(b)).then(album_tail(a, b))),
        "artist-desc" => rows.sort_by(|a, b| artist_key(b).cmp(&artist_key(a)).then(album_tail(a, b))),
        "year-desc" => rows.sort_by(|a, b| year_cmp(a, b, true).then(album_tail(a, b))),
        "year-asc" => rows.sort_by(|a, b| year_cmp(a, b, false).then(album_tail(a, b))),
        "added-desc" => rows.sort_by(|a, b| b.indexed_at.cmp(&a.indexed_at).then(album_tail(a, b))),
        _ => {}
    }
}
