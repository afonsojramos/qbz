//! ONE SQLite cache for every remote media server QBZ reads.
//!
//! A remote source cannot be queried at scroll speed, so its library is mirrored
//! locally and the Local Library grid `ATTACH`es that mirror and `UNION`s it with
//! `local_tracks`. That mechanism already exists — for Plex, by name:
//!
//! ```text
//! qbz-library/src/database.rs:2097
//!   get_albums_metadata_page(…, plex_cache_path: Option<&Path>, …)
//!     ATTACH DATABASE '<path>' AS plex_cache
//!     FROM plex_cache.plex_cache_tracks      // twice
//! ```
//!
//! Copying that shape per source means three `Option<&Path>` parameters and six
//! UNION arms in a SHARED crate, and every future source pays the tax again.
//! This crate is the generic form: one table with a `source` discriminator, one
//! ATTACH, one UNION arm.
//!
//! # Why Plex is NOT migrated in the same change
//!
//! The schema below is `plex_cache_tracks` with its columns renamed — every one
//! of its 19 columns maps 1:1 onto Jellyfin and Subsonic, which is what made
//! "generic" the right call rather than a guess. Folding Plex in is therefore a
//! rename plus a data migration, not a redesign.
//!
//! It is still a migration of a table that ships with a user's data — 17 145
//! rows on the owner's install — and doing it in the same change that introduces
//! two new sources would mean one commit where a regression could come from
//! either. So: this table serves Jellyfin and Subsonic first and proves itself
//! with two sources; Plex moves in as its own change, with its own verification,
//! once it has. Until then the union carries two ATTACHes instead of one, which
//! is a cost paid in one SQL builder rather than a risk taken with a live cache.
//!
//! # Identity: the namespaced row id
//!
//! The Local Library speaks in `i64` row ids, so a remote track needs one that
//! can never collide with a real `local_tracks.id`. The existing scheme is a
//! high bit per source:
//!
//! | source | floor | payload |
//! |---|---|---|
//! | Plex (`local_plex::PLEX_TRACK_ID_FLOOR`) | `1 << 40` | hashed rating key, 40 bits |
//! | ephemeral (`qbz_library::ephemeral`) | `1 << 48` | session counter |
//! | **Jellyfin** | `1 << 41` | this cache's rowid |
//! | **Subsonic** | `1 << 42` | this cache's rowid |
//!
//! Payloads are masked to 40 bits, so the ranges are disjoint by construction.
//!
//! The payload is **this table's `AUTOINCREMENT` rowid, not a hash of the
//! server's id.** Plex had to hash because its ids predate its cache, and that
//! hash is why `PlexSource::resolve_cache_row_id` has to scan to invert it. A
//! new table has no such debt: the mapping is a primary-key lookup in both
//! directions and cannot collide. With ~50 000 tracks a 40-bit hash carries
//! roughly a 0.1 % chance of at least one collision, and a collision here plays
//! the WRONG TRACK — which is precisely the class of bug the source seam exists
//! to remove, so it is not worth reintroducing for a marginally simpler write.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};

/// Namespace bit for Jellyfin row ids.
pub const JELLYFIN_ID_FLOOR: i64 = 1 << 41;
/// Namespace bit for Subsonic row ids.
pub const SUBSONIC_ID_FLOOR: i64 = 1 << 42;
/// Payload width. Matches Plex's, so every source's payload occupies the same
/// low 40 bits and the floors stay disjoint.
pub const ID_PAYLOAD_BITS: u32 = 40;
const PAYLOAD_MASK: i64 = (1 << ID_PAYLOAD_BITS) - 1;

/// Which remote source a cached row came from. The wire values are the same
/// words `qbz_source::SourceId` uses, so nothing has to translate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemoteSource {
    Jellyfin,
    Subsonic,
}

impl RemoteSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            RemoteSource::Jellyfin => "jellyfin",
            RemoteSource::Subsonic => "subsonic",
        }
    }

    pub fn from_word(w: &str) -> Option<Self> {
        match w {
            "jellyfin" => Some(RemoteSource::Jellyfin),
            "subsonic" => Some(RemoteSource::Subsonic),
            _ => None,
        }
    }

    pub const fn floor(self) -> i64 {
        match self {
            RemoteSource::Jellyfin => JELLYFIN_ID_FLOOR,
            RemoteSource::Subsonic => SUBSONIC_ID_FLOOR,
        }
    }

    /// Namespace a cache rowid for the Local Library's `i64` space.
    pub fn namespace(self, rowid: i64) -> i64 {
        self.floor() | (rowid & PAYLOAD_MASK)
    }

    /// Which source owns a namespaced id, if any.
    ///
    /// Tested against the OTHER floors on purpose: bit 40 is Plex's and bit 48
    /// is the ephemeral store's, and a predicate that answered "mine" for
    /// either would route a row to the wrong source.
    pub fn of_id(id: i64) -> Option<Self> {
        for s in [RemoteSource::Jellyfin, RemoteSource::Subsonic] {
            let floor = s.floor();
            if id & floor != 0 && id & !(floor | PAYLOAD_MASK) == 0 {
                return Some(s);
            }
        }
        None
    }

    /// Recover the cache rowid from a namespaced id.
    pub fn rowid_of(id: i64) -> i64 {
        id & PAYLOAD_MASK
    }
}

/// One cached track, as this crate stores and returns it.
///
/// Deliberately flat and owned: it crosses a `spawn_blocking` boundary on every
/// read, and a borrowed shape would tie it to the connection's lifetime.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CachedTrack {
    /// The namespaced `i64` the Local Library uses. Assigned by this crate;
    /// zero until the row is written.
    pub id: i64,
    pub source: String,
    /// The server's OWN id for this track, opaque. The only thing that can be
    /// handed back to the server.
    pub item_id: String,
    pub server_id: String,
    pub library_id: String,
    pub title: String,
    pub artist: String,
    pub album_artist: String,
    pub album: String,
    /// The server's album id — the grouping key for the album view.
    pub album_id: String,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub duration_ms: u64,
    pub year: Option<u32>,
    pub genre: Option<String>,
    pub container: String,
    pub codec: Option<String>,
    /// `None` for lossy. Both wire sentinels (Jellyfin's null, Subsonic's 0)
    /// are folded before they reach here.
    pub bit_depth: Option<u32>,
    pub sample_rate_hz: Option<u32>,
    pub channels: Option<u32>,
    pub bitrate_kbps: Option<u32>,
    /// The RAW artwork token as the server issued it — a Jellyfin image tag, a
    /// Subsonic `coverArt` id. Never a URL: a URL embeds credentials and a
    /// size, and both change while the token does not.
    pub artwork_token: Option<String>,
    pub size_bytes: Option<u64>,
}

/// One cached library / music folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedLibrary {
    pub source: String,
    pub library_id: String,
    pub name: String,
    pub server_id: String,
}

pub type Result<T> = std::result::Result<T, String>;

fn map_err<E: std::fmt::Display>(what: &str) -> impl Fn(E) -> String + '_ {
    move |e| format!("media cache: {what}: {e}")
}

/// Open (creating if needed) the cache at `path` and bring its schema up to date.
///
/// WAL, like every other QBZ database (ADR-002): the scan writes while the grid
/// reads, and without WAL the grid blocks behind the sync.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(map_err("create dir"))?;
    }
    let conn = Connection::open(path).map_err(map_err("open"))?;
    init_schema(&conn)?;
    Ok(conn)
}

/// The schema. Split out so tests can drive an in-memory connection.
pub fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .map_err(map_err("pragmas"))?;
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS remote_cache_libraries (
            source      TEXT NOT NULL,
            library_id  TEXT NOT NULL,
            name        TEXT NOT NULL,
            server_id   TEXT NOT NULL DEFAULT '',
            updated_at  INTEGER NOT NULL,
            PRIMARY KEY (source, library_id)
        );

        -- `id INTEGER PRIMARY KEY AUTOINCREMENT` is load-bearing: it is the
        -- payload of the namespaced Local Library id, so it must never be
        -- REUSED after a delete. Plain `INTEGER PRIMARY KEY` reuses the highest
        -- freed rowid, which would silently point an already-published id at a
        -- different track.
        CREATE TABLE IF NOT EXISTS remote_cache_tracks (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            source          TEXT NOT NULL,
            item_id         TEXT NOT NULL,
            server_id       TEXT NOT NULL DEFAULT '',
            library_id      TEXT NOT NULL DEFAULT '',
            title           TEXT NOT NULL DEFAULT '',
            artist          TEXT NOT NULL DEFAULT '',
            album_artist    TEXT NOT NULL DEFAULT '',
            album           TEXT NOT NULL DEFAULT '',
            album_id        TEXT NOT NULL DEFAULT '',
            track_number    INTEGER,
            disc_number     INTEGER,
            duration_ms     INTEGER NOT NULL DEFAULT 0,
            year            INTEGER,
            genre           TEXT,
            container       TEXT NOT NULL DEFAULT '',
            codec           TEXT,
            bit_depth       INTEGER,
            sample_rate_hz  INTEGER,
            channels        INTEGER,
            bitrate_kbps    INTEGER,
            artwork_token   TEXT,
            size_bytes      INTEGER,
            updated_at      INTEGER NOT NULL,
            UNIQUE (source, item_id)
        );

        CREATE INDEX IF NOT EXISTS idx_rct_source_album
            ON remote_cache_tracks(source, album_id);
        CREATE INDEX IF NOT EXISTS idx_rct_source_library
            ON remote_cache_tracks(source, library_id);
        -- The Local Library grid sorts and filters on these two.
        CREATE INDEX IF NOT EXISTS idx_rct_album_artist
            ON remote_cache_tracks(album_artist);
        CREATE INDEX IF NOT EXISTS idx_rct_title ON remote_cache_tracks(title);
        ",
    )
    .map_err(map_err("schema"))
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn row_to_track(row: &rusqlite::Row<'_>) -> rusqlite::Result<CachedTrack> {
    let source: String = row.get("source")?;
    let rowid: i64 = row.get("id")?;
    let id = RemoteSource::from_word(&source)
        .map(|s| s.namespace(rowid))
        .unwrap_or(rowid);
    Ok(CachedTrack {
        id,
        source,
        item_id: row.get("item_id")?,
        server_id: row.get("server_id")?,
        library_id: row.get("library_id")?,
        title: row.get("title")?,
        artist: row.get("artist")?,
        album_artist: row.get("album_artist")?,
        album: row.get("album")?,
        album_id: row.get("album_id")?,
        track_number: row.get::<_, Option<i64>>("track_number")?.map(|v| v as u32),
        disc_number: row.get::<_, Option<i64>>("disc_number")?.map(|v| v as u32),
        duration_ms: row.get::<_, i64>("duration_ms")? as u64,
        year: row.get::<_, Option<i64>>("year")?.map(|v| v as u32),
        genre: row.get("genre")?,
        container: row.get("container")?,
        codec: row.get("codec")?,
        bit_depth: row.get::<_, Option<i64>>("bit_depth")?.map(|v| v as u32),
        sample_rate_hz: row.get::<_, Option<i64>>("sample_rate_hz")?.map(|v| v as u32),
        channels: row.get::<_, Option<i64>>("channels")?.map(|v| v as u32),
        bitrate_kbps: row.get::<_, Option<i64>>("bitrate_kbps")?.map(|v| v as u32),
        artwork_token: row.get("artwork_token")?,
        size_bytes: row.get::<_, Option<i64>>("size_bytes")?.map(|v| v as u64),
    })
}

const SELECT: &str = "SELECT id, source, item_id, server_id, library_id, title, artist, \
     album_artist, album, album_id, track_number, disc_number, duration_ms, year, genre, \
     container, codec, bit_depth, sample_rate_hz, channels, bitrate_kbps, artwork_token, \
     size_bytes FROM remote_cache_tracks";

/// UPSERT a batch of tracks in ONE transaction.
///
/// `ON CONFLICT (source, item_id) DO UPDATE` rather than delete-then-insert:
/// the row id is a published identity (it is inside every queue entry and every
/// `session.db` row), so a re-scan must not mint a new one for a track that was
/// already there. A user who re-syncs while a Jellyfin track is playing would
/// otherwise find the queue pointing at nothing.
pub fn save_tracks(conn: &mut Connection, source: RemoteSource, tracks: &[CachedTrack]) -> Result<usize> {
    if tracks.is_empty() {
        return Ok(0);
    }
    let ts = now();
    let tx = conn.transaction().map_err(map_err("begin"))?;
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO remote_cache_tracks
                   (source, item_id, server_id, library_id, title, artist, album_artist, album,
                    album_id, track_number, disc_number, duration_ms, year, genre, container,
                    codec, bit_depth, sample_rate_hz, channels, bitrate_kbps, artwork_token,
                    size_bytes, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)
                 ON CONFLICT(source, item_id) DO UPDATE SET
                    server_id=excluded.server_id, library_id=excluded.library_id,
                    title=excluded.title, artist=excluded.artist,
                    album_artist=excluded.album_artist, album=excluded.album,
                    album_id=excluded.album_id, track_number=excluded.track_number,
                    disc_number=excluded.disc_number, duration_ms=excluded.duration_ms,
                    year=excluded.year, genre=excluded.genre, container=excluded.container,
                    codec=excluded.codec, bit_depth=excluded.bit_depth,
                    sample_rate_hz=excluded.sample_rate_hz, channels=excluded.channels,
                    bitrate_kbps=excluded.bitrate_kbps, artwork_token=excluded.artwork_token,
                    size_bytes=excluded.size_bytes, updated_at=excluded.updated_at",
            )
            .map_err(map_err("prepare insert"))?;
        for t in tracks {
            stmt.execute(params![
                source.as_str(),
                t.item_id,
                t.server_id,
                t.library_id,
                t.title,
                t.artist,
                t.album_artist,
                t.album,
                t.album_id,
                t.track_number.map(|v| v as i64),
                t.disc_number.map(|v| v as i64),
                t.duration_ms as i64,
                t.year.map(|v| v as i64),
                t.genre,
                t.container,
                t.codec,
                t.bit_depth.map(|v| v as i64),
                t.sample_rate_hz.map(|v| v as i64),
                t.channels.map(|v| v as i64),
                t.bitrate_kbps.map(|v| v as i64),
                t.artwork_token,
                t.size_bytes.map(|v| v as i64),
                ts,
            ])
            .map_err(map_err("insert track"))?;
        }
    }
    tx.commit().map_err(map_err("commit"))?;
    Ok(tracks.len())
}

/// Replace the known libraries for one source.
pub fn save_libraries(conn: &mut Connection, source: RemoteSource, libs: &[CachedLibrary]) -> Result<()> {
    let ts = now();
    let tx = conn.transaction().map_err(map_err("begin"))?;
    tx.execute(
        "DELETE FROM remote_cache_libraries WHERE source = ?1",
        params![source.as_str()],
    )
    .map_err(map_err("clear libraries"))?;
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO remote_cache_libraries (source, library_id, name, server_id, updated_at)
                 VALUES (?1,?2,?3,?4,?5)",
            )
            .map_err(map_err("prepare library"))?;
        for l in libs {
            stmt.execute(params![source.as_str(), l.library_id, l.name, l.server_id, ts])
                .map_err(map_err("insert library"))?;
        }
    }
    tx.commit().map_err(map_err("commit"))
}

pub fn libraries(conn: &Connection, source: RemoteSource) -> Result<Vec<CachedLibrary>> {
    let mut stmt = conn
        .prepare("SELECT source, library_id, name, server_id FROM remote_cache_libraries WHERE source = ?1 ORDER BY name")
        .map_err(map_err("prepare libraries"))?;
    let rows = stmt
        .query_map(params![source.as_str()], |r| {
            Ok(CachedLibrary {
                source: r.get(0)?,
                library_id: r.get(1)?,
                name: r.get(2)?,
                server_id: r.get(3)?,
            })
        })
        .map_err(map_err("query libraries"))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_err("read libraries"))
}

/// One track by its NAMESPACED id — a primary-key lookup, not a scan.
///
/// This is the whole payoff of using the rowid as the payload: `PlexSource`
/// needs `resolve_cache_row_id` to invert a hash, and its slow path is a full
/// table scan. Here the id IS the key.
pub fn track_by_id(conn: &Connection, id: i64) -> Result<Option<CachedTrack>> {
    let Some(source) = RemoteSource::of_id(id) else {
        return Ok(None);
    };
    let rowid = RemoteSource::rowid_of(id);
    conn.query_row(
        &format!("{SELECT} WHERE id = ?1 AND source = ?2"),
        params![rowid, source.as_str()],
        row_to_track,
    )
    .optional()
    .map_err(map_err("track by id"))
}

/// One track by the SERVER's own id.
pub fn track_by_item_id(conn: &Connection, source: RemoteSource, item_id: &str) -> Result<Option<CachedTrack>> {
    conn.query_row(
        &format!("{SELECT} WHERE source = ?1 AND item_id = ?2"),
        params![source.as_str(), item_id],
        row_to_track,
    )
    .optional()
    .map_err(map_err("track by item id"))
}

/// Every track of one album, in disc/track order.
///
/// NULLs sort LAST rather than first: 75 of 4924 measured Jellyfin rows carry no
/// track number, and letting them lead would put the untagged tracks above
/// track 1 on every album that has any.
pub fn album_tracks(conn: &Connection, source: RemoteSource, album_id: &str) -> Result<Vec<CachedTrack>> {
    let mut stmt = conn
        .prepare(&format!(
            "{SELECT} WHERE source = ?1 AND album_id = ?2 \
             ORDER BY disc_number IS NULL, disc_number, track_number IS NULL, track_number, title"
        ))
        .map_err(map_err("prepare album"))?;
    let rows = stmt
        .query_map(params![source.as_str(), album_id], row_to_track)
        .map_err(map_err("query album"))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_err("read album"))
}

/// Substring search across title / artist / album. An empty needle matches
/// everything, which is what the Local Library's unfiltered listing wants.
pub fn search(conn: &Connection, source: RemoteSource, needle: &str, limit: Option<u32>) -> Result<Vec<CachedTrack>> {
    let like = format!("%{}%", needle.trim());
    let lim = limit.unwrap_or(u32::MAX) as i64;
    let mut stmt = conn
        .prepare(&format!(
            "{SELECT} WHERE source = ?1 AND (?2 = '' OR title LIKE ?3 OR artist LIKE ?3 \
             OR album LIKE ?3 OR album_artist LIKE ?3) \
             ORDER BY album_artist, album, disc_number, track_number LIMIT ?4"
        ))
        .map_err(map_err("prepare search"))?;
    let rows = stmt
        .query_map(params![source.as_str(), needle.trim(), like, lim], row_to_track)
        .map_err(map_err("query search"))?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(map_err("read search"))
}

pub fn count(conn: &Connection, source: RemoteSource) -> Result<u64> {
    conn.query_row(
        "SELECT COUNT(*) FROM remote_cache_tracks WHERE source = ?1",
        params![source.as_str()],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| n as u64)
    .map_err(map_err("count"))
}

/// Forget everything one source cached (a disconnect, or a server swap).
pub fn clear(conn: &mut Connection, source: RemoteSource) -> Result<usize> {
    let tx = conn.transaction().map_err(map_err("begin"))?;
    let n = tx
        .execute(
            "DELETE FROM remote_cache_tracks WHERE source = ?1",
            params![source.as_str()],
        )
        .map_err(map_err("clear tracks"))?;
    tx.execute(
        "DELETE FROM remote_cache_libraries WHERE source = ?1",
        params![source.as_str()],
    )
    .map_err(map_err("clear libraries"))?;
    tx.commit().map_err(map_err("commit"))?;
    Ok(n)
}

/// Drop rows the last sweep did not touch — i.e. tracks deleted on the server.
///
/// Keyed on `updated_at` rather than on a set of ids: a full sweep of 50 000
/// tracks would otherwise have to hold every id in memory and build an
/// `IN (…)` of that size. `before` is the timestamp taken BEFORE the sweep
/// started, so anything still older than it was not seen.
///
/// Only ever called after a sweep that COMPLETED. A partial sweep — a dropped
/// connection halfway through — would otherwise read as "the server deleted
/// everything it did not get to".
pub fn prune_stale(conn: &mut Connection, source: RemoteSource, before: i64) -> Result<usize> {
    let tx = conn.transaction().map_err(map_err("begin"))?;
    let n = tx
        .execute(
            "DELETE FROM remote_cache_tracks WHERE source = ?1 AND updated_at < ?2",
            params![source.as_str(), before],
        )
        .map_err(map_err("prune"))?;
    tx.commit().map_err(map_err("commit"))?;
    Ok(n)
}

/// The timestamp a caller should hand to [`prune_stale`] after its sweep.
pub fn sweep_start() -> i64 {
    now()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        init_schema(&c).unwrap();
        c
    }

    fn track(item_id: &str, album: &str, disc: Option<u32>, no: Option<u32>) -> CachedTrack {
        CachedTrack {
            item_id: item_id.into(),
            album_id: album.into(),
            album: album.into(),
            title: format!("t-{item_id}"),
            artist: "A".into(),
            album_artist: "A".into(),
            disc_number: disc,
            track_number: no,
            duration_ms: 1000,
            container: "flac".into(),
            bit_depth: Some(24),
            sample_rate_hz: Some(96000),
            ..Default::default()
        }
    }

    // ── Identity ───────────────────────────────────────────────────────────

    /// The floors must not overlap each other, Plex's (`1 << 40`) or the
    /// ephemeral store's (`1 << 48`). A predicate that answered "mine" for a
    /// neighbouring source would route a row to the wrong server.
    #[test]
    fn the_id_namespaces_do_not_collide_with_each_other_or_the_existing_ones() {
        const PLEX_FLOOR: i64 = 1 << 40;
        const EPHEMERAL_FLOOR: i64 = 1 << 48;

        let j = RemoteSource::Jellyfin.namespace(1);
        let s = RemoteSource::Subsonic.namespace(1);
        assert_eq!(RemoteSource::of_id(j), Some(RemoteSource::Jellyfin));
        assert_eq!(RemoteSource::of_id(s), Some(RemoteSource::Subsonic));
        assert_ne!(j, s);

        // Neither claims a Plex id, an ephemeral id, or a plain rowid.
        assert_eq!(RemoteSource::of_id(PLEX_FLOOR | 44_440), None);
        assert_eq!(RemoteSource::of_id(EPHEMERAL_FLOOR + 10), None);
        assert_eq!(RemoteSource::of_id(2954), None);
        assert_eq!(RemoteSource::of_id(0), None);

        // ...and the round trip is exact for the whole payload range.
        for rowid in [1i64, 2, 999, PAYLOAD_MASK] {
            for src in [RemoteSource::Jellyfin, RemoteSource::Subsonic] {
                let id = src.namespace(rowid);
                assert_eq!(RemoteSource::of_id(id), Some(src), "rowid {rowid}");
                assert_eq!(RemoteSource::rowid_of(id), rowid);
            }
        }
    }

    // ── Storage ────────────────────────────────────────────────────────────

    #[test]
    fn a_saved_track_round_trips_by_both_of_its_ids() {
        let mut c = db();
        save_tracks(&mut c, RemoteSource::Jellyfin, &[track("srv-1", "alb", Some(1), Some(3))]).unwrap();
        let by_item = track_by_item_id(&c, RemoteSource::Jellyfin, "srv-1")
            .unwrap()
            .expect("by item id");
        assert_eq!(by_item.title, "t-srv-1");
        assert_eq!(by_item.bit_depth, Some(24));
        assert_eq!(RemoteSource::of_id(by_item.id), Some(RemoteSource::Jellyfin));

        let by_id = track_by_id(&c, by_item.id).unwrap().expect("by namespaced id");
        assert_eq!(by_id, by_item);
    }

    /// THE reason this is an UPSERT. A published row id lives inside queue
    /// entries and `session.db`; a re-scan that minted a new one would leave a
    /// playing track pointing at nothing.
    #[test]
    fn a_rescan_updates_a_row_without_changing_its_id() {
        let mut c = db();
        save_tracks(&mut c, RemoteSource::Jellyfin, &[track("srv-1", "alb", Some(1), Some(3))]).unwrap();
        let first = track_by_item_id(&c, RemoteSource::Jellyfin, "srv-1").unwrap().unwrap();

        let mut renamed = track("srv-1", "alb", Some(1), Some(3));
        renamed.title = "retagged".into();
        renamed.bit_depth = Some(16);
        save_tracks(&mut c, RemoteSource::Jellyfin, &[renamed]).unwrap();

        let second = track_by_item_id(&c, RemoteSource::Jellyfin, "srv-1").unwrap().unwrap();
        assert_eq!(second.id, first.id, "the row id changed under a re-scan");
        assert_eq!(second.title, "retagged");
        assert_eq!(second.bit_depth, Some(16));
        assert_eq!(count(&c, RemoteSource::Jellyfin).unwrap(), 1);
    }

    /// The same `item_id` on two DIFFERENT servers is two different tracks.
    /// Subsonic and Jellyfin ids are opaque and can coincide.
    #[test]
    fn the_same_item_id_under_two_sources_is_two_rows() {
        let mut c = db();
        save_tracks(&mut c, RemoteSource::Jellyfin, &[track("same", "a", None, None)]).unwrap();
        save_tracks(&mut c, RemoteSource::Subsonic, &[track("same", "a", None, None)]).unwrap();
        assert_eq!(count(&c, RemoteSource::Jellyfin).unwrap(), 1);
        assert_eq!(count(&c, RemoteSource::Subsonic).unwrap(), 1);
        let j = track_by_item_id(&c, RemoteSource::Jellyfin, "same").unwrap().unwrap();
        let s = track_by_item_id(&c, RemoteSource::Subsonic, "same").unwrap().unwrap();
        assert_ne!(j.id, s.id);
        assert_eq!(RemoteSource::of_id(j.id), Some(RemoteSource::Jellyfin));
        assert_eq!(RemoteSource::of_id(s.id), Some(RemoteSource::Subsonic));
    }

    /// Untagged rows sort LAST. 75 of 4924 measured Jellyfin rows have no track
    /// number; leading with them would put them above track 1 on every album.
    #[test]
    fn album_order_puts_untagged_tracks_after_the_numbered_ones() {
        let mut c = db();
        save_tracks(
            &mut c,
            RemoteSource::Jellyfin,
            &[
                track("c", "alb", Some(1), None),
                track("b", "alb", Some(1), Some(2)),
                track("a", "alb", Some(1), Some(1)),
                track("d", "alb", Some(2), Some(1)),
            ],
        )
        .unwrap();
        let got: Vec<String> = album_tracks(&c, RemoteSource::Jellyfin, "alb")
            .unwrap()
            .into_iter()
            .map(|t| t.item_id)
            .collect();
        assert_eq!(got, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn search_matches_every_display_field_and_an_empty_needle_matches_all() {
        let mut c = db();
        let mut t = track("x", "Kind of Blue", Some(1), Some(1));
        t.artist = "Miles Davis".into();
        t.album_artist = "Miles Davis".into();
        t.title = "So What".into();
        save_tracks(&mut c, RemoteSource::Subsonic, &[t]).unwrap();

        for needle in ["So What", "miles", "Kind of", "so wh"] {
            assert_eq!(
                search(&c, RemoteSource::Subsonic, needle, None).unwrap().len(),
                1,
                "needle {needle:?} found nothing"
            );
        }
        assert_eq!(search(&c, RemoteSource::Subsonic, "", None).unwrap().len(), 1);
        assert!(search(&c, RemoteSource::Subsonic, "zzz", None).unwrap().is_empty());
        // A search never leaks another source's rows.
        assert!(search(&c, RemoteSource::Jellyfin, "", None).unwrap().is_empty());
    }

    #[test]
    fn clearing_one_source_leaves_the_other_alone() {
        let mut c = db();
        save_tracks(&mut c, RemoteSource::Jellyfin, &[track("j", "a", None, None)]).unwrap();
        save_tracks(&mut c, RemoteSource::Subsonic, &[track("s", "a", None, None)]).unwrap();
        assert_eq!(clear(&mut c, RemoteSource::Jellyfin).unwrap(), 1);
        assert_eq!(count(&c, RemoteSource::Jellyfin).unwrap(), 0);
        assert_eq!(count(&c, RemoteSource::Subsonic).unwrap(), 1);
    }

    /// A row deleted on the server disappears; a row the sweep re-saw survives.
    #[test]
    fn prune_drops_only_what_the_sweep_did_not_touch() {
        let mut c = db();
        save_tracks(
            &mut c,
            RemoteSource::Subsonic,
            &[track("keep", "a", None, None), track("gone", "a", None, None)],
        )
        .unwrap();
        // A later sweep re-sees only `keep`. `updated_at` is whole seconds, so
        // the timestamps are set explicitly rather than raced against the clock.
        c.execute(
            "UPDATE remote_cache_tracks SET updated_at = 100 WHERE item_id = 'gone'",
            [],
        )
        .unwrap();
        c.execute(
            "UPDATE remote_cache_tracks SET updated_at = 200 WHERE item_id = 'keep'",
            [],
        )
        .unwrap();
        assert_eq!(prune_stale(&mut c, RemoteSource::Subsonic, 150).unwrap(), 1);
        assert_eq!(count(&c, RemoteSource::Subsonic).unwrap(), 1);
        assert!(track_by_item_id(&c, RemoteSource::Subsonic, "keep").unwrap().is_some());
        assert!(track_by_item_id(&c, RemoteSource::Subsonic, "gone").unwrap().is_none());
    }

    /// AUTOINCREMENT, not plain rowid: a freed id must never be handed to a
    /// different track, because the old one may still be sitting in a queue.
    #[test]
    fn a_deleted_rows_id_is_never_reused() {
        let mut c = db();
        save_tracks(&mut c, RemoteSource::Jellyfin, &[track("first", "a", None, None)]).unwrap();
        let first = track_by_item_id(&c, RemoteSource::Jellyfin, "first").unwrap().unwrap();
        clear(&mut c, RemoteSource::Jellyfin).unwrap();
        save_tracks(&mut c, RemoteSource::Jellyfin, &[track("second", "a", None, None)]).unwrap();
        let second = track_by_item_id(&c, RemoteSource::Jellyfin, "second").unwrap().unwrap();
        assert_ne!(
            second.id, first.id,
            "the deleted track's id was recycled — a stale queue entry would now play this row"
        );
    }

    #[test]
    fn libraries_round_trip_per_source() {
        let mut c = db();
        save_libraries(
            &mut c,
            RemoteSource::Jellyfin,
            &[CachedLibrary {
                source: "jellyfin".into(),
                library_id: "lib1".into(),
                name: "Music".into(),
                server_id: "srv".into(),
            }],
        )
        .unwrap();
        let got = libraries(&c, RemoteSource::Jellyfin).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].name, "Music");
        assert!(libraries(&c, RemoteSource::Subsonic).unwrap().is_empty());
    }

    #[test]
    fn a_lossy_row_keeps_its_absent_quality_through_the_database() {
        let mut c = db();
        let mut t = track("mp3", "a", None, None);
        t.bit_depth = None;
        t.sample_rate_hz = Some(44100);
        t.container = "mp3".into();
        save_tracks(&mut c, RemoteSource::Subsonic, &[t]).unwrap();
        let got = track_by_item_id(&c, RemoteSource::Subsonic, "mp3").unwrap().unwrap();
        assert_eq!(got.bit_depth, None, "NULL came back as something else");
        assert_eq!(got.sample_rate_hz, Some(44100));
    }
}
