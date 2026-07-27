//! Album detail data layer — Slint-free port of `crates/qbz/src/album.rs`
//! (map_album: credits/meta/description/tracks) plus the two bottom
//! carousels ("From the same artist" via get_releases_grid, "Listening
//! suggestions" via get_album_suggest). Publishes ONE JSON document.
//!
//! POC-NOTEs:
//! - Booklet download/rasterize, external links (Last.fm/Discogs/MB),
//!   multi-select + bulk bar, DiscHeaderMenu, album-info modal, custom
//!   covers: out of scope (inert stubs where visible).
//! - Blacklist filtering on carousels: skipped (store not open).
//! - The album-header-gradient atmosphere and Last.fm suggestions row:
//!   not wired (appearance pref + Last.fm subsystem).

use std::sync::Arc;
use std::time::Instant;

use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_models::{Album, Track};
use serde::Serialize;

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
}

#[derive(Clone, Serialize)]
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
}

#[derive(Default, Serialize)]
pub struct AlbumViewData {
    pub header: AlbumHeader,
    pub tracks: Vec<TrackRow>,
    #[serde(rename = "moreFromArtist")]
    pub more_from_artist: Vec<AlbumCardData>,
    pub suggestions: Vec<AlbumCardData>,
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

    let header = AlbumHeader {
        is_favorite: crate::library_qt::with_library(|d| {
            d.feed
                .iter()
                .any(|i| i.kind == "album" && i.id == album.id && i.is_favorite)
        })
        .unwrap_or(false),
        is_pinned: crate::sidebar_qt::is_pinned("album", &album.id),
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
            AlbumCardData {
                id: a.id.clone(),
                title: a.title.clone(),
                artist: a.artist.name.clone(),
                artist_id: a.artist.id.to_string(),
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

/// Fetch + publish everything (perf-marked like phase 5).
pub async fn load_album_view(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    album_id: &str,
) -> Result<String, String> {
    let t = Instant::now();
    let mut data = load_album(runtime, album_id).await?;
    let artist_id = data.header.artist_id.clone();
    let (more, suggestions) = tokio::join!(
        load_more_from_artist(runtime, &artist_id, album_id),
        load_suggestions(runtime, album_id),
    );
    data.more_from_artist = more;
    data.suggestions = suggestions;
    let json = serde_json::to_string(&data).map_err(|e| e.to_string())?;
    log::info!(
        "[qbz-qt][perf] album load: {:?} ({} tracks, {} more, {} suggestions)",
        t.elapsed(),
        data.tracks.len(),
        data.more_from_artist.len(),
        data.suggestions.len(),
    );
    Ok(json)
}
