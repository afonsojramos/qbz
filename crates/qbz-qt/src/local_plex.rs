//! Plex glue for the Local Library (the Qt port of `plex_settings.rs` +
//! the browse-facing half of `plex_auth.rs`).
//!
//! ADR-006: the persisted model + SQLite store live in the shared
//! `qbz_app::settings::plex` module and the runtime lives in `qbz-plex`.
//! This file only owns:
//!
//!  1. a process-global `PlexSettingsState` bound to the ACTIVE user
//!     (`<data_dir>/qbz/users/<uid>/plex_settings.db` — the same file the
//!     Slint frontend writes, so a user authenticates Plex once);
//!  2. the gates the Local Library reads — `is_enabled` (master toggle),
//!     `is_configured` (`canUsePlexRequests`: enabled + LAN address +
//!     resolved base url + token) and `cache_db_path` (the `ATTACH` path
//!     that turns the album query into a local+Plex UNION);
//!  3. Plex-cache reads mapped into the `qbz_library::LocalTrack` shape so
//!     Plex rows flow through the SAME mapping/playback pipeline as local
//!     files (`map_cached_to_local_track`, 1:1 with the Slint);
//!  4. the manual sync (`sync_now`) the Local Library header button fires:
//!     sections fetch -> save -> default-select-ALL -> prune -> per-section
//!     track fetch + cache save.
//!
//! The PIN/auth flow (`plex_auth_pin_*`) belongs to Settings and is NOT
//! here; `connect_manual` / `disconnect` are exposed so the settings panel
//! can drive the store without duplicating it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, RwLock};

use qbz_app::user_data::UserDataPaths;
use qbz_library::LocalTrack;

pub use qbz_app::settings::plex::{PlexSettings, PlexSettingsState};

/// Namespace for Plex track ids so they can never collide with a real
/// `local_tracks.id` (1:1 with the Slint's `PLEX_TRACK_ID_FLOOR`).
pub const PLEX_TRACK_ID_FLOOR: u64 = 1 << 40;

/// True when this row id belongs to the Plex namespace.
pub fn is_plex_track_id(id: i64) -> bool {
    (id as u64) & PLEX_TRACK_ID_FLOOR != 0
}

// ---------------------------------------------------------------------------
// The per-user store handle
// ---------------------------------------------------------------------------

static STATE: LazyLock<PlexSettingsState> = LazyLock::new(PlexSettingsState::new_empty);
/// The user id the store is currently bound to (None = session-less).
static BOUND: Mutex<Option<u64>> = Mutex::new(None);

/// The last read of [`settings`], kept so the accessor is not a database hit.
///
/// # Why this exists — it is a measured regression, not a micro-optimisation
///
/// `settings()` runs `ensure_bound()` (a `load_last_user_id` file read plus a
/// mutex) and then a SQLite query. That was fine while its callers were gates
/// asked a handful of times per view.
///
/// Design 02 §9 stage 4 put it on a PER-ROW path: `local_rows::art_ref` asks
/// the owning source what a row's artwork token means, and `PlexSource`'s
/// answer depends on whether Plex is connected — so every Plex row in the grid
/// paid a file read, a mutex and a query. Measured on the owner's library:
///
/// ```text
///     29 local rows   map 0.06 ms   ->  2 µs/row
///   1271 Plex rows    map 50-117 ms -> 39-92 µs/row
/// ```
///
/// 25-45x per row, on a document that is rebuilt on every visit to Local
/// Library. The grid had been tuned to a good place and this is what took it
/// away.
///
/// The cache is INVALIDATED by every writer in this module and by
/// `init_for_user` / `reset`, so a connect, a disconnect or a user switch is
/// still seen immediately — the property `PlexCredsGlue` documents (it reads
/// the live store rather than caching a copy) is preserved, because the
/// authority itself is what caches now.
static CACHE: RwLock<Option<PlexSettings>> = RwLock::new(None);

/// Drop the memo. Call from EVERY writer — a stale `enabled` here is a Plex
/// library that will not disappear when the user switches it off.
fn invalidate_cache() {
    if let Ok(mut c) = CACHE.write() {
        *c = None;
    }
}

/// Bind (or re-bind) the store to the ACTIVE user. Called lazily by every
/// accessor, so no glue in the login path is required; `init_for_user` is
/// provided for the shell to call explicitly on session activation.
fn ensure_bound() {
    let Some(uid) = UserDataPaths::load_last_user_id() else {
        return;
    };
    let mut bound = BOUND.lock().unwrap();
    if *bound == Some(uid) {
        return;
    }
    let dir = crate::sidebar_qt::user_dir().unwrap_or_else(|| {
        dirs::data_dir()
            .unwrap_or_default()
            .join("qbz")
            .join("users")
            .join(uid.to_string())
    });
    match STATE.init_at(&dir) {
        Ok(()) => {
            *bound = Some(uid);
            log::info!("[qbz-qt] plex settings bound to user {uid}");
        }
        Err(e) => log::warn!("[qbz-qt] plex settings init failed: {e}"),
    }
}

/// Explicit bind on session activation (optional — accessors self-bind).
pub fn init_for_user(base_dir: &std::path::Path) {
    invalidate_cache();
    if let Err(e) = STATE.init_at(base_dir) {
        log::warn!("[qbz-qt] plex settings init failed: {e}");
        return;
    }
    *BOUND.lock().unwrap() = UserDataPaths::load_last_user_id();
}

/// Forget the binding (logout) so the next read re-binds to the new user.
pub fn reset() {
    invalidate_cache();
    *BOUND.lock().unwrap() = None;
    let _ = STATE.teardown();
}

/// Current persisted settings (defaults when there is no session).
pub fn settings() -> PlexSettings {
    if let Ok(c) = CACHE.read() {
        if let Some(s) = c.as_ref() {
            return s.clone();
        }
    }
    ensure_bound();
    let fresh = STATE.get_settings().unwrap_or_default();
    if let Ok(mut c) = CACHE.write() {
        *c = Some(fresh.clone());
    }
    fresh
}

// ---------------------------------------------------------------------------
// URL helpers (`normalizePlexServerUrl` / `isLocalPlexAddress` /
// `resolvePlexBaseUrl`, ported from plex_auth.rs)
// ---------------------------------------------------------------------------

fn normalize_server_url(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    }
}

/// (scheme, host-without-port, port) for `http(s)://host[:port][/...]`.
fn parse_url(normalized: &str) -> Option<(String, String, Option<String>)> {
    let (scheme, rest) = if let Some(r) = normalized.strip_prefix("http://") {
        ("http", r)
    } else if let Some(r) = normalized.strip_prefix("https://") {
        ("https", r)
    } else {
        return None;
    };
    let authority_end = rest
        .find(|c| c == '/' || c == '?' || c == '#')
        .unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() {
        return None;
    }
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let (host, port) = match authority.rfind(':') {
        Some(idx)
            if !authority[idx + 1..].is_empty()
                && authority[idx + 1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            (authority[..idx].to_string(), Some(authority[idx + 1..].to_string()))
        }
        _ => (authority.to_string(), None),
    };
    if host.is_empty() {
        return None;
    }
    Some((scheme.to_string(), host, port))
}

fn is_private_ipv4(host: &str) -> bool {
    let octets: Vec<&str> = host.split('.').collect();
    if octets.len() != 4 {
        return false;
    }
    let parsed = octets
        .iter()
        .map(|x| x.parse::<u32>().ok().filter(|v| *v <= 255))
        .collect::<Option<Vec<u32>>>();
    let Some(o) = parsed else {
        return false;
    };
    o[0] == 10
        || o[0] == 127
        || (o[0] == 192 && o[1] == 168)
        || (o[0] == 172 && (16..=31).contains(&o[1]))
}

/// `isLocalPlexAddress` — QBZ only talks to a LAN Plex server.
pub fn is_local_address(url_input: &str) -> bool {
    let normalized = normalize_server_url(url_input);
    if normalized.is_empty() {
        return false;
    }
    let Some((_, host, _)) = parse_url(&normalized) else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    host == "localhost"
        || host == "::1"
        || host.ends_with(".local")
        || host.ends_with(".lan")
        || !host.contains('.')
        || is_private_ipv4(&host)
}

/// `resolvePlexBaseUrl`: normalize + default port 32400 -> `proto://host:port`.
pub fn resolve_base_url(server_url: &str) -> String {
    let normalized = normalize_server_url(server_url);
    if normalized.is_empty() {
        return String::new();
    }
    let Some((scheme, host, port)) = parse_url(&normalized) else {
        return String::new();
    };
    if scheme != "http" && scheme != "https" {
        return String::new();
    }
    format!("{scheme}://{host}:{}", port.unwrap_or_else(|| "32400".into()))
}

// ---------------------------------------------------------------------------
// Gates
// ---------------------------------------------------------------------------

/// Master toggle only — what the browse UNION is keyed on (the Slint's
/// `plex_cache_db_path` gate: cached content stays browsable even if the
/// server is currently unreachable).
pub fn is_enabled() -> bool {
    settings().enabled
}

/// `canUsePlexRequests`: enabled && LAN && resolved base url && token. This
/// is `plex-available` — it gates the header Sync button and every request
/// that leaves the process.
pub fn is_configured() -> bool {
    let cfg = settings();
    cfg.enabled
        && is_local_address(&cfg.base_url)
        && !resolve_base_url(&cfg.base_url).is_empty()
        && !cfg.token.trim().is_empty()
}

/// `<data_dir>/qbz/plex_cache.db`, gated on the master toggle. `None` means
/// the album union degrades to local-only — identical behaviour to before
/// Plex existed.
pub fn cache_db_path() -> Option<PathBuf> {
    if !is_enabled() {
        return None;
    }
    let path = dirs::data_dir()?.join("qbz").join("plex_cache.db");
    path.exists().then_some(path)
}

// ---------------------------------------------------------------------------
// Artwork
// ---------------------------------------------------------------------------

/// A Plex thumb is a SERVER-RELATIVE path, never a filesystem path.
pub fn is_thumb_path(path: &str) -> bool {
    path.starts_with("/library/") || path.starts_with("/photo/")
}

/// Tokenized, size-transcoded thumb URL for `path` ("" when Plex is not
/// usable). `qbz_models::plex_thumb_url` is the shared builder, so the Qt
/// and Slint frontends hit the SAME cache keys.
pub fn thumb_url(path: &str, size: Option<u32>) -> String {
    let cfg = settings();
    if cfg.base_url.is_empty() || cfg.token.is_empty() {
        return String::new();
    }
    qbz_models::plex_thumb_url(&cfg.base_url, &cfg.token, path, size)
}

// ---------------------------------------------------------------------------
// Cache reads, mapped into the local-library shapes
// ---------------------------------------------------------------------------

fn parse_audio_format(s: &str) -> qbz_library::AudioFormat {
    use qbz_library::AudioFormat;
    match s.to_ascii_lowercase().as_str() {
        "flac" => AudioFormat::Flac,
        "alac" => AudioFormat::Alac,
        "wav" | "wave" => AudioFormat::Wav,
        "aiff" | "aif" => AudioFormat::Aiff,
        "ape" => AudioFormat::Ape,
        "mp3" => AudioFormat::Mp3,
        _ => AudioFormat::Unknown,
    }
}

/// Map a Plex-cache row to `LocalTrack` (1:1 with the Slint's
/// `map_plex_cached_to_local_track`): `file_path` carries the `rating_key`
/// the playback resolve needs, `artwork_path` stays the RAW `/library/...`
/// thumb path (tokenized at fetch time), `source` is `"plex"`, and the id is
/// namespaced so it cannot collide with a local row.
pub fn map_cached_to_local_track(t: qbz_plex::PlexCachedTrack) -> LocalTrack {
    LocalTrack {
        id: (PLEX_TRACK_ID_FLOOR | (t.id & (PLEX_TRACK_ID_FLOOR - 1))) as i64,
        file_path: t.rating_key,
        title: t.title,
        artist: t.artist,
        album: t.album.clone(),
        // Per-EDITION key so two same-titled Plex albums don't interleave;
        // pre-resync rows without a parent key fall back to the album hash.
        album_group_key: t
            .parent_rating_key
            .as_deref()
            .filter(|k| !k.is_empty())
            .map(|k| format!("plex:album:{k}"))
            .unwrap_or_else(|| t.album_key.clone()),
        album_group_title: t.album.clone(),
        track_number: t.track_number,
        disc_number: t.disc_number,
        duration_secs: t.duration_secs,
        format: parse_audio_format(&t.format),
        bit_depth: t.bit_depth,
        sample_rate: t.sample_rate as f64,
        artwork_path: t.artwork_path,
        source: Some("plex".to_string()),
        ..Default::default()
    }
}

/// The FULL Plex track set matching `query` (cap 5000 — the Plex cache is a
/// bounded set, not 16K-row scale), in the `LocalTrack` shape.
pub fn search_tracks(query: &str) -> Vec<LocalTrack> {
    qbz_plex::plex_cache_search_tracks(query.trim().to_string(), None)
        .unwrap_or_default()
        .into_iter()
        .map(map_cached_to_local_track)
        .collect()
}

/// One Plex album's tracks, by the content-hash key (`plex:<hash>`).
pub fn album_tracks(album_key: &str) -> Vec<LocalTrack> {
    qbz_plex::plex_cache_get_album_tracks(album_key.to_string())
        .unwrap_or_default()
        .into_iter()
        .map(map_cached_to_local_track)
        .collect()
}

/// The content-hash album key for a played Plex track (its
/// `album_group_key` is the per-edition split key, which the album cache is
/// NOT keyed by).
pub fn album_key_for(artist: &str, album: &str) -> String {
    qbz_plex::plex_album_key(artist, album)
}

pub fn cached_artists() -> Vec<qbz_plex::PlexCachedArtist> {
    qbz_plex::plex_cache_get_artists().unwrap_or_default()
}

pub fn cached_track_count() -> i64 {
    qbz_plex::plex_cache_count_tracks().unwrap_or(0) as i64
}

/// Cached library sections + which of them are selected (Settings panel).
pub fn cached_sections() -> (Vec<qbz_plex::PlexMusicSection>, Vec<String>) {
    (
        qbz_plex::plex_cache_get_sections().unwrap_or_default(),
        settings().selected_section_keys,
    )
}

// ---------------------------------------------------------------------------
// Mutations (the settings panel drives the SAME store)
// ---------------------------------------------------------------------------

pub fn set_enabled(value: bool) {
    invalidate_cache();
    ensure_bound();
    if let Err(e) = STATE.set_enabled(value) {
        log::error!("[qbz-qt] plex set_enabled failed: {e}");
    }
}

/// The stable per-install identifier plex.tv keys a PIN against. Generated
/// once and persisted by the shared store; `X-Plex-Client-Identifier` must be
/// the SAME value on the pin/start and pin/check calls or the check never
/// sees the authorization.
pub fn client_id() -> String {
    ensure_bound();
    STATE.get_or_create_client_id().unwrap_or_else(|e| {
        log::error!("[qbz-qt] plex client id failed: {e}");
        String::new()
    })
}

/// Manual-token mode: the user pasted an `X-Plex-Token` instead of signing in
/// through a PIN. The PIN path clears it on success so the panel goes back to
/// showing Authorize (`plex_auth.rs:531`).
pub fn set_manual_token_mode(value: bool) {
    invalidate_cache();
    ensure_bound();
    if let Err(e) = STATE.set_manual_token_mode(value) {
        log::error!("[qbz-qt] plex set_manual_token_mode failed: {e}");
    }
}

/// Record the server's `machineIdentifier` from a successful ping.
///
/// Its READER has been live since the port landed (`local_plex.rs`'s cache
/// rows carry `server_id`), but the only WRITER was the Slint build's
/// `ping_inner` — so a Qt-only install stamped `server_id = NULL` on every
/// cached row. Porting the ping is what closes that.
pub fn set_machine_id(value: &str) {
    invalidate_cache();
    ensure_bound();
    if let Err(e) = STATE.set_machine_id(value) {
        log::error!("[qbz-qt] plex set_machine_id failed: {e}");
    }
}

/// The persisted (resolved) base url + token, for callers that need to talk
/// to the server without going through a fresh connect.
pub fn credentials() -> (String, String) {
    let cfg = settings();
    (cfg.base_url, cfg.token)
}

// Seam for Settings > Local Library > Plex ("write metadata back to Plex").
// That settings surface is not ported yet, so nothing calls this today.
#[allow(dead_code)]
pub fn set_metadata_write_enabled(value: bool) {
    invalidate_cache();
    ensure_bound();
    if let Err(e) = STATE.set_metadata_write_enabled(value) {
        log::error!("[qbz-qt] plex set_metadata_write_enabled failed: {e}");
    }
}

/// Persist a manually entered server + token (`persistPlexConfig`): the URL
/// is RESOLVED (`proto://host:32400`) before it is stored. Returns the
/// resolved base url ("" when the input is unusable).
pub fn connect_manual(server_url: &str, token: &str) -> String {
    invalidate_cache();
    ensure_bound();
    let base = resolve_base_url(server_url);
    if base.is_empty() {
        return base;
    }
    if let Err(e) = STATE.set_credentials(&base, token.trim()) {
        log::error!("[qbz-qt] plex set_credentials failed: {e}");
        return String::new();
    }
    let _ = STATE.set_manual_token_mode(true);
    base
}

pub fn set_selected_sections(keys: &[String]) {
    invalidate_cache();
    ensure_bound();
    if let Err(e) = STATE.set_selected_section_keys(keys) {
        log::error!("[qbz-qt] plex set_selected_section_keys failed: {e}");
    }
}

/// Sign out of Plex: clear creds + sections + machine id (keeps `enabled`,
/// `client_id`, `metadata_write_enabled`) and purge the shared cache DB so
/// no Plex rows survive in the browse union.
pub fn disconnect() {
    invalidate_cache();
    ensure_bound();
    if let Err(e) = STATE.disconnect() {
        log::error!("[qbz-qt] plex disconnect failed: {e}");
    }
    if let Err(e) = qbz_plex::plex_cache_clear() {
        log::warn!("[qbz-qt] plex cache clear failed: {e}");
    }
}

// ---------------------------------------------------------------------------
// Manual sync (#573)
// ---------------------------------------------------------------------------

static SYNCING: AtomicBool = AtomicBool::new(false);

pub fn is_syncing() -> bool {
    SYNCING.load(Ordering::SeqCst)
}

/// `sync_now` + `syncSelectedPlexLibraries`: fetch the music sections, save
/// them, default-select ALL when nothing is persisted, PRUNE the de-selected
/// sections (never a full wipe — that would drop hydrated quality rows), then
/// per selected section fetch the tracks and save them into the cache DB.
/// Returns the total track count written.
///
/// Re-entrancy: a second call while a sync is in flight returns `Err` instead
/// of racing the cache writes.
pub async fn sync_now() -> Result<usize, String> {
    if SYNCING.swap(true, Ordering::SeqCst) {
        return Err("Plex sync already running".to_string());
    }
    let result = sync_inner().await;
    SYNCING.store(false, Ordering::SeqCst);
    result
}

async fn sync_inner() -> Result<usize, String> {
    let cfg = settings();
    if !is_configured() {
        return Err("Plex is not configured".to_string());
    }
    let base = cfg.base_url.clone();
    let token = cfg.token.clone();

    let sections = qbz_plex::plex_get_music_sections(base.trim().to_string(), token.trim().to_string())
        .await
        .map_err(|e| e.to_string())?;

    let machine_id = settings().machine_id;
    let server_id = (!machine_id.is_empty()).then_some(machine_id);

    {
        let sections = sections.clone();
        let server_id = server_id.clone();
        let _ = tokio::task::spawn_blocking(move || {
            qbz_plex::plex_cache_save_sections(server_id, sections)
        })
        .await;
    }

    // Default-select ALL when the persisted selection is empty / stale.
    let available: std::collections::HashSet<String> =
        sections.iter().map(|s| s.key.clone()).collect();
    let persisted: Vec<String> = settings()
        .selected_section_keys
        .into_iter()
        .filter(|k| available.contains(k))
        .collect();
    let selected: Vec<String> = if persisted.is_empty() {
        sections.iter().map(|s| s.key.clone()).collect()
    } else {
        persisted
    };
    set_selected_sections(&selected);

    // Prune ONLY the de-selected sections: a full clear would delete the
    // hydrated rows BEFORE save_tracks can carry their quality over.
    {
        let keep = selected.clone();
        let _ = tokio::task::spawn_blocking(move || qbz_plex::plex_cache_prune_sections(&keep)).await;
    }

    let mut total = 0usize;
    for key in &selected {
        let tracks = match qbz_plex::plex_get_section_tracks(
            base.trim().to_string(),
            token.trim().to_string(),
            key.clone(),
            None,
        )
        .await
        {
            Ok(t) => t,
            Err(e) => {
                log::warn!("[qbz-qt] plex get_section_tracks({key}) failed: {e}");
                continue;
            }
        };
        total += tracks.len();
        let server_id = server_id.clone();
        let key = key.clone();
        let _ = tokio::task::spawn_blocking(move || {
            qbz_plex::plex_cache_save_tracks(server_id, key, tracks)
        })
        .await;
    }
    log::info!("[qbz-qt] plex sync: {total} tracks across {} sections", selected.len());
    Ok(total)
}
