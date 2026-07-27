//! Artist detail data layer — Slint-free port of `crates/qbz/src/artist.rs`
//! (`load_artist`/`map_artist`, `load_release_page`, the release-section
//! bucketing in the official order). Publishes ONE JSON document.
//!
//! POC-NOTEs:
//! - MusicBrainz sidebar sections (Origin / Relationships / Discovery),
//!   the Magazine/Stories tab, blacklist banner/filter, "In library"
//!   PLAYLISTS sub-lists, jump-tab scroll-tracking: out of scope (the
//!   Network sidebar carries LABELS + SIMILAR ARTISTS, both wired).
//! - `is_following` seeds from the phase-5 library feed (the Slint app
//!   resolves it from the favorites cache — same truth, different path).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use qbz_app::shell::AppRuntime;
use qbz_core::LoggingAdapter;
use qbz_models::{PageArtistRelease, PageArtistResponse, PageArtistTrack, QueueTrack};
use serde::Serialize;

use crate::album_qt::{truncate_words, AlbumCardData, TrackRow};
use crate::home_qt;

pub const RELEASE_PAGE_SIZE: u32 = 20;

/// Official on-screen order of release buckets (artist.rs
/// RELEASE_SECTION_ORDER; titles are the msgids translated at build time).
const RELEASE_SECTION_ORDER: &[(&str, &str)] = &[
    ("album", "Albums"),
    ("epSingle", "EPs & Singles"),
    ("ep", "EPs & Singles"),
    ("single", "EPs & Singles"),
    ("live", "Live"),
    ("compilation", "Compilations"),
    ("download", "Purchase Only"),
    ("composer", "Composer"),
    ("other", "Other"),
    ("awardedRelease", "Critics' Picks"),
    ("next", "Upcoming"),
];

#[derive(Clone, Serialize)]
pub struct ArtistReleaseSection {
    #[serde(rename = "releaseType")]
    pub release_type: String,
    pub title: String,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
    pub cards: Vec<AlbumCardData>,
}

#[derive(Clone, Serialize)]
pub struct ArtistLabel {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Serialize)]
pub struct ArtistSimilar {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Serialize)]
pub struct ArtistPlaylist {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    #[serde(rename = "artUrl")]
    pub art_url: String,
}

#[derive(Default, Serialize)]
pub struct ArtistViewData {
    pub id: String,
    pub name: String,
    pub bio: String,
    #[serde(rename = "bioShort")]
    pub bio_short: String,
    #[serde(rename = "bioTruncated")]
    pub bio_truncated: bool,
    #[serde(rename = "artUrl")]
    pub artwork_url: String,
    #[serde(rename = "isFollowing")]
    pub is_following: bool,
    #[serde(rename = "isPinned")]
    pub is_pinned: bool,
    #[serde(rename = "libraryCount")]
    pub library_count: i64,
    #[serde(rename = "topTracks")]
    pub top_tracks: Vec<TrackRow>,
    #[serde(rename = "appearsOn")]
    pub appears_on: Vec<TrackRow>,
    #[serde(rename = "lastRelease")]
    pub last_release: Option<AlbumCardData>,
    #[serde(rename = "releaseSections")]
    pub release_sections: Vec<ArtistReleaseSection>,
    pub labels: Vec<ArtistLabel>,
    #[serde(rename = "similarArtists")]
    pub similar_artists: Vec<ArtistSimilar>,
    pub playlists: Vec<ArtistPlaylist>,
}

fn mmss(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// artist.rs `map_release`.
pub(crate) fn map_release(release: &PageArtistRelease) -> AlbumCardData {
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
    let artist_id = release
        .artist
        .as_ref()
        .map(|a| a.id.to_string())
        .unwrap_or_default();
    let bit_depth = release.audio_info.as_ref().and_then(|a| a.maximum_bit_depth);
    let sample_rate = release
        .audio_info
        .as_ref()
        .and_then(|a| a.maximum_sampling_rate);
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

/// artist.rs `map_track` (Popular Tracks / Appears On rows — album title +
/// cover ride the nested album object).
fn map_track(index: usize, track: PageArtistTrack) -> TrackRow {
    let mut title = track.title;
    if let Some(version) = track.version.as_ref().filter(|v| !v.is_empty()) {
        title = format!("{title} ({version})");
    }
    let (artist, artist_id) = track
        .artist
        .map(|a| (a.name.display, a.id.to_string()))
        .unwrap_or_default();
    let (album_id, album, artwork_url) = track
        .album
        .map(|a| {
            let url = a.image.and_then(|img| img.smallest().cloned()).unwrap_or_default();
            (a.id, a.title, url)
        })
        .unwrap_or_default();
    let bit_depth = track.audio_info.as_ref().and_then(|a| a.maximum_bit_depth);
    let sample_rate = track.audio_info.as_ref().and_then(|a| a.maximum_sampling_rate);
    TrackRow {
        id: track.id.to_string(),
        number: (index + 1).to_string(),
        title,
        artist,
        artist_id,
        album,
        album_id,
        duration: mmss(track.duration.unwrap_or(0)),
        quality_tier: home_qt::quality_tier_from_depth(bit_depth).to_string(),
        quality_detail: home_qt::quality_detail_from_parts(bit_depth, sample_rate),
        explicit: track.parental_warning.unwrap_or(false),
        disc: 1,
        work_header: String::new(),
        work_composer_name: String::new(),
        work_composer_id: String::new(),
        artwork_url,
    }
}

/// Fetch and map an artist page by id (artist.rs `load_artist`/`map_artist`).
pub async fn load_artist(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    artist_id: &str,
) -> Result<ArtistViewData, String> {
    let id: u64 = artist_id
        .parse()
        .map_err(|_| format!("invalid artist id: {artist_id}"))?;
    let page = runtime
        .core()
        .get_artist_page(id, None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(map_artist(page))
}

fn map_artist(page: PageArtistResponse) -> ArtistViewData {
    let name = page.name.display;
    let bio = page
        .biography
        .and_then(|b| b.content)
        .map(|c| qbz_text_utils::strip_html::strip_html(&c))
        .unwrap_or_default();
    let bio_short = truncate_words(&bio, 360);
    let bio_truncated = bio_short != bio;
    let artwork_url = page
        .images
        .and_then(|images| images.portrait)
        .map(|portrait| {
            format!(
                "https://static.qobuz.com/images/artists/covers/large/{}.{}",
                portrait.hash, portrait.format
            )
        })
        .unwrap_or_default();

    let top_tracks = page
        .top_tracks
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, track)| map_track(index, track))
        .collect();

    // Server-driven bucketing (webplayer source of truth): every non-empty
    // bucket in the official order; ids deduped across groups; labels from
    // the artist's own "album" group.
    let mut bucket_cards: HashMap<String, Vec<AlbumCardData>> = HashMap::new();
    let mut bucket_has_more: HashMap<String, bool> = HashMap::new();
    let mut seen_release_ids: HashSet<String> = HashSet::new();
    let mut labels_by_id: BTreeMap<u64, String> = BTreeMap::new();

    for group in page.releases.into_iter().flatten() {
        let release_type = group.release_type.clone();
        let is_album_group = release_type == "album";
        *bucket_has_more.entry(release_type.clone()).or_insert(false) |= group.has_more;
        for release in group.items.into_iter() {
            if seen_release_ids.contains(&release.id) {
                continue;
            }
            seen_release_ids.insert(release.id.clone());
            if is_album_group {
                if let Some(label) = release.label.as_ref() {
                    labels_by_id
                        .entry(label.id)
                        .or_insert_with(|| label.name.clone());
                }
            }
            bucket_cards
                .entry(release_type.clone())
                .or_default()
                .push(map_release(&release));
        }
    }

    let mut labels: Vec<ArtistLabel> = labels_by_id
        .into_iter()
        .map(|(id, name)| ArtistLabel {
            id: id.to_string(),
            name,
        })
        .collect();
    labels.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let similar_artists: Vec<ArtistSimilar> = page
        .similar_artists
        .map(|s| {
            s.items
                .into_iter()
                .map(|item| ArtistSimilar {
                    id: item.id.to_string(),
                    name: item.name.display,
                })
                .collect()
        })
        .unwrap_or_default();

    let playlists: Vec<ArtistPlaylist> = page
        .playlists
        .map(|p| {
            p.items
                .into_iter()
                .map(|pl| {
                    let owner = pl
                        .owner
                        .and_then(|o| o.name)
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| "Qobuz".to_string());
                    let track_count = pl.tracks_count.unwrap_or(0);
                    let art_url = pl
                        .images
                        .and_then(|imgs| imgs.rectangle)
                        .and_then(|rects| rects.into_iter().find(|s| !s.is_empty()))
                        .unwrap_or_default();
                    ArtistPlaylist {
                        id: pl.id.to_string(),
                        title: pl.title.unwrap_or_default(),
                        subtitle: format!("{owner} · {track_count}"),
                        art_url,
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let last_release = page.last_release.as_ref().map(map_release);
    let appears_on: Vec<TrackRow> = page
        .tracks_appears_on
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, track)| map_track(index, track))
        .collect();

    // One section per non-empty bucket in the official order; "download"
    // drains but never renders (Purchase Only is hidden upstream too).
    let mut release_sections: Vec<ArtistReleaseSection> = Vec::new();
    for &(rt, title) in RELEASE_SECTION_ORDER {
        if rt == "download" {
            bucket_cards.remove(rt);
            continue;
        }
        if let Some(cards) = bucket_cards.remove(rt) {
            if cards.is_empty() {
                continue;
            }
            release_sections.push(ArtistReleaseSection {
                release_type: rt.to_string(),
                title: qbz_i18n::t(title),
                has_more: bucket_has_more.get(rt).copied().unwrap_or(false),
                cards,
            });
        }
    }
    // Leftovers: server buckets unknown to the table, appended (title-cased).
    for (rt, cards) in bucket_cards {
        if cards.is_empty() {
            continue;
        }
        let title = {
            let mut chars = rt.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => rt.clone(),
            }
        };
        release_sections.push(ArtistReleaseSection {
            has_more: false,
            release_type: rt,
            title,
            cards,
        });
    }

    let (library_count, is_following) = crate::library_qt::with_library(|d| {
        let count = d
            .feed
            .iter()
            .filter(|i| {
                (i.kind == "album" || i.kind == "track") && i.artist_id == page.id.to_string()
            })
            .count() as i64;
        let following = d
            .feed
            .iter()
            .any(|i| i.kind == "artist" && i.id == page.id.to_string() && i.is_favorite);
        (count, following)
    })
    .unwrap_or((0, false));

    ArtistViewData {
        id: page.id.to_string(),
        name,
        bio,
        bio_short,
        bio_truncated,
        artwork_url,
        is_following,
        is_pinned: crate::sidebar_qt::is_pinned("artist", &page.id.to_string()),
        library_count,
        top_tracks,
        appears_on,
        last_release,
        release_sections,
        labels,
        similar_artists,
        playlists,
    }
}

/// The last loaded artist's Popular Tracks as a playable queue (row play
/// + Play-all + Shuffle-all + Play-next/queue on rows).
static TOP_QUEUE: std::sync::Mutex<Vec<QueueTrack>> = std::sync::Mutex::new(Vec::new());

fn track_row_to_queue(row: &TrackRow) -> QueueTrack {
    let duration_secs = {
        let mut parts = row.duration.split(':');
        parts.next().and_then(|m| m.parse::<u64>().ok()).unwrap_or(0) * 60
            + parts.next().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0)
    };
    QueueTrack {
        id: row.id.parse().unwrap_or(0),
        title: row.title.clone(),
        version: None,
        artist: row.artist.clone(),
        album: row.album.clone(),
        album_version: None,
        duration_secs,
        artwork_url: if row.artwork_url.is_empty() {
            None
        } else {
            Some(row.artwork_url.clone())
        },
        hires: row.quality_tier == "hires",
        bit_depth: None,
        sample_rate: None,
        is_local: false,
        album_id: if row.album_id.is_empty() {
            None
        } else {
            Some(row.album_id.clone())
        },
        artist_id: row.artist_id.parse::<u64>().ok(),
        streamable: true,
        source: Some("qobuz".to_string()),
        parental_warning: row.explicit,
        source_item_id_hint: None,
        context_kind: None,
        context_id: None,
    }
}

/// Stash the queue at publish time (called from load_artist_view).
fn stash_top_queue(data: &ArtistViewData) {
    let queue: Vec<QueueTrack> = data.top_tracks.iter().map(track_row_to_queue).collect();
    *TOP_QUEUE.lock().unwrap() = queue;
}

/// The stashed queue (cloned) + the start index for `track_id` (0 when
/// unknown — e.g. Play-all).
pub fn top_queue(track_id: Option<u64>) -> (Vec<QueueTrack>, usize) {
    let queue = TOP_QUEUE.lock().unwrap().clone();
    let start = track_id
        .and_then(|id| queue.iter().position(|t| t.id == id))
        .unwrap_or(0);
    (queue, start)
}

/// One more page of a releases bucket (artist.rs `load_release_page`).
pub async fn load_release_page(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    artist_id: &str,
    release_type: &str,
    offset: u32,
) -> Result<(Vec<AlbumCardData>, bool), String> {
    let id: u64 = artist_id
        .parse()
        .map_err(|_| format!("invalid artist id: {artist_id}"))?;
    let resp = runtime
        .core()
        .get_releases_grid(id, release_type, RELEASE_PAGE_SIZE, offset, Some("release_date"))
        .await
        .map_err(|e| e.to_string())?;
    let has_more = resp.has_more;
    let cards = resp.items.iter().map(map_release).collect();
    Ok((cards, has_more))
}

/// Fetch + publish (perf-marked like phase 5).
pub async fn load_artist_view(
    runtime: &Arc<AppRuntime<LoggingAdapter>>,
    artist_id: &str,
) -> Result<String, String> {
    let t = Instant::now();
    let data = load_artist(runtime, artist_id).await?;
    let sections: usize = data.release_sections.iter().map(|s| s.cards.len()).sum();
    stash_top_queue(&data);
    let json = serde_json::to_string(&data).map_err(|e| e.to_string())?;
    log::info!(
        "[qbz-qt][perf] artist load: {:?} ({} top tracks, {} releases in {} sections)",
        t.elapsed(),
        data.top_tracks.len(),
        sections,
        data.release_sections.len(),
    );
    Ok(json)
}
