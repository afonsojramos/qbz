//! The library sweep: a media server's catalog into the shared cache.
//!
//! Lives here rather than in `qbz-source` because it needs the tokio runtime
//! and a channel to the progress UI, and that crate deliberately has neither
//! (design 02 §8). It writes through the cache handle the SOURCE owns, so the
//! sweep and every read agree about which user's mirror they are touching.
//!
//! # The two protocols cost very different amounts, and the code says so
//!
//! Measured on 2026-08-20:
//!
//! | | rows | wall | per track |
//! |---|---|---|---|
//! | Jellyfin | 4924 | **45.8 s** | 9.3 ms |
//! | Subsonic | 6678 | **0.81 s** | 0.12 ms |
//!
//! Jellyfin's cost is server-side media-info hydration, demanded by
//! `Fields=MediaSources` — the only way to get `BitDepth` / `SampleRate`.
//! `Fields=MediaStreams` trims 29 % of the bytes and saves nothing. Subsonic
//! ships the same facts as ordinary OpenSubsonic song fields, for free.
//!
//! So Jellyfin gets a progress report and a delta path; Subsonic needs neither
//! and is simply run to completion.
//!
//! # Two rules the prune depends on
//!
//! 1. **`prune_stale` runs only after a sweep that COMPLETED.** It deletes rows
//!    the sweep did not touch, which is how a track deleted on the server
//!    disappears here. A connection dropped halfway would otherwise read as
//!    "the server deleted everything the sweep never got to".
//! 2. **A delta sweep never prunes**, for the same reason with the sign
//!    flipped: it deliberately does not see unchanged rows, so every one of
//!    them looks stale.

use std::sync::atomic::{AtomicBool, Ordering};

use qbz_app::settings::media_servers::{MediaServerKind, MediaServerSettings};
use qbz_media_cache::{CachedLibrary, CachedTrack, RemoteSource};

/// One sweep at a time, per source. A second one would fight the first for the
/// cache's write lock and double the server's load to produce the same rows.
static JELLYFIN_BUSY: AtomicBool = AtomicBool::new(false);
static SUBSONIC_BUSY: AtomicBool = AtomicBool::new(false);

fn busy_flag(kind: MediaServerKind) -> &'static AtomicBool {
    match kind {
        MediaServerKind::Jellyfin => &JELLYFIN_BUSY,
        MediaServerKind::Subsonic => &SUBSONIC_BUSY,
    }
}

/// Is a sweep running for this server right now?
pub fn is_syncing(kind: MediaServerKind) -> bool {
    busy_flag(kind).load(Ordering::Relaxed)
}

/// RAII guard so an early return — or a `?` — cannot leave the flag stuck on.
/// A stuck flag means the sync button never works again until a restart.
struct BusyGuard(&'static AtomicBool);

impl BusyGuard {
    fn acquire(kind: MediaServerKind) -> Option<Self> {
        let flag = busy_flag(kind);
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| BusyGuard(flag))
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// What a finished sweep reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncReport {
    /// Rows written or refreshed.
    pub saved: usize,
    /// Rows deleted because the server no longer has them. Always 0 for a
    /// delta sweep — see the module header.
    pub pruned: usize,
    /// Tracks in the cache afterwards.
    pub total: u64,
}

/// Push a progress line to the UI. Cheap enough to call per page; the pages are
/// seconds apart on Jellyfin and there are 14 of them on Subsonic.
fn report(kind: MediaServerKind, done: u64, total: u64) {
    log::info!("[qbz-qt] {} sync: {done}/{total}", kind.as_str());
    let text = format!("{done}/{total}");
    crate::local_bridge::ui(move |mut b| {
        b.as_mut()
            .set_media_sync_progress(cxx_qt_lib::QString::from(text.as_str()));
    });
}

/// Raise/lower the spinner flag and clear the progress text when it goes down.
///
/// Separate from [`BusyGuard`] on purpose: that guard is process state and must
/// be released synchronously on every path, while this hop crosses to the Qt
/// thread and cannot be done from a `Drop`.
fn set_syncing_ui(on: bool) {
    crate::local_bridge::ui(move |mut b| {
        b.as_mut().set_media_syncing(on);
        if !on {
            b.as_mut()
                .set_media_sync_progress(cxx_qt_lib::QString::default());
        }
    });
}

// ---------------------------------------------------------------------------
// Jellyfin
// ---------------------------------------------------------------------------

/// Sweep a Jellyfin server into the cache.
///
/// `full` forces a complete pass; otherwise a server that has been swept before
/// gets a DELTA (`minDateLastSaved`), which Jellyfin honours — verified: a
/// future-dated delta returns zero rows. That turns a re-sync from 45.8 s into
/// the cost of whatever actually changed.
pub async fn sync_jellyfin(full: bool) -> Result<SyncReport, String> {
    let kind = MediaServerKind::Jellyfin;
    let Some(_guard) = BusyGuard::acquire(kind) else {
        return Err("a jellyfin sync is already running".into());
    };
    let cfg = crate::media_servers_qt::get(kind);
    if !cfg.is_configured(kind) {
        return Err("jellyfin is not configured".into());
    }
    set_syncing_ui(true);
    let out = sync_jellyfin_inner(cfg, full).await;
    set_syncing_ui(false);
    out
}

async fn sync_jellyfin_inner(
    cfg: MediaServerSettings,
    full: bool,
) -> Result<SyncReport, String> {
    let kind = MediaServerKind::Jellyfin;

    let client = qbz_jellyfin::JellyfinClient::new(&cfg.base_url, &cfg.token, &cfg.username)
        .map_err(|e| e.to_string())?;
    // `username` holds the Jellyfin USER ID for the API's purposes — see
    // `connect_jellyfin`, which stores the id the auth response returned rather
    // than the typed name, because every `/Items` call keys on the id.
    let libraries = client.music_libraries().await.map_err(|e| e.to_string())?;
    if libraries.is_empty() {
        return Err("this jellyfin server exposes no music library".into());
    }
    let wanted: Vec<&qbz_jellyfin::MusicLibrary> = if cfg.selected_libraries.is_empty() {
        // Never chosen: take them all. Matching the Plex flow, where the first
        // fetch default-selects everything rather than showing an empty grid
        // until the user opens settings.
        libraries.iter().collect()
    } else {
        libraries
            .iter()
            .filter(|l| cfg.selected_libraries.contains(&l.id))
            .collect()
    };

    let delta = (!full && cfg.last_sync_at > 0).then(|| iso8601(cfg.last_sync_at));
    let started = qbz_media_cache::sweep_start();
    let mut saved = 0usize;

    for lib in &wanted {
        let total = client
            .track_count(Some(&lib.id))
            .await
            .map_err(|e| e.to_string())?;
        let mut offset = 0u64;
        loop {
            let (page, _) = client
                .tracks_page(Some(&lib.id), offset, delta.as_deref())
                .await
                .map_err(|e| e.to_string())?;
            if page.is_empty() {
                break;
            }
            let rows: Vec<CachedTrack> = page
                .iter()
                .map(|t| jellyfin_row(t, &cfg.server_id, &lib.id))
                .collect();
            saved += write_rows(RemoteSource::Jellyfin, &rows)?;
            offset += page.len() as u64;
            report(kind, offset.min(total), total);
            if page.len() < qbz_jellyfin::PAGE_SIZE as usize {
                break;
            }
        }
    }

    write_libraries(
        RemoteSource::Jellyfin,
        &libraries
            .iter()
            .map(|l| CachedLibrary {
                source: "jellyfin".into(),
                library_id: l.id.clone(),
                name: l.name.clone(),
                server_id: cfg.server_id.clone(),
            })
            .collect::<Vec<_>>(),
    )?;

    // A DELTA never prunes: it did not ask about unchanged rows, so every one
    // of them would look stale.
    let pruned = if delta.is_none() {
        prune(RemoteSource::Jellyfin, started)?
    } else {
        0
    };
    finish(kind, cfg, saved, pruned, RemoteSource::Jellyfin)
}

fn jellyfin_row(t: &qbz_jellyfin::JellyfinTrack, server_id: &str, library_id: &str) -> CachedTrack {
    CachedTrack {
        id: 0,
        source: "jellyfin".into(),
        item_id: t.id.clone(),
        server_id: server_id.to_string(),
        library_id: library_id.to_string(),
        title: t.title.clone(),
        artist: t.artist.clone(),
        album_artist: t.album_artist.clone(),
        album: t.album.clone(),
        album_id: t.album_id.clone(),
        track_number: t.track_number,
        disc_number: t.disc_number,
        duration_ms: t.duration_ms,
        year: t.year,
        genre: t.genre.clone(),
        container: t.container.clone(),
        codec: t.codec.clone(),
        bit_depth: t.bit_depth,
        sample_rate_hz: t.sample_rate_hz,
        channels: t.channels,
        bitrate_kbps: t.bitrate_bps.map(|b| b / 1000),
        // `<albumId>/<tag>`, because a Jellyfin image tag alone is not
        // addressable — the url hangs off the ITEM and the tag only versions
        // it. `JellyfinSource::artwork_token` splits it back apart.
        artwork_token: t
            .album_image_tag
            .as_ref()
            .filter(|_| !t.album_id.is_empty())
            .map(|tag| format!("{}/{}", t.album_id, tag)),
        size_bytes: None,
    }
}

/// Jellyfin wants `minDateLastSaved` as ISO-8601 UTC.
///
/// Hand-rolled from a Unix timestamp rather than pulling in `chrono`: this is
/// the only date this crate formats, the civil-calendar arithmetic below is
/// fixed-rule (proleptic Gregorian, no zones, no leap seconds), and the value
/// is only ever compared by the server against its own stamps.
fn iso8601(unix_secs: i64) -> String {
    let days = unix_secs.div_euclid(86_400);
    let secs = unix_secs.rem_euclid(86_400);
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    // Days since 1970-01-01 -> civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if mth <= 2 { y + 1 } else { y };
    format!("{year:04}-{mth:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

// ---------------------------------------------------------------------------
// Subsonic
// ---------------------------------------------------------------------------

/// Sweep a Subsonic-compatible server into the cache.
///
/// No delta — the protocol offers none — and none is needed: the whole library
/// costs under a second because quality rides along with every song.
///
/// The sweep MODE is detected rather than assumed. `search3` with an empty
/// query enumerated 6678 tracks in 14 requests on Navidrome, but that is the
/// behaviour most likely to differ on a server that was never on the bench, so
/// a probe decides and the portable `getAlbumList2` + `getAlbum` walk is the
/// fallback.
pub async fn sync_subsonic(_full: bool) -> Result<SyncReport, String> {
    let kind = MediaServerKind::Subsonic;
    let Some(_guard) = BusyGuard::acquire(kind) else {
        return Err("a subsonic sync is already running".into());
    };
    let cfg = crate::media_servers_qt::get(kind);
    let Some((base, creds)) = crate::media_servers_qt::subsonic_credentials() else {
        return Err("subsonic is not configured".into());
    };
    set_syncing_ui(true);
    let out = sync_subsonic_inner(cfg, base, creds).await;
    set_syncing_ui(false);
    out
}

async fn sync_subsonic_inner(
    cfg: MediaServerSettings,
    base: String,
    creds: qbz_subsonic::Credentials,
) -> Result<SyncReport, String> {
    let kind = MediaServerKind::Subsonic;
    let client = qbz_subsonic::SubsonicClient::new(&base, creds).map_err(|e| e.to_string())?;

    let folders = client.music_folders().await.unwrap_or_default();
    let started = qbz_media_cache::sweep_start();
    let mode = client.detect_sweep_mode().await;
    log::info!("[qbz-qt] subsonic sync: {mode:?}");
    let mut saved = 0usize;

    match mode {
        qbz_subsonic::SweepMode::Search3 => {
            let mut offset = 0u32;
            loop {
                let page = client.search_page(offset).await.map_err(|e| e.to_string())?;
                if page.is_empty() {
                    break;
                }
                let rows: Vec<CachedTrack> = page.iter().map(subsonic_row).collect();
                saved += write_rows(RemoteSource::Subsonic, &rows)?;
                offset += page.len() as u32;
                report(kind, offset as u64, offset as u64);
                if (page.len() as u32) < qbz_subsonic::PAGE_SIZE {
                    break;
                }
            }
        }
        qbz_subsonic::SweepMode::PerAlbum => {
            let mut offset = 0u32;
            let mut albums: Vec<String> = Vec::new();
            loop {
                let page = client.album_ids(offset).await.map_err(|e| e.to_string())?;
                if page.is_empty() {
                    break;
                }
                offset += page.len() as u32;
                let done = page.len() as u32;
                albums.extend(page);
                if done < qbz_subsonic::PAGE_SIZE {
                    break;
                }
            }
            let total = albums.len() as u64;
            for (i, id) in albums.iter().enumerate() {
                // One dead album must not abort a 675-request sweep. Logged and
                // skipped, exactly as `resolve_collection_tracks` treats a
                // failed item: partial beats total failure.
                match client.album_tracks(id).await {
                    Ok(t) => {
                        let rows: Vec<CachedTrack> = t.iter().map(subsonic_row).collect();
                        saved += write_rows(RemoteSource::Subsonic, &rows)?;
                    }
                    Err(e) => log::warn!("[qbz-qt] subsonic sync: album {id} failed ({e}) — skipped"),
                }
                if i % 25 == 0 {
                    report(kind, i as u64, total);
                }
            }
        }
    }

    write_libraries(
        RemoteSource::Subsonic,
        &folders
            .iter()
            .map(|f| CachedLibrary {
                source: "subsonic".into(),
                library_id: f.id.clone(),
                name: f.name.clone(),
                server_id: String::new(),
            })
            .collect::<Vec<_>>(),
    )?;

    let pruned = prune(RemoteSource::Subsonic, started)?;
    finish(kind, cfg, saved, pruned, RemoteSource::Subsonic)
}

fn subsonic_row(t: &qbz_subsonic::SubsonicTrack) -> CachedTrack {
    CachedTrack {
        id: 0,
        source: "subsonic".into(),
        item_id: t.id.clone(),
        server_id: String::new(),
        library_id: String::new(),
        title: t.title.clone(),
        artist: t.artist.clone(),
        album_artist: t.album_artist.clone(),
        album: t.album.clone(),
        album_id: t.album_id.clone(),
        track_number: t.track_number,
        disc_number: t.disc_number,
        duration_ms: t.duration_ms,
        year: t.year,
        genre: t.genre.clone(),
        container: t.suffix.clone(),
        codec: t.content_type.clone(),
        bit_depth: t.bit_depth,
        sample_rate_hz: t.sample_rate_hz,
        channels: t.channels,
        bitrate_kbps: t.bitrate_kbps,
        // The OPAQUE coverArt id, verbatim. Never parsed, never built.
        artwork_token: t.cover_art.clone(),
        size_bytes: t.size,
    }
}

// ---------------------------------------------------------------------------
// Cache plumbing — through the SOURCE's handle, never a second connection
// ---------------------------------------------------------------------------

/// The cache handle for a source. Going through the registry rather than
/// opening our own connection is what keeps the sweep and every read pointed at
/// the same user's mirror: `bind_user` moves them together.
fn handle(source: RemoteSource) -> &'static qbz_source::CacheHandle {
    match source {
        RemoteSource::Jellyfin => qbz_source::registry().jellyfin().cache(),
        RemoteSource::Subsonic => qbz_source::registry().subsonic().cache(),
    }
}

fn write_rows(source: RemoteSource, rows: &[CachedTrack]) -> Result<usize, String> {
    handle(source)
        .with_mut(|c| qbz_media_cache::save_tracks(c, source, rows))
        .ok_or_else(|| "the remote cache is not bound to a user".to_string())?
}

fn write_libraries(source: RemoteSource, libs: &[CachedLibrary]) -> Result<(), String> {
    handle(source)
        .with_mut(|c| qbz_media_cache::save_libraries(c, source, libs))
        .ok_or_else(|| "the remote cache is not bound to a user".to_string())?
}

fn prune(source: RemoteSource, started: i64) -> Result<usize, String> {
    handle(source)
        .with_mut(|c| qbz_media_cache::prune_stale(c, source, started))
        .ok_or_else(|| "the remote cache is not bound to a user".to_string())?
}

fn count(source: RemoteSource) -> u64 {
    handle(source)
        .with(|c| qbz_media_cache::count(c, source).unwrap_or(0))
        .unwrap_or(0)
}

/// Stamp the sweep and report.
///
/// `last_sync_at` is written ONLY here, at the end of a run that finished —
/// every failure path above returns early without touching it. That is what
/// makes the next delta sound: a stamp written after a partial sweep would tell
/// the server "I have everything up to now", and the rows the interrupted run
/// never saw would never be asked for again.
fn finish(
    kind: MediaServerKind,
    mut cfg: MediaServerSettings,
    saved: usize,
    pruned: usize,
    source: RemoteSource,
) -> Result<SyncReport, String> {
    let total = count(source);
    cfg.last_sync_at = qbz_media_cache::sweep_start();
    cfg.last_sync_tracks = total as i64;
    crate::media_servers_qt::put(kind, &cfg);
    log::info!(
        "[qbz-qt] {} sync finished: {saved} saved, {pruned} pruned, {total} cached",
        kind.as_str()
    );
    crate::local_catalog_qt::request_catch_up();
    Ok(SyncReport {
        saved,
        pruned,
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The delta parameter Jellyfin is handed. A wrong date is not a crash —
    /// it is a sweep that silently returns the wrong set, so the arithmetic is
    /// pinned against known instants.
    #[test]
    fn the_delta_timestamp_is_iso8601_utc() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(1_000_000_000), "2001-09-09T01:46:40Z");
        // A leap day, which is where a hand-rolled civil calendar goes wrong.
        assert_eq!(iso8601(1_709_164_800), "2024-02-29T00:00:00Z");
        assert_eq!(iso8601(1_709_251_199), "2024-02-29T23:59:59Z");
        assert_eq!(iso8601(1_709_251_200), "2024-03-01T00:00:00Z");
        // A non-leap century, the other classic off-by-one.
        assert_eq!(iso8601(951_782_400), "2000-02-29T00:00:00Z");
    }

    /// The busy flag must survive an early return. A stuck flag means the sync
    /// button never works again until the app restarts.
    #[test]
    fn the_busy_guard_releases_on_drop() {
        let kind = MediaServerKind::Jellyfin;
        assert!(!is_syncing(kind));
        {
            let _g = BusyGuard::acquire(kind).expect("first acquire");
            assert!(is_syncing(kind));
            // A second sweep is refused rather than queued.
            assert!(BusyGuard::acquire(kind).is_none());
        }
        assert!(!is_syncing(kind), "the flag stuck after the guard dropped");
        // ...and the other source was never blocked by it.
        assert!(!is_syncing(MediaServerKind::Subsonic));
    }

    /// A Jellyfin art token is only useful with the item it hangs off, so a row
    /// with a tag and no album id must carry NO token rather than a broken one.
    #[test]
    fn a_jellyfin_art_token_needs_its_album_id() {
        let mut t = qbz_jellyfin::JellyfinTrack {
            id: "i".into(),
            title: String::new(),
            artist: String::new(),
            album_artist: String::new(),
            album: String::new(),
            album_id: "alb".into(),
            track_number: None,
            disc_number: None,
            duration_ms: 0,
            year: None,
            genre: None,
            container: String::new(),
            codec: None,
            bit_depth: None,
            sample_rate_hz: None,
            channels: None,
            bitrate_bps: None,
            album_image_tag: Some("tag".into()),
            server_path: None,
        };
        assert_eq!(
            jellyfin_row(&t, "srv", "lib").artwork_token.as_deref(),
            Some("alb/tag")
        );
        t.album_id = String::new();
        assert_eq!(jellyfin_row(&t, "srv", "lib").artwork_token, None);
        t.album_id = "alb".into();
        t.album_image_tag = None;
        assert_eq!(jellyfin_row(&t, "srv", "lib").artwork_token, None);
    }

    /// Bitrate crosses the boundary in different units: Jellyfin reports bits
    /// per second, Subsonic kilobits. The cache stores kbps.
    #[test]
    fn bitrate_is_normalised_to_kbps_from_both_wires() {
        let jf = qbz_jellyfin::JellyfinTrack {
            id: "i".into(),
            title: String::new(),
            artist: String::new(),
            album_artist: String::new(),
            album: String::new(),
            album_id: "a".into(),
            track_number: None,
            disc_number: None,
            duration_ms: 0,
            year: None,
            genre: None,
            container: String::new(),
            codec: None,
            bit_depth: None,
            sample_rate_hz: None,
            channels: None,
            bitrate_bps: Some(3_120_281),
            album_image_tag: None,
            server_path: None,
        };
        assert_eq!(jellyfin_row(&jf, "s", "l").bitrate_kbps, Some(3120));
    }
}
