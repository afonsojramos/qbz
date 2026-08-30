//! The inverse membership question: WHICH playlists already hold this track?
//!
//! The playlist tables were all indexed forward (`playlist_id` leading);
//! the Add-to-Playlist picker asks the opposite direction. This module owns
//! the typed track reference at that boundary — replacing the loosely encoded
//! `"plex:<key>"` / `"<i64>"` strings — and the containment query across all
//! four membership stores:
//!
//! - `qobuz_playlist_snapshot_tracks` — Qobuz tracks in Qobuz playlists
//!   (hydrated by the membership worker; honesty via `index_state`);
//! - `playlist_local_tracks` / `playlist_plex_tracks` — library/Plex sidecars
//!   attached to Qobuz playlists (authoritative locally, complete by
//!   construction: QBZ is their only writer);
//! - `local_playlist_tracks` — everything inside `local:` playlists (same:
//!   locally authoritative).
//!
//! Identity notes. Jellyfin and Subsonic rows reach playlists through their
//! `local_tracks` projection, so their playlist identity IS the library row
//! (`track_id` + the stored path/key) — structurally collision-free across
//! servers because each server's items are distinct projected rows. Plex
//! sidecars store the bare rating key (the Plex merge keeps it in
//! `file_path`); rating keys are scoped to the single configured server, the
//! same assumption the sidecar tables have always made.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, Result};

/// One selected track at the picker/index boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlaylistTrackRef {
    /// A Qobuz catalog track.
    Qobuz(u64),
    /// A library-projected row: a local file, or a Jellyfin/Subsonic track
    /// (their playlist storage identity is the projection). `path` is the
    /// `local_tracks.file_path` value local playlists key on.
    Library {
        track_id: i64,
        path: Option<String>,
    },
    /// A Plex track by rating key.
    Plex { rating_key: String },
}

impl PlaylistTrackRef {
    /// Build the ref for a library row the way the playlist writers key it:
    /// Plex rows carry the rating key in `file_path`; every other source is
    /// the projected library row itself.
    pub fn from_library_row(source: &str, track_id: i64, file_path: &str) -> Self {
        if source == "plex" {
            Self::Plex {
                rating_key: file_path.to_string(),
            }
        } else {
            Self::Library {
                track_id,
                path: Some(file_path.to_string()),
            }
        }
    }
}

/// A playlist that contains at least one of the selected refs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ContainmentTarget {
    Qobuz(u64),
    Local(String),
}

#[derive(Debug, Clone)]
pub struct PlaylistContainment {
    pub target: ContainmentTarget,
    /// How many DISTINCT selected refs this playlist holds (the picker's
    /// "N of M"). Duplicate rows inside a playlist count once.
    pub contained: u32,
}

/// SQLite's default variable cap is 999; stay far under it.
const IN_CHUNK: usize = 400;

fn placeholders(n: usize) -> String {
    let mut s = String::with_capacity(n * 2);
    for i in 0..n {
        if i > 0 {
            s.push(',');
        }
        s.push('?');
    }
    s
}

/// Every writable playlist containing at least one of `refs`, with the
/// distinct-ref count. Qobuz targets come from the membership snapshot
/// restricted to OWNED, ACTIVE playlists (the strip offers add targets, and a
/// retired header must not resurrect there) plus the sidecar tables; `local:`
/// targets from `local_playlist_tracks`. Whether the Qobuz half may be
/// presented as definitive is `qobuz_playlist_snapshot::index_state`'s call,
/// not this function's.
pub fn playlists_containing(
    conn: &Connection,
    refs: &[PlaylistTrackRef],
) -> Result<Vec<PlaylistContainment>> {
    // ordinal = position in `refs`; contained counts distinct ordinals.
    let mut qobuz_ids: Vec<(i64, usize)> = Vec::new();
    let mut lib_ids: Vec<(i64, usize)> = Vec::new();
    let mut lib_paths: Vec<(&str, usize)> = Vec::new();
    let mut plex_keys: Vec<(&str, usize)> = Vec::new();
    for (ordinal, r) in refs.iter().enumerate() {
        match r {
            PlaylistTrackRef::Qobuz(id) => qobuz_ids.push((*id as i64, ordinal)),
            PlaylistTrackRef::Library { track_id, path } => {
                lib_ids.push((*track_id, ordinal));
                if let Some(p) = path {
                    lib_paths.push((p.as_str(), ordinal));
                }
            }
            PlaylistTrackRef::Plex { rating_key } => plex_keys.push((rating_key.as_str(), ordinal)),
        }
    }

    let mut hits: HashMap<ContainmentTarget, HashSet<usize>> = HashMap::new();
    let mut collect_qobuz = |rows: Vec<(i64, usize)>| {
        for (pid, ordinal) in rows {
            hits.entry(ContainmentTarget::Qobuz(pid as u64))
                .or_default()
                .insert(ordinal);
        }
    };

    // Qobuz refs → snapshot membership of owned active playlists.
    for chunk in qobuz_ids.chunks(IN_CHUNK) {
        let sql = format!(
            "SELECT t.qobuz_playlist_id, t.track_id
               FROM qobuz_playlist_snapshot_tracks t
               JOIN qobuz_playlist_snapshot h
                 ON h.qobuz_playlist_id = t.qobuz_playlist_id
              WHERE h.is_owned = 1 AND h.inactive = 0
                AND t.track_id IN ({})",
            placeholders(chunk.len())
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(chunk.iter().map(|(id, _)| *id)),
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        let mut found = Vec::new();
        for row in rows {
            let (pid, tid) = row?;
            for (id, ordinal) in chunk {
                if *id == tid {
                    found.push((pid, *ordinal));
                }
            }
        }
        collect_qobuz(found);
    }

    // Library refs → local-track sidecars on Qobuz playlists.
    for chunk in lib_ids.chunks(IN_CHUNK) {
        let sql = format!(
            "SELECT qobuz_playlist_id, local_track_id FROM playlist_local_tracks
              WHERE local_track_id IN ({})",
            placeholders(chunk.len())
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(chunk.iter().map(|(id, _)| *id)),
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        let mut found = Vec::new();
        for row in rows {
            let (pid, tid) = row?;
            for (id, ordinal) in chunk {
                if *id == tid {
                    found.push((pid, *ordinal));
                }
            }
        }
        collect_qobuz(found);
    }

    // Plex refs → Plex sidecars on Qobuz playlists.
    for chunk in plex_keys.chunks(IN_CHUNK) {
        let sql = format!(
            "SELECT qobuz_playlist_id, plex_rating_key FROM playlist_plex_tracks
              WHERE plex_rating_key IN ({})",
            placeholders(chunk.len())
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(chunk.iter().map(|(key, _)| *key)),
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )?;
        let mut found = Vec::new();
        for row in rows {
            let (pid, key) = row?;
            for (k, ordinal) in chunk {
                if *k == key {
                    found.push((pid, *ordinal));
                }
            }
        }
        collect_qobuz(found);
    }

    // All three identity columns of local playlists.
    let mut collect_local = |rows: Vec<(String, usize)>| {
        for (pid, ordinal) in rows {
            hits.entry(ContainmentTarget::Local(pid))
                .or_default()
                .insert(ordinal);
        }
    };
    for chunk in qobuz_ids.chunks(IN_CHUNK) {
        let sql = format!(
            "SELECT playlist_id, qobuz_track_id FROM local_playlist_tracks
              WHERE qobuz_track_id IN ({})",
            placeholders(chunk.len())
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(chunk.iter().map(|(id, _)| *id)),
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )?;
        let mut found = Vec::new();
        for row in rows {
            let (pid, tid) = row?;
            for (id, ordinal) in chunk {
                if *id == tid {
                    found.push((pid.clone(), *ordinal));
                }
            }
        }
        collect_local(found);
    }
    for chunk in lib_paths.chunks(IN_CHUNK) {
        let sql = format!(
            "SELECT playlist_id, local_path FROM local_playlist_tracks
              WHERE local_path IN ({})",
            placeholders(chunk.len())
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(chunk.iter().map(|(p, _)| *p)),
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?;
        let mut found = Vec::new();
        for row in rows {
            let (pid, path) = row?;
            for (p, ordinal) in chunk {
                if *p == path {
                    found.push((pid.clone(), *ordinal));
                }
            }
        }
        collect_local(found);
    }
    for chunk in plex_keys.chunks(IN_CHUNK) {
        let sql = format!(
            "SELECT playlist_id, plex_key FROM local_playlist_tracks
              WHERE plex_key IN ({})",
            placeholders(chunk.len())
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(chunk.iter().map(|(k, _)| *k)),
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?;
        let mut found = Vec::new();
        for row in rows {
            let (pid, key) = row?;
            for (k, ordinal) in chunk {
                if *k == key {
                    found.push((pid.clone(), *ordinal));
                }
            }
        }
        collect_local(found);
    }

    let mut out: Vec<PlaylistContainment> = hits
        .into_iter()
        .map(|(target, ordinals)| PlaylistContainment {
            target,
            contained: ordinals.len() as u32,
        })
        .collect();
    // Deterministic order: fullest first, then stable by identity.
    out.sort_by(|a, b| {
        b.contained.cmp(&a.contained).then_with(|| {
            let key = |t: &ContainmentTarget| match t {
                ContainmentTarget::Qobuz(id) => (0u8, id.to_string()),
                ContainmentTarget::Local(id) => (1u8, id.clone()),
            };
            key(&a.target).cmp(&key(&b.target))
        })
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qobuz_playlist_snapshot as snapshot;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        snapshot::init_schema(&c).unwrap();
        crate::local_playlists::init_schema(&c).unwrap();
        // Self-contained copies of the sidecar tables (the database.rs idiom:
        // unit tests build only the schema slice they exercise).
        c.execute_batch(
            "CREATE TABLE playlist_local_tracks (
                 id INTEGER PRIMARY KEY,
                 qobuz_playlist_id INTEGER NOT NULL,
                 local_track_id INTEGER NOT NULL,
                 position INTEGER NOT NULL,
                 added_at INTEGER NOT NULL,
                 UNIQUE(qobuz_playlist_id, local_track_id));
             CREATE TABLE playlist_plex_tracks (
                 id INTEGER PRIMARY KEY,
                 qobuz_playlist_id INTEGER NOT NULL,
                 plex_rating_key TEXT NOT NULL,
                 position INTEGER NOT NULL,
                 added_at INTEGER NOT NULL,
                 UNIQUE(qobuz_playlist_id, plex_rating_key));",
        )
        .unwrap();
        c
    }

    /// ONE authoritative list naming every playlist (separate lists would
    /// advance the generation and retire the earlier ones — the grace rule
    /// working as intended), then a membership snapshot for each.
    fn owned_playlists(c: &Connection, playlists: &[(u64, &str, &[u64])]) {
        let entries: Vec<snapshot::AuthoritativeEntry> = playlists
            .iter()
            .map(|(id, name, ids)| snapshot::AuthoritativeEntry {
                qobuz_playlist_id: *id,
                name: name.to_string(),
                owner: None,
                track_count: Some(ids.len() as u32),
                remote_updated_at: None,
                is_owned: true,
            })
            .collect();
        snapshot::record_authoritative_list(c, &entries).unwrap();
        for (id, name, ids) in playlists {
            snapshot::replace_tracks(c, *id, name, None, ids, None).unwrap();
        }
    }

    fn contained(rows: &[PlaylistContainment], target: ContainmentTarget) -> Option<u32> {
        rows.iter()
            .find(|r| r.target == target)
            .map(|r| r.contained)
    }

    #[test]
    fn ref_construction_routes_plex_by_stored_key() {
        assert_eq!(
            PlaylistTrackRef::from_library_row("plex", 99, "12345"),
            PlaylistTrackRef::Plex {
                rating_key: "12345".into()
            }
        );
        assert_eq!(
            PlaylistTrackRef::from_library_row("jellyfin", 7, "/j/item"),
            PlaylistTrackRef::Library {
                track_id: 7,
                path: Some("/j/item".into())
            }
        );
    }

    #[test]
    fn qobuz_containment_counts_distinct_refs() {
        let c = conn();
        owned_playlists(
            &c,
            &[
                (1, "Both", &[10, 20, 30]),
                (2, "One", &[20]),
                (3, "Neither", &[99]),
            ],
        );

        let rows = playlists_containing(
            &c,
            &[PlaylistTrackRef::Qobuz(10), PlaylistTrackRef::Qobuz(20)],
        )
        .unwrap();
        assert_eq!(contained(&rows, ContainmentTarget::Qobuz(1)), Some(2));
        assert_eq!(contained(&rows, ContainmentTarget::Qobuz(2)), Some(1));
        assert_eq!(contained(&rows, ContainmentTarget::Qobuz(3)), None);
        // Fullest playlist first.
        assert_eq!(rows[0].target, ContainmentTarget::Qobuz(1));
    }

    #[test]
    fn duplicate_membership_rows_count_once() {
        let c = conn();
        owned_playlists(&c, &[(1, "Dupes", &[10, 10, 10])]);
        let rows = playlists_containing(&c, &[PlaylistTrackRef::Qobuz(10)]).unwrap();
        assert_eq!(contained(&rows, ContainmentTarget::Qobuz(1)), Some(1));
    }

    #[test]
    fn unowned_and_inactive_playlists_are_not_targets() {
        let c = conn();
        snapshot::record_authoritative_list(
            &c,
            &[snapshot::AuthoritativeEntry {
                qobuz_playlist_id: 5,
                name: "Followed".into(),
                owner: None,
                track_count: Some(1),
                remote_updated_at: None,
                is_owned: false,
            }],
        )
        .unwrap();
        snapshot::replace_tracks(&c, 5, "Followed", None, &[10], None).unwrap();
        let rows = playlists_containing(&c, &[PlaylistTrackRef::Qobuz(10)]).unwrap();
        assert!(rows.is_empty());

        owned_playlists(&c, &[(6, "Mine", &[10])]);
        // Retire 6 by two authoritative lists without it.
        for _ in 0..2 {
            snapshot::record_authoritative_list(&c, &[]).unwrap();
        }
        let rows = playlists_containing(&c, &[PlaylistTrackRef::Qobuz(10)]).unwrap();
        assert!(rows.is_empty());
    }

    #[test]
    fn sidecars_and_local_playlists_answer_by_typed_identity() {
        let c = conn();
        c.execute_batch(
            "INSERT INTO playlist_local_tracks VALUES (NULL, 7, 42, 0, 1);
             INSERT INTO playlist_plex_tracks VALUES (NULL, 7, 'rk-9', 1, 1);",
        )
        .unwrap();
        let lp = crate::local_playlists::create(&c, "Local mix", None, false).unwrap();
        crate::local_playlists::add_tracks(
            &c,
            &lp,
            &[
                crate::local_playlists::LocalPlaylistTrackInput::Qobuz(1000),
                crate::local_playlists::LocalPlaylistTrackInput::Local("/music/a.flac".into()),
                crate::local_playlists::LocalPlaylistTrackInput::Plex("rk-9".into()),
            ],
        )
        .unwrap();

        let refs = [
            PlaylistTrackRef::Qobuz(1000),
            PlaylistTrackRef::Library {
                track_id: 42,
                path: Some("/music/a.flac".into()),
            },
            PlaylistTrackRef::Plex {
                rating_key: "rk-9".into(),
            },
        ];
        let rows = playlists_containing(&c, &refs).unwrap();
        // The Qobuz playlist holds the library row and the Plex key (2 of 3);
        // the local playlist holds all three.
        assert_eq!(contained(&rows, ContainmentTarget::Qobuz(7)), Some(2));
        assert_eq!(contained(&rows, ContainmentTarget::Local(lp.clone())), Some(3));
        assert_eq!(rows[0].target, ContainmentTarget::Local(lp));
    }

    #[test]
    fn empty_refs_answer_nothing() {
        let c = conn();
        assert!(playlists_containing(&c, &[]).unwrap().is_empty());
    }
}
