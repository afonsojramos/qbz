//! Discover > Home data layer — a Slint-free port of the PURE mapping
//! logic of `crates/qbz/src/home.rs` (+ the personalized rails of
//! `crates/qbz/src/foryou.rs` that need only a live session).
//!
//! Produces plain data rows; serialization to the bridge happens as one
//! JSON document (`homeSectionsJson`) — see the POC-NOTE in
//! `publish_sections` for why not QVariantList-of-QVariantMap.
//!
//! POC-NOTEs (skipped vs the Slint controller):
//! - Genre filter: OUT OF SCOPE — `get_discover_index(None)` (the
//!   no-filter default).
//! - Artist/album blacklist filtering (T8): the blacklist store is not
//!   opened in this POC (phase-1 skip), so no rows are dropped.
//! - Recently-played rails (local play-history store): OUT OF SCOPE — the
//!   view renders the Slint empty-data placeholders instead.
//! - Reco-scored taste ordering of favorite albums (reco store skipped):
//!   favorites render in plain favorite order.
//! - Pinned / Most Played Albums / Qobuz Mixes rows: stores/static tiles
//!   out of scope.
//! - Discover prefs (per-tab section visibility/reorder): OUT OF SCOPE —
//!   a fixed superset order renders (see `SECTION_ORDER`).
//! - Editor's Picks / For You / Recommendations tabs: placeholder pages
//!   (the editor section set is not built).

use std::sync::Arc;

use qbz_app::shell::AppRuntime;
use qbz_core::FrontendAdapter;
use qbz_models::{
    Album, AlbumAward, Artist, DiscoverAlbum, DiscoverAudioInfo, DiscoverContainer,
    DiscoverPlaylist,
};
use serde::Serialize;

/// One card row in a section rail (superset of home.rs's CardData /
/// SlimData / PlaylistCardData / foryou ArtistSlim — unused fields stay
/// empty per section kind).
#[derive(Clone, Default, Serialize)]
pub struct HomeCard {
    pub id: String,
    pub title: String,
    pub artist: String,
    #[serde(rename = "artistId")]
    pub artist_id: String,
    pub genre: String,
    pub year: String,
    #[serde(rename = "qualityTier")]
    pub quality_tier: String,
    #[serde(rename = "qualityLabel")]
    pub quality_label: String,
    #[serde(rename = "qualityDetail")]
    pub quality_detail: String,
    pub ribbon: String,
    #[serde(rename = "ribbonKind")]
    pub ribbon_kind: String,
    #[serde(rename = "isPinned", default)]
    pub is_pinned: bool,
    /// Slim rows ("Popular albums"): the 1-based rank ("" = none).
    pub rank: String,
    /// Playlist rows: the UPPERCASE first-tag category subtag.
    pub category: String,
    #[serde(rename = "artUrl")]
    pub art_url: String,
    /// `file://<cached path>` when already on disk ("" = needs download).
    #[serde(rename = "artPath")]
    pub art_path: String,
}

#[derive(Clone, Serialize)]
pub struct HomeSection {
    pub id: String,
    pub title: String,
    /// "album" | "slim" | "playlist" | "artists" | "recentPlaceholder".
    pub kind: String,
    /// Placeholder hint for recentPlaceholder sections.
    pub hint: String,
    pub items: Vec<HomeCard>,
}

/// Push one album-carousel section from its discover container (drops
/// empty/missing containers, like home.rs `push_section`).
fn push_albums(
    out: &mut Vec<HomeSection>,
    id: &str,
    title: String,
    container: Option<DiscoverContainer<DiscoverAlbum>>,
) {
    let Some(container) = container else {
        return;
    };
    if container.data.items.is_empty() {
        return;
    }
    out.push(HomeSection {
        id: id.to_string(),
        title,
        kind: "album".to_string(),
        hint: String::new(),
        items: container.data.items.into_iter().map(map_album).collect(),
    });
}

/// The fixed render order (prefs out of scope — see module docs). Mirrors
/// the default-enabled Home order for the shared sections, then the
/// optional rails whose data is fetched anyway.
fn build_sections(
    containers: qbz_models::DiscoverContainers,
    favorite_albums: Vec<HomeCard>,
    release_watch: Vec<HomeCard>,
    top_artists: Vec<HomeCard>,
) -> Vec<HomeSection> {
    let mut out: Vec<HomeSection> = Vec::new();

    push_albums(&mut out, "newReleases", qbz_i18n::t("New Releases"), containers.new_releases);
    push_albums(&mut out, "pressAwards", qbz_i18n::t("Press Accolades"), containers.press_awards);

    // Qobuz Playlists row (single-cover cards, first-tag category subtag).
    let playlists: Vec<HomeCard> = containers
        .playlists
        .map(|c| c.data.items)
        .unwrap_or_default()
        .into_iter()
        .take(40)
        .map(map_playlist)
        .collect();
    if !playlists.is_empty() {
        out.push(HomeSection {
            id: "qobuzPlaylists".to_string(),
            title: qbz_i18n::t("Qobuz Playlists"),
            kind: "playlist".to_string(),
            hint: String::new(),
            items: playlists,
        });
    }

    // Recently-played rails are OUT OF SCOPE (local store) — the Slint
    // empty-data placeholders render instead (always present on Home).
    out.push(HomeSection {
        id: "recentlyPlayedAlbums".to_string(),
        title: qbz_i18n::t("Recently Played Albums"),
        kind: "recentPlaceholder".to_string(),
        hint: qbz_i18n::t("Albums you play will appear here."),
        items: Vec::new(),
    });
    out.push(HomeSection {
        id: "continueListening".to_string(),
        title: qbz_i18n::t("Recently Played Tracks"),
        kind: "recentPlaceholder".to_string(),
        hint: qbz_i18n::t("Tracks you play will appear here."),
        items: Vec::new(),
    });

    push_albums(
        &mut out,
        "idealDiscography",
        qbz_i18n::t("Ideal Discography"),
        containers.ideal_discography,
    );

    // Most Streamed on Home renders as the "Popular albums" slim grid,
    // capped at 24 (two carousel pages of 12), 1-based ranked.
    let popular: Vec<HomeCard> = containers
        .most_streamed
        .map(|container| container.data.items)
        .unwrap_or_default()
        .into_iter()
        .take(24)
        .enumerate()
        .map(|(index, album)| map_slim(index, album))
        .collect();
    if !popular.is_empty() {
        out.push(HomeSection {
            id: "mostStreamed".to_string(),
            title: qbz_i18n::t("Popular albums"),
            kind: "slim".to_string(),
            hint: String::new(),
            items: popular,
        });
    }

    push_albums(
        &mut out,
        "editorPicks",
        qbz_i18n::t("Albums of the Week"),
        containers.album_of_the_week,
    );
    push_albums(&mut out, "qobuzissimes", qbz_i18n::t("Qobuzissimes"), containers.qobuzissims);

    // Personalized rails (self-hide while empty, 1:1 Slint).
    if !favorite_albums.is_empty() {
        out.push(HomeSection {
            id: "favoriteAlbums".to_string(),
            title: qbz_i18n::t("Library Albums"),
            kind: "album".to_string(),
            hint: String::new(),
            items: favorite_albums,
        });
    }
    if !release_watch.is_empty() {
        out.push(HomeSection {
            id: "releaseWatch".to_string(),
            title: qbz_i18n::t("Release Watch"),
            kind: "album".to_string(),
            hint: String::new(),
            items: release_watch,
        });
    }
    if !top_artists.is_empty() {
        out.push(HomeSection {
            id: "topArtists".to_string(),
            title: qbz_i18n::t("Your Top Artists"),
            kind: "artists".to_string(),
            hint: String::new(),
            items: top_artists,
        });
    }

    out
}

/// Fetch the discover index (UNFILTERED — genre filter out of scope) and
/// the personalized rails concurrently (mirrors home.rs's `join!`), then
/// map everything into plain rows.
pub async fn load_home<A>(runtime: &Arc<AppRuntime<A>>) -> Result<Vec<HomeSection>, String>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    let (response, favorite_albums, release_watch, top_artists) = tokio::join!(
        runtime.core().get_discover_index(None),
        favorite_album_cards(runtime),
        fetch_release_watch(runtime),
        top_artist_cards(runtime),
    );
    let response = response.map_err(|e| e.to_string())?;
    log::info!(
        "[qbz-qt] discover index fetched; building home sections (fav={}, rw={}, artists={})",
        favorite_albums.len(),
        release_watch.len(),
        top_artists.len(),
    );
    Ok(build_sections(
        response.containers,
        favorite_albums,
        release_watch,
        top_artists,
    ))
}

// ---------------------------------------------------------------------------
// Pure mapping — ported from home.rs (Discover* inputs).
// ---------------------------------------------------------------------------

pub(crate) fn map_album(album: DiscoverAlbum) -> HomeCard {
    let artist = album
        .artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let artist_id = album
        .artists
        .first()
        .map(|a| a.id.to_string())
        .unwrap_or_default();
    let genre = album.genre.map(|g| g.name).unwrap_or_default();
    let year = qbz_text_utils::dates::release_label(
        album
            .dates
            .as_ref()
            .and_then(|d| d.original.as_ref().or(d.download.as_ref()).or(d.stream.as_ref()))
            .map(|s| s.as_str()),
    );
    let (ribbon, ribbon_kind) = pick_ribbon(album.awards.as_deref());
    let artwork_url = album
        .image
        .large
        .or(album.image.thumbnail)
        .or(album.image.small)
        .unwrap_or_default();
    let is_pinned = crate::sidebar_qt::is_pinned("album", &album.id);
    HomeCard {
        is_pinned,
        id: album.id,
        title: album.title,
        artist,
        artist_id,
        genre,
        year,
        quality_tier: quality_tier(album.audio_info.as_ref()).to_string(),
        quality_label: quality_label(album.audio_info.as_ref()),
        quality_detail: quality_detail(album.audio_info.as_ref()),
        ribbon,
        ribbon_kind,
        art_url: artwork_url,
        ..HomeCard::default()
    }
}

/// Map a Discover playlist into a single-cover card (1:1 home.rs
/// `map_playlist`): landscape `rectangle` preferred, first square cover
/// fallback; first tag -> UPPERCASE category subtag.
fn map_playlist(p: DiscoverPlaylist) -> HomeCard {
    let art_url = p
        .image
        .rectangle
        .or_else(|| p.image.covers.and_then(|c| c.into_iter().next()))
        .unwrap_or_default();
    let category = p
        .tags
        .as_ref()
        .and_then(|t| t.first())
        .map(|t| t.name.to_uppercase())
        .unwrap_or_default();
    HomeCard {
        id: p.id.to_string(),
        title: p.name,
        category,
        art_url,
        ..HomeCard::default()
    }
}

/// A compact ranked slim item (home.rs `map_slim`).
fn map_slim(index: usize, album: DiscoverAlbum) -> HomeCard {
    let subtitle = album
        .artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let art_url = album
        .image
        .thumbnail
        .or(album.image.small)
        .or(album.image.large)
        .unwrap_or_default();
    HomeCard {
        id: album.id,
        title: album.title,
        artist: subtitle,
        rank: (index + 1).to_string(),
        art_url,
        ..HomeCard::default()
    }
}

/// Pick the single award ribbon, mirroring `pickAlbumRibbon`: award id 151
/// = Album of the Week, 88 = Qobuzissime, otherwise the last award becomes
/// a generic "press" ribbon.
pub(crate) fn pick_ribbon(awards: Option<&[AlbumAward]>) -> (String, String) {
    let Some(awards) = awards else {
        return (String::new(), String::new());
    };
    if awards.is_empty() {
        return (String::new(), String::new());
    }
    if let Some(a) = awards.iter().find(|a| a.id.as_deref() == Some("151")) {
        return (a.name.clone(), "albumOfTheWeek".to_string());
    }
    if let Some(a) = awards.iter().find(|a| a.id.as_deref() == Some("88")) {
        return (a.name.clone(), "qobuzissime".to_string());
    }
    let last = awards.last().expect("non-empty checked above");
    (last.name.clone(), "press".to_string())
}

/// Quality tier for the icon-only badge: 24-bit and up is Hi-Res, anything
/// else with audio info is CD-quality, no audio info hides the badge.
pub(crate) fn quality_tier(audio: Option<&DiscoverAudioInfo>) -> &'static str {
    let Some(audio) = audio else {
        return "";
    };
    match audio.maximum_bit_depth {
        Some(depth) if depth >= 24 => "hires",
        _ => "cd",
    }
}

/// Exact-quality label for the badge hover tooltip
/// (`{tier}: {depth}-bit / {rate} kHz`).
pub(crate) fn quality_label(audio: Option<&DiscoverAudioInfo>) -> String {
    let Some(audio) = audio else {
        return String::new();
    };
    let hi_res = matches!(audio.maximum_bit_depth, Some(depth) if depth >= 24);
    let tier = if hi_res { "Hi-Res" } else { "CD" };
    let depth = audio
        .maximum_bit_depth
        .unwrap_or(if hi_res { 24 } else { 16 });
    let rate = audio
        .maximum_sampling_rate
        .unwrap_or(if hi_res { 96.0 } else { 44.1 });
    format!("{tier}: {depth}-bit / {} kHz", format_rate(rate))
}

/// Bare exact-quality detail ("24-bit / 96 kHz", no tier prefix).
pub(crate) fn quality_detail(audio: Option<&DiscoverAudioInfo>) -> String {
    let Some(audio) = audio else {
        return String::new();
    };
    let hi_res = matches!(audio.maximum_bit_depth, Some(depth) if depth >= 24);
    let depth = audio
        .maximum_bit_depth
        .unwrap_or(if hi_res { 24 } else { 16 });
    let rate = audio
        .maximum_sampling_rate
        .unwrap_or(if hi_res { 96.0 } else { 44.1 });
    format!("{depth}-bit / {} kHz", format_rate(rate))
}

/// Format a kHz sample rate without a trailing `.0` (96.0 -> "96").
pub(crate) fn format_rate(rate: f64) -> String {
    if (rate.fract()).abs() < f64::EPSILON {
        format!("{}", rate as i64)
    } else {
        format!("{rate}")
    }
}

/// Tier from a bare bit depth (album_map.rs `tier`): >16 is Hi-Res, a
/// known depth <= 16 is CD, unknown hides the badge.
pub(crate) fn quality_tier_from_depth(bit_depth: Option<u32>) -> &'static str {
    match bit_depth {
        Some(b) if b > 16 => "hires",
        Some(_) => "cd",
        None => "",
    }
}

/// Bare exact-quality detail from parts, Hz- or kHz-tolerant (quality.rs
/// `detail`): "24-bit / 96 kHz".
pub(crate) fn quality_detail_from_parts(bit_depth: Option<u32>, sample_rate: Option<f64>) -> String {
    let hi_res = matches!(bit_depth, Some(depth) if depth >= 24);
    let depth = bit_depth.unwrap_or(if hi_res { 24 } else { 16 });
    let rate = sample_rate.unwrap_or(if hi_res { 96.0 } else { 44.1 });
    let rate = if rate >= 1000.0 { rate / 1000.0 } else { rate };
    format!("{depth}-bit / {} kHz", format_rate(rate))
}

// ---------------------------------------------------------------------------
// Personalized rails — ported from foryou.rs (live-session only; the reco /
// blacklist stores are skipped, see module docs).
// ---------------------------------------------------------------------------

/// Flat-Album card mapping (foryou.rs `map_album`).
fn map_flat_album(album: Album) -> HomeCard {
    let year = album
        .release_date_original
        .as_deref()
        .and_then(|s| s.get(..4).map(|y| y.to_string()))
        .unwrap_or_default();
    let quality_tier = match album.maximum_bit_depth {
        Some(d) if d >= 24 => "hires",
        Some(_) => "cd",
        None => "",
    }
    .to_string();
    let quality_label = match (album.maximum_bit_depth, album.maximum_sampling_rate) {
        (Some(bd), Some(sr)) => format!("{}-bit / {} kHz", bd, sr),
        _ => String::new(),
    };
    HomeCard {
        id: album.id,
        title: album.title,
        artist: album.artist.name,
        artist_id: album.artist.id.to_string(),
        year,
        quality_tier,
        quality_label,
        art_url: album.image.best().cloned().unwrap_or_default(),
        ..HomeCard::default()
    }
}

/// "Library Albums" — favorite albums, capped at 18, in favorite order
/// (reco taste-ordering skipped — cold reco store = no reorder upstream).
async fn favorite_album_cards<A>(runtime: &Arc<AppRuntime<A>>) -> Vec<HomeCard>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    match runtime.core().get_favorites("albums", 100, 0).await {
        Ok(value) => qbz_models::lenient::parse_items_array::<Album>(&value, "albums", "home fav album")
            .into_iter()
            .take(18)
            .map(map_flat_album)
            .collect(),
        Err(e) => {
            log::warn!("[qbz-qt] favorite albums fetch failed: {e}");
            Vec::new()
        }
    }
}

/// "Release Watch" — `/release/watch` artists, capped 18 (blacklist filter
/// skipped — the store is not open in this POC).
async fn fetch_release_watch<A>(runtime: &Arc<AppRuntime<A>>) -> Vec<HomeCard>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    match runtime.core().get_release_watch("artists", 18, 0).await {
        Ok(page) => page.items.into_iter().map(map_flat_album).collect(),
        Err(e) => {
            log::warn!("[qbz-qt] release watch fetch failed: {e}");
            Vec::new()
        }
    }
}

/// "Your Top Artists" — favorite artists, capped 18.
async fn top_artist_cards<A>(runtime: &Arc<AppRuntime<A>>) -> Vec<HomeCard>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    match runtime.core().get_favorites("artists", 50, 0).await {
        Ok(value) => qbz_models::lenient::parse_items_array::<Artist>(&value, "artists", "home artist")
            .into_iter()
            .take(18)
            .map(|a| HomeCard {
                id: a.id.to_string(),
                title: a.name,
                art_url: a
                    .image
                    .and_then(|img| img.best().cloned())
                    .unwrap_or_default(),
                ..HomeCard::default()
            })
            .collect(),
        Err(e) => {
            log::warn!("[qbz-qt] favorite artists fetch failed: {e}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(depth: Option<u32>, rate: Option<f64>) -> DiscoverAudioInfo {
        DiscoverAudioInfo {
            maximum_sampling_rate: rate,
            maximum_bit_depth: depth,
            maximum_channel_count: None,
        }
    }

    fn award(id: Option<&str>, name: &str) -> AlbumAward {
        AlbumAward {
            id: id.map(|s| s.to_string()),
            name: name.to_string(),
            awarded_at: None,
        }
    }

    #[test]
    fn quality_tier_mapping() {
        assert_eq!(quality_tier(None), "");
        assert_eq!(quality_tier(Some(&audio(Some(24), Some(96.0)))), "hires");
        assert_eq!(quality_tier(Some(&audio(Some(16), Some(44.1)))), "cd");
        // Bit depth present but < 24 is CD even without a rate.
        assert_eq!(quality_tier(Some(&audio(Some(16), None))), "cd");
        // No depth at all still maps to CD (audio info exists).
        assert_eq!(quality_tier(Some(&audio(None, None))), "cd");
    }

    #[test]
    fn quality_label_and_detail() {
        assert_eq!(quality_label(None), "");
        assert_eq!(quality_detail(None), "");
        assert_eq!(
            quality_label(Some(&audio(Some(24), Some(96.0)))),
            "Hi-Res: 24-bit / 96 kHz"
        );
        assert_eq!(
            quality_label(Some(&audio(Some(16), Some(44.1)))),
            "CD: 16-bit / 44.1 kHz"
        );
        assert_eq!(
            quality_detail(Some(&audio(Some(24), Some(192.0)))),
            "24-bit / 192 kHz"
        );
        // Missing fields fall back per tier (hi-res: 24/96, cd: 16/44.1).
        assert_eq!(quality_label(Some(&audio(Some(24), None))), "Hi-Res: 24-bit / 96 kHz");
        assert_eq!(quality_detail(Some(&audio(None, Some(44.1)))), "16-bit / 44.1 kHz");
    }

    #[test]
    fn format_rate_trailing_zero() {
        assert_eq!(format_rate(96.0), "96");
        assert_eq!(format_rate(44.1), "44.1");
    }

    #[test]
    fn pick_ribbon_precedence() {
        assert_eq!(pick_ribbon(None), (String::new(), String::new()));
        assert_eq!(pick_ribbon(Some(&[])), (String::new(), String::new()));
        // 151 wins over everything.
        assert_eq!(
            pick_ribbon(Some(&[award(Some("88"), "Qobuzissime"), award(Some("151"), "AOTW")])),
            ("AOTW".to_string(), "albumOfTheWeek".to_string())
        );
        // 88 wins over a generic press award.
        assert_eq!(
            pick_ribbon(Some(&[award(Some("1"), "Press X"), award(Some("88"), "Qobuzissime")])),
            ("Qobuzissime".to_string(), "qobuzissime".to_string())
        );
        // Otherwise the LAST award is the press ribbon.
        assert_eq!(
            pick_ribbon(Some(&[award(Some("1"), "First"), award(Some("2"), "Last")])),
            ("Last".to_string(), "press".to_string())
        );
        // Awards without ids are skipped by the id lookups.
        assert_eq!(
            pick_ribbon(Some(&[award(None, "NoId")])),
            ("NoId".to_string(), "press".to_string())
        );
    }
}
