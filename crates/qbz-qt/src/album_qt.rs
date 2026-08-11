//! Album detail data layer — Slint-free port of `crates/qbz/src/album.rs`
//! (map_album: credits/meta/description/tracks) plus the three bottom
//! carousels ("From the same artist" via get_releases_grid, "Listening
//! suggestions" via get_album_suggest, "Similar albums" via Last.fm).
//! Publishes ONE JSON document.
//!
//! The page publishes PROGRESSIVELY, exactly like `navigate_album` in the
//! Slint app (`crates/qbz/src/main.rs`): `load_album_view` returns the PRIMARY
//! document — header + tracks — as soon as `/album/get` lands, and a detached
//! task then re-publishes the SAME document with each carousel folded in as it
//! resolves. Before this the whole page waited on the Last.fm row (an external
//! service plus MusicBrainz) before a single track was rendered.
//!
//! Every deferred row carries its OWN loading flag, seeded before the first
//! serialization so the view can mount a placeholder with the first frame:
//! `moreLoading`, `suggestionsLoading`, `similarLoading`. A row that CANNOT
//! resolve (no artist id, Last.fm not connected) is seeded `false` — its
//! placeholder never appears and the section stays absent, which is also what
//! an empty result produces. Late replies are generation-guarded, so a slow
//! Last.fm answer for album A can never paint onto album B.
//!
//! POC-NOTEs:
//! - Booklet download/rasterize, multi-select + bulk bar, DiscHeaderMenu,
//!   album-info modal, custom covers: out of scope (inert stubs where visible).
//! - Blacklist filtering on carousels: skipped (store not open).
//! - The album-header-gradient atmosphere: not wired (appearance pref).
//! - The Last.fm "similar albums" second suggestions row needs the
//!   `qbz-external-reco` engine, which is not a dependency of this crate —
//!   see the GLUE note in the handoff report. NOT faked here.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cxx_qt_lib::QString;
use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_models::{Album, Track};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::home_qt;

const MORE_FROM_ARTIST_MAX: usize = 16;
const RELEASE_PAGE_SIZE: u32 = 20;

#[derive(Clone, Default, Serialize)]
pub struct TrackRow {
    pub id: String,
    pub number: String,
    pub title: String,
    pub artist: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    pub album: String,
    #[serde(rename = "albumId")]
    pub album_id: String,
    pub duration: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    /// RAW catalog max bit depth / sample rate (kHz) — the SAME two numbers
    /// `quality_detail` above is derived from.
    ///
    /// THE CONTRACT (reference: `crates/qbz/src/playback.rs:2426`
    /// `make_queue_track`, which fills `bit_depth: track.maximum_bit_depth` /
    /// `sample_rate: track.maximum_sampling_rate` straight off the API model):
    /// a queue track must carry the numbers, never only the formatted string.
    /// The row builders here map a DISPLAY row into a `QueueTrack`
    /// (`artist_qt::track_row_to_queue`, `label_qt::track_row_to_queue`), so
    /// without these fields they physically could not fill them and hardcoded
    /// `None` — which zeroes `quality_state`'s `TRACK_MAX_*` seed and leaves
    /// the NPB AudioStamp with a tier and no detail line. Re-parsing
    /// `quality_detail` back into numbers would be a second source of truth
    /// (and lossy); the producer has the numbers in hand, so it passes them.
    #[serde(rename = "bitDepth", skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<u32>,
    #[serde(rename = "sampleRate", skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<f64>,
    pub explicit: bool,
    pub disc: u32,
    /// Work-section header text ("" = none; album view only).
    #[serde(rename = "workHeader")]
    pub work_header: String,
    #[serde(rename = "workComposerName")]
    pub work_composer_name: String,
    #[serde(rename = "workComposerId")]
    pub work_composer_id: String,
    /// Artwork url ("" on album view; artist top-tracks carry it).
    #[serde(rename = "artUrl")]
    pub artwork_url: String,
    /// Heart state at build time. `TrackRow.qml` reads `item.isFavorite`; the
    /// field simply did not exist, so every row on the album / artist / label
    /// pages serialized without it, QML saw `undefined`, and EVERY track heart
    /// drew empty — including on tracks the user had favourited.
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
}

/// `Deserialize` rides along with `Serialize` so a card can be read BACK out of
/// the stashed artist document: `artist_qt::sort_release_values` re-sorts a
/// release bucket that already lives on disk-shaped JSON (the stash the
/// progressive enrichment passes republish), and doing that through the typed
/// struct keeps ONE copy of the sort arms — `artist_qt::sort_release_cards` —
/// instead of a second comparator written against raw `serde_json::Value`.
/// The `#[serde(rename)]`s below are bidirectional, so the camelCase the QML
/// document carries round-trips unchanged.
#[derive(Clone, Serialize, Deserialize)]
pub struct AlbumCardData {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    pub genre: String,
    pub year: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    #[serde(rename = "artUrl")]
    pub art_url: String,
    /// Pin badge state at build time. The carousels that mount these rows
    /// (SectionRail on Album/Artist pages) draw the same AlbumCard glyph as
    /// Home, and a row that never carries the flag makes the glyph lie —
    /// the first click on an already-pinned album UN-pins it.
    #[serde(rename = "isPinned")]
    pub is_pinned: bool,
    /// Heart state at build time, twin of `home_qt::HomeCard::is_favorite`.
    /// These rows feed the SAME `cards/AlbumCard.qml` — `SectionRail.qml` and
    /// `AlbumCollection.qml` are the mounts — so the same rule applies: the
    /// glyph must not disagree with what a click will do.
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
}

#[derive(Clone, Default, Serialize)]
pub struct AlbumHeader {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    /// "Name|id|role" triples for the credits line.
    pub credits: Vec<(String, String, String)>,
    #[serde(rename = "infoLine")]
    pub info_line: String,
    #[serde(rename = "metaPre")]
    pub meta_pre: String,
    #[serde(rename = "metaPost")]
    pub meta_post: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    pub description: String,
    #[serde(rename = "descriptionShort")]
    pub description_short: String,
    #[serde(rename = "artUrl")]
    pub artwork_url: String,
    pub label: String,
    #[serde(rename = "labelId")]
    pub label_id: String,
    /// (id, name) award pairs for the sidebar.
    pub awards: Vec<(String, String)>,
    #[serde(rename = "isFavorite")]
    pub is_favorite: bool,
    #[serde(rename = "isPinned")]
    pub is_pinned: bool,
    /// Blocked state at build time, seeded from the per-user album blacklist
    /// (1:1 with `qbz/src/album.rs:683`, which seeds it right after the heart).
    /// `AlbumView.qml`'s header menu folds this through `toggleState("blocked",
    /// ...)`, so without it an ALREADY-BLOCKED album offers "Block this album"
    /// and the first click UN-blocks — the same lie the pin glyph told before
    /// `isPinned` existed.
    #[serde(rename = "isAlbumBlocked")]
    pub is_album_blocked: bool,
    /// EXTERNAL LINKS sidebar block — deep links into the three music
    /// databases, built from artist + title. These are plain URLs handed to
    /// the system browser on click: nothing is fetched, no account is needed
    /// and no integration has to be connected for them to work.
    #[serde(rename = "showExternalLinks")]
    pub show_external_links: bool,
    #[serde(rename = "lastfmUrl")]
    pub lastfm_url: String,
    #[serde(rename = "discogsUrl")]
    pub discogs_url: String,
    #[serde(rename = "musicbrainzUrl")]
    pub musicbrainz_url: String,
}

#[derive(Default, Serialize)]
pub struct AlbumViewData {
    pub header: AlbumHeader,
    pub tracks: Vec<TrackRow>,
    #[serde(rename = "moreFromArtist")]
    pub more_from_artist: Vec<AlbumCardData>,
    pub suggestions: Vec<AlbumCardData>,
    /// Last.fm "similar albums" — empty (and silent) unless the user connected
    /// Last.fm. See external_reco_qt.rs.
    #[serde(rename = "similarAlbums")]
    pub similar_albums: Vec<AlbumCardData>,
    // ---- Deferred-row gates (see the module header) ----------------------
    // True ONLY while a row that can still produce something is in flight.
    // The view mounts its skeleton on the flag and the row itself on a
    // non-empty list, so `false` + empty = the section is simply not there.
    #[serde(rename = "moreLoading")]
    pub more_loading: bool,
    #[serde(rename = "suggestionsLoading")]
    pub suggestions_loading: bool,
    #[serde(rename = "similarLoading")]
    pub similar_loading: bool,
    // ---- Header atmosphere (controls/HeaderGradient.qml) ------------------
    /// Artwork-derived header tint, "#rrggbb" ("" until the cover resolves).
    /// Patched in by `spawn_header_color` once the cover is on disk.
    #[serde(rename = "headerColor")]
    pub header_color: String,
    /// The `album_header_gradient` appearance pref as of page load. The view
    /// prefers the LIVE value off `QbzBridge.settingsJson` and falls back to
    /// this, because the settings snapshot is only published on settings-view
    /// open / mutation (main.rs `publish_settings`) — on a cold start there is
    /// no snapshot to read yet.
    #[serde(rename = "headerGradient")]
    pub header_gradient: bool,
}

// ==================== Header tint (the gradient band) ======================
//
// 1:1 port of `crates/qbz/src/artwork.rs::header_tint`, the representative
// colour the Slint hands to the album/artist header gradient
// (`AlbumState.header-color`, set in album.rs `apply_artwork`). Kept here
// rather than in artwork_qt.rs because both pages that need it are in this
// pair of modules.
//
// A plain average desaturates badly, so the average is saturation-boosted off
// its own mean and then normalized to a fixed peak brightness: bright enough
// to perceive, dark enough to keep white header text readable.

fn header_tint(pixels: &[u8]) -> (u8, u8, u8) {
    let (mut r, mut g, mut b, mut n) = (0f64, 0f64, 0f64, 0u64);
    for px in pixels.chunks_exact(4) {
        if px[3] < 16 {
            continue;
        }
        r += px[0] as f64;
        g += px[1] as f64;
        b += px[2] as f64;
        n += 1;
    }
    if n == 0 {
        return (34, 34, 42);
    }
    let nf = n as f64;
    let (mut r, mut g, mut b) = (r / nf, g / nf, b / nf);

    let mean = (r + g + b) / 3.0;
    let boost = 2.1;
    let saturate = |c: f64| (mean + (c - mean) * boost).clamp(0.0, 255.0);
    r = saturate(r);
    g = saturate(g);
    b = saturate(b);

    let peak = r.max(g).max(b).max(1.0);
    let scale = (138.0 / peak).min(1.7);
    ((r * scale) as u8, (g * scale) as u8, (b * scale) as u8)
}

/// Decode a cached cover and reduce it to its header tint, `#rrggbb`.
/// BLOCKING (decode) — call it from `spawn_blocking`. 64x64 is the sample
/// size: `header_tint` only ever computes an average, and the Slint feeds it
/// an already-downscaled decode too (artwork.rs `decode_size`).
pub(crate) fn header_tint_hex(path: &str) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let tiny = image::imageops::resize(
        &img.to_rgba8(),
        64,
        64,
        image::imageops::FilterType::Triangle,
    );
    let (r, g, b) = header_tint(tiny.as_raw());
    Some(format!("#{r:02x}{g:02x}{b:02x}"))
}

/// Resolve `artwork_url` (downloading it if it is not cached yet), reduce it
/// to a header tint and patch it into the page document. Generation-guarded
/// like every other deferred pass, so a slow cover for album A can never tint
/// album B's header.
///
/// ORDERING: unlike the carousel passes this one is NOT gated on a network
/// round-trip (an already-cached cover goes straight to the decode), so on a
/// multi-threaded runtime it can in principle reach the bridge before
/// main.rs writes the primary document and be overwritten. That is benign
/// rather than lossy: `publish_patch` mutates the STASHED document first, so
/// the tint is carried by the next republish — and the deferred carousels
/// always republish (their gates are seeded true). Worst case the band fades
/// in a beat later, never not at all.
fn spawn_header_color(generation: u64, artwork_url: String) {
    if artwork_url.is_empty() {
        return;
    }
    crate::spawn(async move {
        if crate::artwork_qt::cached_path(&artwork_url).is_empty() {
            crate::artwork_qt::download_missing(vec![artwork_url.clone()]).await;
        }
        let path = crate::artwork_qt::cached_path(&artwork_url);
        if path.is_empty() {
            return;
        }
        let path = path.trim_start_matches("file://").to_string();
        // One decode, two products: the flat tint (the FALLBACK arm) and the
        // blurred atmosphere Slint actually renders behind the header.
        let p2 = path.clone();
        let hex = tokio::task::spawn_blocking(move || header_tint_hex(&path))
            .await
            .ok()
            .flatten();
        let atmo = tokio::task::spawn_blocking(move || {
            crate::atmosphere_qt::for_cover_blocking(&p2)
        })
        .await
        .ok()
        .flatten();
        if hex.is_some() || atmo.is_some() {
            publish_patch(generation, move |doc| {
                if let Some(hex) = hex {
                    doc["headerColor"] = json!(hex);
                }
                if let Some(atmo) = atmo {
                    doc["headerAtmosphere"] = json!(atmo);
                }
            });
        }
    });
}

fn mmss(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn format_duration(secs: u32) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Last.fm path segment: percent-encode, then render spaces as `+` (Last.fm's
/// `/music/{artist}/{album}` paths use `+` for spaces). `urlencoding::encode`
/// emits `%20` for spaces, so swap them — the remaining percent-escapes
/// (e.g. `/`, `?`) stay path-safe. (album.rs `lastfm_segment`.)
fn lastfm_segment(text: &str) -> String {
    urlencoding::encode(text).replace("%20", "+")
}

/// The three external-database deep links for an album (album.rs
/// `apply_album`'s external-links block). Returns `None` when either the
/// artist or the title is missing — the sidebar block is then absent.
fn external_links(artist: &str, title: &str) -> Option<(String, String, String)> {
    if artist.is_empty() || title.is_empty() {
        return None;
    }
    let lastfm = format!(
        "https://www.last.fm/music/{}/{}",
        lastfm_segment(artist),
        lastfm_segment(title),
    );
    // `{artist}+{album}` query (spaces as `+`, each part percent-encoded).
    let query = format!(
        "{}+{}",
        urlencoding::encode(artist),
        urlencoding::encode(title)
    );
    let discogs = format!("https://www.discogs.com/search/?q={query}&type=release");
    let musicbrainz =
        format!("https://musicbrainz.org/search?query={query}&type=release&method=indexed");
    Some((lastfm, discogs, musicbrainz))
}

/// Word-boundary truncation (album.rs `truncate_words`).
pub(crate) fn truncate_words(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max).collect();
    let cut = truncated.rfind(' ').unwrap_or(truncated.len());
    format!("{}…", truncated[..cut].trim_end())
}

/// Localized readable release date ("Feb 19, 2026"); empty when absent.
fn format_release_date(iso: Option<&str>) -> String {
    let Some(raw) = iso.map(str::trim).filter(|s| !s.is_empty()) else {
        return String::new();
    };
    let head = raw.get(0..10).unwrap_or(raw);
    qbz_text_utils::dates::release_label(Some(head))
}

/// album.rs `credit_role`: "" for the main artist; the first non-main role
/// (localized via the same format_role_label path).
fn credit_role(roles: Option<&Vec<String>>) -> String {
    let Some(roles) = roles else {
        return String::new();
    };
    roles
        .iter()
        .find(|r| r.as_str() != "main-artist")
        .map(|r| qbz_i18n::t(&qbz_qobuz::performers::format_role_label(r)))
        .unwrap_or_default()
}

/// album.rs `build_credits` (releaseArtistsMapper parity, minus the
/// "VARIOUS"-composer drop it also applies).
fn build_credits(album: &Album) -> Vec<(String, String, String)> {
    let mut credits: Vec<(String, String, String)> = match album.artists.as_ref().filter(|v| !v.is_empty())
    {
        Some(list) => list
            .iter()
            .map(|a| {
                (
                    a.name.clone(),
                    a.id.to_string(),
                    credit_role(a.roles.as_ref()),
                )
            })
            .collect(),
        None => vec![(
            album.artist.name.clone(),
            album.artist.id.to_string(),
            String::new(),
        )],
    };
    // Album-level composer appended last, unless it's the localized
    // "Various Composers" placeholder (releaseArtistsMapper's VARIOUS drop).
    if let Some(composer) = album.composer.as_ref().filter(|c| !c.name.is_empty()) {
        if !composer.name.to_uppercase().contains("VARIOUS") {
            credits.push((
                composer.name.clone(),
                composer.id.to_string(),
                String::new(),
            ));
        }
    }
    credits
}

/// album.rs `map_track` (with work headers for classical albums).
fn map_track(track: &Track) -> TrackRow {
    let work = track
        .work
        .as_ref()
        .filter(|w| !w.is_empty())
        .cloned()
        .unwrap_or_default();
    let (work_composer_name, work_composer_id) = if work.is_empty() {
        (String::new(), String::new())
    } else {
        track
            .composer
            .as_ref()
            .filter(|c| !c.name.is_empty())
            .map(|c| (c.name.clone(), c.id.to_string()))
            .unwrap_or_default()
    };
    let mut title = track.title.clone();
    if let Some(version) = track.version.as_ref().filter(|v| !v.is_empty()) {
        title = format!("{title} ({version})");
    }
    let (artist, artist_id) = track
        .performer
        .as_ref()
        .map(|p| (p.name.clone(), p.id.to_string()))
        .unwrap_or_default();
    TrackRow {
        is_favorite: crate::fav_cache_qt::contains_track(track.id),
        id: track.id.to_string(),
        number: track.track_number.to_string(),
        title,
        artist,
        artist_id,
        album: String::new(),
        album_id: String::new(),
        duration: mmss(track.duration),
        quality_tier: home_qt::quality_tier_from_depth(track.maximum_bit_depth).to_string(),
        quality_detail: home_qt::quality_detail_from_parts(
            track.maximum_bit_depth,
            track.maximum_sampling_rate,
        ),
        bit_depth: track.maximum_bit_depth,
        sample_rate: track.maximum_sampling_rate,
        explicit: track.parental_warning,
        disc: track.media_number.unwrap_or(1),
        work_header: work,
        work_composer_name,
        work_composer_id,
        artwork_url: String::new(),
    }
}

/// Fetch + map the album detail (album.rs `map_album` port).
pub async fn load_album(runtime: &Arc<AppRuntime<LoggingAdapter>>, album_id: &str) -> Result<AlbumViewData, String> {
    let album = runtime
        .core()
        .get_album(album_id)
        .await
        .map_err(|e| e.to_string())?;

    let artist = album.artist.name.clone();
    let artist_id = album.artist.id.to_string();
    let credits = build_credits(&album);
    let date_display = format_release_date(
        album
            .release_date_original
            .as_deref()
            .or_else(|| album.dates.as_ref().and_then(|d| d.original.as_deref())),
    );
    let label_name = album
        .label
        .as_ref()
        .filter(|l| !l.name.is_empty())
        .map(|l| l.name.clone());
    let genre_str = album
        .genre
        .as_ref()
        .filter(|g| !g.name.is_empty())
        .map(|g| g.name.clone());
    let tracks_str = album.tracks_count.map(|count| {
        qbz_i18n::tf("{} track", "{} tracks", count as i64, &[&count.to_string()])
    });
    let duration_str = album.duration.map(format_duration);

    let mut pre_parts: Vec<String> = Vec::new();
    if !date_display.is_empty() {
        pre_parts.push(date_display);
    }
    let mut post_parts: Vec<String> = Vec::new();
    if let Some(g) = &genre_str {
        post_parts.push(g.clone());
    }
    if let Some(tc) = &tracks_str {
        post_parts.push(tc.clone());
    }
    if let Some(d) = &duration_str {
        post_parts.push(d.clone());
    }
    let meta_pre = pre_parts.join("   •   ");
    let meta_post = post_parts.join("   •   ");
    let mut all_parts = pre_parts.clone();
    if let Some(l) = &label_name {
        all_parts.push(l.clone());
    }
    all_parts.extend(post_parts.clone());
    let info_line = all_parts.join("   •   ");

    let description = album
        .description
        .as_deref()
        .map(qbz_text_utils::strip_html::strip_html)
        .unwrap_or_default();
    let description_short = truncate_words(&description, 360);
    let awards = album
        .awards
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|a| (a.id.clone().unwrap_or_default(), a.name.clone()))
        .filter(|(_, n)| !n.is_empty())
        .collect();
    let title = crate::album_qt::format_album_title(&album.title, album.version.as_deref());
    let tracks: Vec<TrackRow> = album
        .tracks
        .as_ref()
        .map(|container| container.items.iter().map(map_track).collect())
        .unwrap_or_default();

    // External links are built from the SAME artist/title the header shows.
    let links = external_links(&artist, &title);
    let (lastfm_url, discogs_url, musicbrainz_url) = links.clone().unwrap_or_default();

    let header = AlbumHeader {
        show_external_links: links.is_some(),
        lastfm_url,
        discogs_url,
        musicbrainz_url,
        // Through `library_qt::is_favorite`, never the raw feed: the feed is
        // filled only by `load_library_once()`, so a direct scan drew an EMPTY
        // heart on an album the user HAS favourited until Library was opened —
        // and the toggle then read the same false and re-added it.
        is_favorite: crate::library_qt::is_favorite("album", &album.id),
        is_pinned: crate::sidebar_qt::is_pinned("album", &album.id),
        is_album_blocked: crate::artist_blacklist::is_album_blacklisted(&album.id),
        id: album.id,
        title,
        artist,
        artist_id,
        credits,
        info_line,
        meta_pre,
        meta_post,
        quality_tier: home_qt::quality_tier_from_depth(album.maximum_bit_depth).to_string(),
        quality_detail: home_qt::quality_detail_from_parts(
            album.maximum_bit_depth,
            album.maximum_sampling_rate,
        ),
        description,
        description_short,
        artwork_url: album.image.best().cloned().unwrap_or_default(),
        label: album
            .label
            .as_ref()
            .map(|l| l.name.clone())
            .unwrap_or_default(),
        label_id: album
            .label
            .as_ref()
            .map(|l| l.id.to_string())
            .unwrap_or_default(),
        awards,
    };
    Ok(AlbumViewData {
        header,
        tracks,
        ..Default::default()
    })
}

pub(crate) fn format_album_title(title: &str, version: Option<&str>) -> String {
    match version.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => format!("{title} ({v})"),
        None => title.to_string(),
    }
}

/// album_map.rs `album_artist` (Slint `qbz/src/album_map.rs:168-187`):
/// `artist.name` first, then the `artists[]` array (the entry with the
/// `main-artist` role, else the first entry). `/album/suggest` returns items
/// whose flat `artist` is empty but whose `artists[]` is populated — without
/// the fallback the "Listening suggestions" cards render no artist line.
fn card_artist(album: &Album) -> (String, String) {
    if !album.artist.name.is_empty() {
        return (album.artist.name.clone(), album.artist.id.to_string());
    }
    if let Some(list) = album.artists.as_ref() {
        let pick = list
            .iter()
            .find(|a| {
                a.roles
                    .as_ref()
                    .map(|r| r.iter().any(|role| role == "main-artist"))
                    .unwrap_or(false)
            })
            .or_else(|| list.first());
        if let Some(a) = pick {
            return (a.name.clone(), a.id.to_string());
        }
    }
    (String::new(), String::new())
}

/// artist.rs `map_release` for carousel cards (deduped vs the open album).
fn map_release_card(release: &qbz_models::PageArtistRelease) -> AlbumCardData {
    let artist = release
        .artist
        .as_ref()
        .map(|a| a.name.display.clone())
        .or_else(|| {
            release
                .artists
                .as_ref()
                .and_then(|list| list.first().map(|a| a.name.clone()))
        })
        .unwrap_or_default();
    let bit_depth = release.audio_info.as_ref().and_then(|a| a.maximum_bit_depth);
    let sample_rate = release
        .audio_info
        .as_ref()
        .and_then(|a| a.maximum_sampling_rate);
    let artist_id = release
        .artist
        .as_ref()
        .map(|a| a.id.to_string())
        .unwrap_or_default();
    AlbumCardData {
        is_pinned: crate::sidebar_qt::is_pinned("album", &release.id),
        // Heart from the favourite-id cache (see `AlbumCardData::is_favorite`).
        is_favorite: crate::fav_cache_qt::is_album_favorite(&release.id),
        id: release.id.clone(),
        title: release.title.clone(),
        artist,
        artist_id,
        genre: release
            .genre
            .as_ref()
            .map(|g| g.name.clone())
            .unwrap_or_default(),
        year: release
            .dates
            .as_ref()
            .and_then(|d| d.original.as_deref())
            .and_then(|s| s.get(..4).map(|y| y.to_string()))
            .unwrap_or_default(),
        quality_tier: home_qt::quality_tier_from_depth(bit_depth).to_string(),
        quality_detail: home_qt::quality_detail_from_parts(bit_depth, sample_rate),
        art_url: release
            .image
            .as_ref()
            .and_then(|img| img.best().cloned())
            .unwrap_or_default(),
    }
}

/// "From the same artist" — get_releases_grid(artist, "album"), capped 16,
/// minus the open album.
pub async fn load_more_from_artist(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    artist_id: &str,
    current_album_id: &str,
) -> Vec<AlbumCardData> {
    let id: u64 = match artist_id.parse() {
        Ok(id) => id,
        Err(_) => return Vec::new(),
    };
    let resp = match runtime
        .core()
        .get_releases_grid(id, "album", RELEASE_PAGE_SIZE, 0, Some("release_date"))
        .await
    {
        Ok(resp) => resp,
        Err(e) => {
            log::warn!("[qbz-qt] more-from-artist load failed: {e}");
            return Vec::new();
        }
    };
    resp.items
        .iter()
        .map(map_release_card)
        .filter(|c| c.id != current_album_id)
        .take(MORE_FROM_ARTIST_MAX)
        .collect()
}

/// One Last.fm recommendation as the card shape the two Qobuz rows already use,
/// so the third row reuses their delegate instead of a fourth card variant.
fn reco_to_card(r: qbz_external_reco::AlbumReco) -> AlbumCardData {
    AlbumCardData {
        is_pinned: crate::sidebar_qt::is_pinned("album", &r.qobuz_album_id),
        is_favorite: crate::fav_cache_qt::is_album_favorite(&r.qobuz_album_id),
        id: r.qobuz_album_id,
        title: r.title,
        artist: r.artist,
        artist_id: r.artist_id,
        genre: r.genre,
        year: r.year,
        quality_tier: r.quality_tier,
        quality_detail: r.quality_label,
        art_url: r.artwork_url,
    }
}

/// "Listening suggestions" — /album/suggest, minus the open album.
pub async fn load_suggestions(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
) -> Vec<AlbumCardData> {
    let resp = match runtime.core().get_album_suggest(album_id).await {
        Ok(r) => r,
        Err(e) => {
            log::warn!("[qbz-qt] album suggestions load failed: {e}");
            return Vec::new();
        }
    };
    resp.albums
        .map(|page| page.items)
        .unwrap_or_default()
        .iter()
        .map(|a| {
            let bit_depth = a
                .audio_info
                .as_ref()
                .and_then(|ai| ai.maximum_bit_depth)
                .or(a.maximum_bit_depth);
            let sample_rate = a
                .audio_info
                .as_ref()
                .and_then(|ai| ai.maximum_sampling_rate)
                .or(a.maximum_sampling_rate);
            let date = a
                .dates
                .as_ref()
                .and_then(|d| d.original.clone().or(d.download.clone()).or(d.stream.clone()))
                .or(a.release_date_original.clone());
            let (artist, card_artist_id) = card_artist(a);
            AlbumCardData {
                is_pinned: crate::sidebar_qt::is_pinned("album", &a.id),
                is_favorite: crate::fav_cache_qt::is_album_favorite(&a.id),
                id: a.id.clone(),
                title: a.title.clone(),
                artist,
                artist_id: card_artist_id,
                genre: a.genre.as_ref().map(|g| g.name.clone()).unwrap_or_default(),
                year: qbz_text_utils::dates::release_label(date.as_deref()),
                quality_tier: home_qt::quality_tier_from_depth(bit_depth).to_string(),
                quality_detail: home_qt::quality_detail_from_parts(bit_depth, sample_rate),
                art_url: a.image.best().cloned().unwrap_or_default(),
            }
        })
        .filter(|c| c.id != album_id)
        .collect()
}

/// Is Last.fm connected? Read from the local scrobbler config — no network,
/// no side effect. Decided BEFORE the first serialization so a disconnected
/// user never sees a placeholder for a row that will never be requested
/// (integrations are strictly opt-in).
fn lastfm_connected() -> bool {
    let cfg = crate::integrations_qt::scrobble_settings();
    cfg.lastfm_is_authed() && !cfg.lastfm_username.is_empty()
}

/// Fetch + publish the PRIMARY album document (header + tracks) and hand it
/// back for the bridge, then resolve the three carousels in the background.
/// Perf-marked like phase 5; the mark now measures the time to a USABLE page.
pub async fn load_album_view(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
) -> Result<String, String> {
    let t = Instant::now();
    let mut data = load_album(runtime, album_id).await?;
    let artist_id = data.header.artist_id.clone();
    let artist_name = data.header.artist.clone();

    // Seed the deferred-row gates BEFORE serializing so the view paints their
    // placeholders with the first frame. A row with nothing to ask for is
    // false from the start: no skeleton, no empty frame, no request.
    let lastfm_on = lastfm_connected();
    data.more_loading = !artist_id.is_empty();
    data.suggestions_loading = true;
    data.similar_loading = lastfm_on;
    // Header atmosphere: the pref rides the document (cold-start fallback —
    // see the field comment); the tint itself needs the cover on disk, so it
    // lands as a patch a moment later. `headerColor` stays "" until then and
    // the band simply does not paint — no colour pop.
    data.header_gradient = crate::settings_qt::pref_bool("album_header_gradient", true);
    let header_art = data.header.artwork_url.clone();

    let json = serde_json::to_string(&data).map_err(|e| e.to_string())?;
    log::info!(
        "[qbz-qt][perf] album primary doc: {:?} ({} tracks; rows deferred)",
        t.elapsed(),
        data.tracks.len(),
    );

    // Stash the document for the deferred passes, under a fresh generation.
    let generation = ALBUM_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    match serde_json::from_str::<serde_json::Value>(&json) {
        Ok(doc) => {
            if let Ok(mut guard) = ALBUM_DOC.lock() {
                *guard = Some((generation, doc));
            }
        }
        Err(e) => log::warn!("[qbz-qt] album doc stash failed: {e}"),
    }

    spawn_header_color(generation, header_art);
    spawn_deferred_rows(
        runtime.clone(),
        generation,
        album_id.to_string(),
        artist_id,
        artist_name,
        lastfm_on,
    );

    Ok(json)
}

// ==================== Deferred carousels (progressive publish) =============
//
// Same machinery as `artist_qt.rs`'s enrichment pass, aimed at the album
// bridge: a monotonic generation plus the last published document, so a
// partial update is merged into it and the WHOLE document re-published (the
// bridge carries exactly one JSON property, phase-23 pattern).
//
// ORDERING: `load_album_view` returns the primary json and main.rs queues it
// onto the Qt thread with no await in between, while every pass below is
// gated on a network round-trip first — so the primary publish always
// precedes the first patch. (Identical assumption to artist_qt.rs.)

/// Monotonic id for the album page currently on screen.
static ALBUM_GEN: AtomicU64 = AtomicU64::new(0);
/// The last published album document, kept so a partial update can be merged.
static ALBUM_DOC: Mutex<Option<(u64, serde_json::Value)>> = Mutex::new(None);

/// Merge `f`'s edits into the stashed document and re-publish it, but only
/// while `generation` is still the page on screen.
fn publish_patch(generation: u64, f: impl FnOnce(&mut serde_json::Value)) {
    let json = {
        let Ok(mut guard) = ALBUM_DOC.lock() else {
            return;
        };
        let Some((current, doc)) = guard.as_mut() else {
            return;
        };
        if *current != generation {
            return;
        }
        f(doc);
        match serde_json::to_string(&*doc) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("[qbz-qt] album doc republish failed: {e}");
                return;
            }
        }
    };
    crate::album_bridge::ui(move |mut b| {
        b.as_mut().set_album_json(QString::from(json.as_str()));
    });
}

/// Write one carousel's cards + clear its loading flag, in one republish.
fn publish_row(
    generation: u64,
    cards_key: &'static str,
    loading_key: &'static str,
    cards: &[AlbumCardData],
) {
    let value = serde_json::to_value(cards).unwrap_or_else(|_| json!([]));
    publish_patch(generation, move |doc| {
        doc[cards_key] = value;
        doc[loading_key] = json!(false);
    });
}

/// The three bottom rows, resolved after the page is already usable. The two
/// Qobuz rows run CONCURRENTLY and each paints the moment it lands; the
/// Last.fm row runs last because it dedups against both.
fn spawn_deferred_rows(
    runtime: Arc<AppRuntime<LoggingAdapter>>,
    generation: u64,
    album_id: String,
    artist_id: String,
    artist_name: String,
    lastfm_on: bool,
) {
    crate::spawn(async move {
        let t = Instant::now();
        let (more, suggestions) = tokio::join!(
            async {
                // No artist id = nothing to ask for; the gate was already
                // seeded false, so this is a pure no-op.
                let more = if artist_id.is_empty() {
                    Vec::new()
                } else {
                    load_more_from_artist(&runtime, &artist_id, &album_id).await
                };
                publish_row(generation, "moreFromArtist", "moreLoading", &more);
                more
            },
            async {
                let suggestions = load_suggestions(&runtime, &album_id).await;
                publish_row(generation, "suggestions", "suggestionsLoading", &suggestions);
                suggestions
            },
        );
        log::info!(
            "[qbz-qt][perf] album qobuz rows: {:?} ({} more, {} suggestions)",
            t.elapsed(),
            more.len(),
            suggestions.len(),
        );

        // Last.fm row: seeded on the album's artist, excluding what the two
        // Qobuz rows already show. STRICTLY opt-in — with Last.fm not
        // connected the gate was seeded false and nothing below runs, so no
        // request leaves the process and the row is simply absent.
        if !lastfm_on {
            return;
        }
        let exclude_pairs: Vec<(String, String)> = more
            .iter()
            .chain(suggestions.iter())
            .map(|c| (c.artist.to_lowercase(), c.title.to_lowercase()))
            .collect();
        let exclude_ids: std::collections::HashSet<String> = more
            .iter()
            .chain(suggestions.iter())
            .map(|c| c.id.clone())
            .chain(std::iter::once(album_id.clone()))
            .collect();
        let similar: Vec<AlbumCardData> = crate::external_reco_qt::similar_albums(
            &runtime,
            &album_id,
            &artist_name,
            &exclude_pairs,
            &exclude_ids,
        )
        .await
        .into_iter()
        .map(reco_to_card)
        .collect();
        publish_row(generation, "similarAlbums", "similarLoading", &similar);
    });
}
