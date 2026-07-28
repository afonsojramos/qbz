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
use crate::local_rows::{
    artist_key, map_album, map_track, AlbumRow, ArtistRow, LocalCounts, TrackRow,
};
use crate::local_state::{group_mode, state, with_art, with_db, TRACKS_PAGE};

/// Albums tab: the FULL metadata/folder-grouped set (local + Plex) in one
/// page. Search, sort and grouping derive QML-side over the parsed array.
pub fn load_albums_blocking() -> Result<Vec<AlbumRow>, String> {
    let mode = group_mode();
    let plex_path = crate::local_plex::cache_db_path();
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
            mode,
        )
    })
    .ok_or_else(|| "local library not available".to_string())?;
    let total = page.total;
    let albums = page.albums;
    let rows = with_art(|art| {
        albums
            .into_iter()
            .map(|a| map_album(a, art))
            .collect::<Vec<AlbumRow>>()
    });
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

/// Artists tab: the on-disk artists, with the aggregated Plex artists folded
/// in by case-insensitive name (a local + Plex artist of the same name counts
/// ONCE, with summed album/track counts — the Slint's `merge_artists` rule).
pub fn load_artists_blocking() -> Result<Vec<ArtistRow>, String> {
    let artists = with_db(|db| {
        db.get_artists_with_filter(
            /* include_qobuz_downloads */ true,
            /* exclude_network_folders */ false,
        )
    })
    .ok_or_else(|| "local library not available".to_string())?;

    let mut rows: Vec<ArtistRow> = artists
        .into_iter()
        .map(|a| ArtistRow {
            art_key: artist_key(&a.name),
            name: a.name,
            album_count: a.album_count,
            track_count: a.track_count,
            source: "local".into(),
        })
        .collect();

    if crate::local_plex::is_enabled() {
        let mut index: HashMap<String, usize> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| (r.name.trim().to_lowercase(), i))
            .collect();
        let plex_artists = crate::local_plex::cached_artists();
        with_art(|art| {
            for pa in plex_artists {
                let key = pa.name.trim().to_lowercase();
                if key.is_empty() {
                    continue;
                }
                let art_key = artist_key(&pa.name);
                if let Some(thumb) = pa.artwork_path.as_ref().filter(|p| !p.is_empty()) {
                    art.entry(art_key.clone()).or_insert_with(|| thumb.clone());
                }
                match index.get(&key) {
                    Some(&i) => {
                        rows[i].album_count += pa.album_count;
                        rows[i].track_count += pa.track_count;
                        rows[i].source = "mixed".into();
                    }
                    None => {
                        index.insert(key, rows.len());
                        rows.push(ArtistRow {
                            art_key,
                            name: pa.name,
                            album_count: pa.album_count,
                            track_count: pa.track_count,
                            source: "plex".into(),
                        });
                    }
                }
            }
        });
        rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    state(|s| {
        s.counts.artists = rows.len() as i64;
        s.artists = rows.clone();
    });
    Ok(rows)
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
    let tracks = fetch_album_tracks_blocking(id);
    if tracks.is_empty() {
        return None;
    }
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
