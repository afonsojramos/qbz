//! Discover > For You controller.
//!
//! Loads the personalized For You sections the Slint MVP can source
//! today and pushes them into `ForYouState`. Each section reuses an
//! existing card component (album Carousel, SlimCarousel, artist
//! ArtistCarousel). Lazy: the tab loads once on first open.
//!
//! Backed sections: Release Watch (get_release_watch), Recently
//! Played Tracks / Albums (local play-history), Your Top Artists
//! (favorites), Artists to Follow (similar artists seeded from
//! favorites). The reco-DB sections (Qobuz mixes, more from library,
//! rediscover, spotlight, radio) are separate later increments.

use std::collections::HashSet;
use std::sync::Arc;

use qbz_app::shell::AppRuntime;
use qbz_core::FrontendAdapter;
use qbz_models::{Album, Artist};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::artwork::{ArtworkJob, ArtworkTarget};
use crate::{AlbumCardItem, AppWindow, DiscoverSection, ForYouState, SlimItem};

const ARTIST_SEEDS: usize = 4;
const SIMILAR_PER_SEED: u32 = 10;
const FOLLOW_MAX: usize = 18;

pub struct ForYouData {
    pub release_watch: Vec<AlbumCard>,
    pub recent_albums: Vec<AlbumCard>,
    pub recent_tracks: Vec<TrackSlim>,
    pub top_artists: Vec<ArtistSlim>,
    pub artists_to_follow: Vec<ArtistSlim>,
    /// Favorite albums not in the recent play-history — an
    /// approximation of Tauri's reco-DB "forgotten favorites".
    pub rediscover: Vec<AlbumCard>,
    /// Albums similar to a recently-played / favorite seed album.
    pub more_from_library: Vec<AlbumCard>,
    /// Album-seeded radio tiles (recent + favorite albums).
    pub radio_stations: Vec<RadioSeed>,
    pub spotlight: Option<SpotlightData>,
}

#[derive(Clone)]
pub struct RadioSeed {
    pub album_id: String,
    pub title: String,
    pub artist: String,
    pub artwork_url: String,
}

pub struct SpotlightData {
    pub artist_id: String,
    pub artist_name: String,
    pub category: String,
    pub image_url: String,
    pub has_top_tracks: bool,
    pub albums: Vec<AlbumCard>,
}

#[derive(Clone)]
pub struct AlbumCard {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub artist_id: String,
    pub year: String,
    pub quality_tier: String,
    pub quality_label: String,
    pub artwork_url: String,
}

#[derive(Clone)]
pub struct TrackSlim {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub artwork_url: String,
}

#[derive(Clone)]
pub struct ArtistSlim {
    pub id: String,
    pub name: String,
    pub artwork_url: String,
    pub following: bool,
}

fn map_album(album: Album) -> AlbumCard {
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
    AlbumCard {
        id: album.id,
        title: album.title,
        artist: album.artist.name,
        artist_id: album.artist.id.to_string(),
        year,
        quality_tier,
        quality_label,
        artwork_url: album.image.best().cloned().unwrap_or_default(),
    }
}

fn map_artist(artist: Artist, following: bool) -> ArtistSlim {
    ArtistSlim {
        id: artist.id.to_string(),
        name: artist.name,
        artwork_url: artist
            .image
            .and_then(|img| img.best().cloned())
            .unwrap_or_default(),
        following,
    }
}

pub async fn load_for_you<A>(runtime: &Arc<AppRuntime<A>>) -> ForYouData
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    // Release Watch — new releases from followed artists.
    let release_watch: Vec<AlbumCard> = match runtime
        .core()
        .get_release_watch("artists", 18, 0)
        .await
    {
        Ok(page) => page.items.into_iter().map(map_album).collect(),
        Err(_) => Vec::new(),
    };

    // Recently played — local play-history store.
    let recent_albums: Vec<AlbumCard> = crate::recently::load_albums()
        .into_iter()
        .map(|a| AlbumCard {
            id: a.id,
            title: a.title,
            artist: a.artist,
            artist_id: String::new(),
            year: String::new(),
            quality_tier: a.quality_tier,
            quality_label: a.quality_label,
            artwork_url: a.artwork_url,
        })
        .collect();
    let recent_tracks: Vec<TrackSlim> = crate::recently::load()
        .into_iter()
        .take(24)
        .map(|t| TrackSlim {
            id: t.id,
            title: t.title,
            subtitle: t.subtitle,
            artwork_url: t.artwork_url,
        })
        .collect();

    // Your Top Artists — the user's favorite artists.
    let fav_artists: Vec<Artist> = match runtime.core().get_favorites("artists", 50, 0).await {
        Ok(value) => {
            let items = value
                .get("artists")
                .and_then(|b| b.get("items"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            serde_json::from_value(items).unwrap_or_default()
        }
        Err(_) => Vec::new(),
    };
    let favorite_ids: HashSet<u64> = fav_artists.iter().map(|a| a.id).collect();
    let top_artists: Vec<ArtistSlim> = fav_artists
        .iter()
        .take(18)
        .cloned()
        .map(|a| map_artist(a, true))
        .collect();

    // Artists to Follow — similar artists seeded from a few favorites,
    // excluding ones already followed.
    let mut to_follow: Vec<ArtistSlim> = Vec::new();
    let mut seen: HashSet<u64> = favorite_ids.clone();
    for seed in fav_artists.iter().take(ARTIST_SEEDS) {
        if to_follow.len() >= FOLLOW_MAX {
            break;
        }
        if let Ok(page) = runtime
            .core()
            .get_similar_artists(seed.id, SIMILAR_PER_SEED, 0)
            .await
        {
            for artist in page.items {
                if seen.insert(artist.id) {
                    to_follow.push(map_artist(artist, false));
                    if to_follow.len() >= FOLLOW_MAX {
                        break;
                    }
                }
            }
        }
    }

    // Favorite albums (full list) — feeds Rediscover + the More from
    // your library seed.
    let recent_album_list = crate::recently::load_albums();
    let recent_ids: HashSet<String> =
        recent_album_list.iter().map(|a| a.id.clone()).collect();
    let fav_albums: Vec<Album> = match runtime.core().get_favorites("albums", 100, 0).await {
        Ok(value) => {
            let items = value
                .get("albums")
                .and_then(|b| b.get("items"))
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            serde_json::from_value(items).unwrap_or_default()
        }
        Err(_) => Vec::new(),
    };

    // Rediscover — favorite albums not in the recent play-history.
    // (Tauri uses a reco DB tracking play recency; the Slint MVP
    // approximates with the local recently-played album set.)
    let rediscover: Vec<AlbumCard> = fav_albums
        .iter()
        .filter(|a| !recent_ids.contains(&a.id))
        .take(18)
        .cloned()
        .map(map_album)
        .collect();

    // More from your library — albums similar to a seed (most recent
    // played album, else first favorite) via /album/suggest.
    let seed_id = recent_album_list
        .first()
        .map(|a| a.id.clone())
        .or_else(|| fav_albums.first().map(|a| a.id.clone()));
    let more_from_library: Vec<AlbumCard> = match seed_id {
        Some(id) if !id.is_empty() => match runtime.core().get_album_suggest(&id).await {
            Ok(resp) => resp
                .albums
                .map(|p| p.items)
                .unwrap_or_default()
                .into_iter()
                .take(18)
                .map(map_album)
                .collect(),
            Err(_) => Vec::new(),
        },
        _ => Vec::new(),
    };

    // Radio Stations — album-seeded radio tiles from recent +
    // favorite albums, deduped, capped at 12.
    let mut radio_seen: HashSet<String> = HashSet::new();
    let mut radio_stations: Vec<RadioSeed> = Vec::new();
    for a in &recent_album_list {
        if radio_seen.insert(a.id.clone()) {
            radio_stations.push(RadioSeed {
                album_id: a.id.clone(),
                title: a.title.clone(),
                artist: a.artist.clone(),
                artwork_url: a.artwork_url.clone(),
            });
        }
    }
    for a in &fav_albums {
        if radio_stations.len() >= 12 {
            break;
        }
        if radio_seen.insert(a.id.clone()) {
            radio_stations.push(RadioSeed {
                album_id: a.id.clone(),
                title: a.title.clone(),
                artist: a.artist.name.clone(),
                artwork_url: a.image.best().cloned().unwrap_or_default(),
            });
        }
    }
    radio_stations.truncate(12);

    // Spotlight — highlight one favorite artist (rotated by time) with
    // their page (albums + whether they have top tracks).
    let spotlight = load_spotlight(runtime, &fav_artists).await;

    ForYouData {
        release_watch,
        recent_albums,
        recent_tracks,
        top_artists,
        artists_to_follow: to_follow,
        rediscover,
        more_from_library,
        radio_stations,
        spotlight,
    }
}

async fn load_spotlight<A>(
    runtime: &Arc<AppRuntime<A>>,
    favorites: &[Artist],
) -> Option<SpotlightData>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    if favorites.is_empty() {
        return None;
    }
    // Rotate among the top 5 favorites by wall-clock seconds.
    let pool = favorites.len().min(5);
    let idx = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as usize % pool)
        .unwrap_or(0);
    let seed = &favorites[idx];

    let page = runtime.core().get_artist_page(seed.id, None).await.ok()?;
    let image_url = page
        .images
        .as_ref()
        .and_then(|i| i.portrait.as_ref())
        .map(|p| {
            format!(
                "https://static.qobuz.com/images/artists/covers/medium/{}.{}",
                p.hash, p.format
            )
        })
        .unwrap_or_default();

    // Up to 6 albums, preferring full albums then live/ep/compilation.
    let mut seen: HashSet<String> = HashSet::new();
    let mut albums: Vec<AlbumCard> = Vec::new();
    for want in ["album", "live", "ep-single", "compilation"] {
        if albums.len() >= 6 {
            break;
        }
        let Some(groups) = page.releases.as_ref() else {
            break;
        };
        let Some(group) = groups.iter().find(|g| g.release_type == want) else {
            continue;
        };
        for rel in &group.items {
            if !seen.insert(rel.id.clone()) {
                continue;
            }
            let year = rel
                .dates
                .as_ref()
                .and_then(|d| d.original.as_deref())
                .and_then(|s| s.get(..4).map(|y| y.to_string()))
                .unwrap_or_default();
            let bd = rel.audio_info.as_ref().and_then(|a| a.maximum_bit_depth);
            let sr = rel.audio_info.as_ref().and_then(|a| a.maximum_sampling_rate);
            albums.push(AlbumCard {
                id: rel.id.clone(),
                title: rel.title.clone(),
                artist: rel
                    .artist
                    .as_ref()
                    .map(|a| a.name.display.clone())
                    .unwrap_or_else(|| page.name.display.clone()),
                artist_id: rel
                    .artist
                    .as_ref()
                    .map(|a| a.id.to_string())
                    .unwrap_or_default(),
                year,
                quality_tier: match bd {
                    Some(d) if d >= 24 => "hires",
                    Some(_) => "cd",
                    None => "",
                }
                .to_string(),
                quality_label: match (bd, sr) {
                    (Some(b), Some(r)) => format!("{}-bit / {} kHz", b, r),
                    _ => String::new(),
                },
                artwork_url: rel
                    .image
                    .as_ref()
                    .and_then(|img| img.best().cloned())
                    .unwrap_or_default(),
            });
            if albums.len() >= 6 {
                break;
            }
        }
    }

    Some(SpotlightData {
        artist_id: seed.id.to_string(),
        artist_name: page.name.display.clone(),
        category: page.artist_category.clone().unwrap_or_default(),
        image_url,
        has_top_tracks: page.top_tracks.as_ref().map(|t| !t.is_empty()).unwrap_or(false),
        albums,
    })
}

fn album_items(cards: &[AlbumCard]) -> Vec<AlbumCardItem> {
    cards
        .iter()
        .map(|c| AlbumCardItem {
            id: c.id.clone().into(),
            title: c.title.clone().into(),
            artist: c.artist.clone().into(),
            artist_id: c.artist_id.clone().into(),
            genre: "".into(),
            year: c.year.clone().into(),
            quality_tier: c.quality_tier.clone().into(),
            quality_label: c.quality_label.clone().into(),
            ribbon: "".into(),
            ribbon_kind: "".into(),
            artwork_url: c.artwork_url.clone().into(),
            artwork: slint::Image::default(),
        })
        .collect()
}

fn artist_items(artists: &[ArtistSlim]) -> Vec<SlimItem> {
    artists
        .iter()
        .map(|a| SlimItem {
            id: a.id.clone().into(),
            title: a.name.clone().into(),
            subtitle: "".into(),
            rank: "".into(),
            artwork_url: a.artwork_url.clone().into(),
            artwork: slint::Image::default(),
            following: a.following,
        })
        .collect()
}

fn section(title: &str, cards: &[AlbumCard]) -> DiscoverSection {
    DiscoverSection {
        title: title.into(),
        albums: ModelRc::new(VecModel::from(album_items(cards))),
    }
}

pub fn apply_for_you(window: &AppWindow, data: &ForYouData) {
    let state = window.global::<ForYouState>();
    state.set_release_watch(section("Release Watch", &data.release_watch));
    state.set_recent_albums(section("Recently Played Albums", &data.recent_albums));
    let tracks: Vec<SlimItem> = data
        .recent_tracks
        .iter()
        .map(|t| SlimItem {
            id: t.id.clone().into(),
            title: t.title.clone().into(),
            subtitle: t.subtitle.clone().into(),
            rank: "".into(),
            artwork_url: t.artwork_url.clone().into(),
            artwork: slint::Image::default(),
            following: false,
        })
        .collect();
    state.set_recent_tracks(ModelRc::new(VecModel::from(tracks)));
    state.set_top_artists(ModelRc::new(VecModel::from(artist_items(&data.top_artists))));
    state.set_artists_to_follow(ModelRc::new(VecModel::from(artist_items(
        &data.artists_to_follow,
    ))));
    state.set_more_from_library(section("More From Your Library", &data.more_from_library));
    state.set_rediscover(section("Rediscover Your Library", &data.rediscover));
    let radio: Vec<crate::RadioStationItem> = data
        .radio_stations
        .iter()
        .map(|r| crate::RadioStationItem {
            album_id: r.album_id.clone().into(),
            title: r.title.clone().into(),
            artist: r.artist.clone().into(),
            artwork_url: r.artwork_url.clone().into(),
            artwork: slint::Image::default(),
        })
        .collect();
    state.set_radio_stations(ModelRc::new(VecModel::from(radio)));

    if let Some(sp) = &data.spotlight {
        state.set_spotlight_visible(true);
        state.set_spotlight_artist_id(sp.artist_id.clone().into());
        state.set_spotlight_name(sp.artist_name.clone().into());
        state.set_spotlight_category(sp.category.clone().into());
        state.set_spotlight_image_url(sp.image_url.clone().into());
        state.set_spotlight_has_top_tracks(sp.has_top_tracks);
        state.set_spotlight_albums(ModelRc::new(VecModel::from(album_items(&sp.albums))));
    } else {
        state.set_spotlight_visible(false);
    }

    state.set_loading(false);
    state.set_loaded(true);
}

pub fn reset_loading(window: &AppWindow) {
    window.global::<ForYouState>().set_loading(true);
}

pub fn artwork_jobs(data: &ForYouData) -> Vec<ArtworkJob> {
    let mut jobs = Vec::new();
    for (i, c) in data.release_watch.iter().enumerate() {
        if !c.artwork_url.is_empty() {
            jobs.push(ArtworkJob {
                url: c.artwork_url.clone(),
                target: ArtworkTarget::ForYouReleaseWatch { index: i },
            });
        }
    }
    for (i, c) in data.recent_albums.iter().enumerate() {
        if !c.artwork_url.is_empty() {
            jobs.push(ArtworkJob {
                url: c.artwork_url.clone(),
                target: ArtworkTarget::ForYouRecentAlbum { index: i },
            });
        }
    }
    for (i, t) in data.recent_tracks.iter().enumerate() {
        if !t.artwork_url.is_empty() {
            jobs.push(ArtworkJob {
                url: t.artwork_url.clone(),
                target: ArtworkTarget::ForYouRecentTrack { index: i },
            });
        }
    }
    for (i, a) in data.top_artists.iter().enumerate() {
        if !a.artwork_url.is_empty() {
            jobs.push(ArtworkJob {
                url: a.artwork_url.clone(),
                target: ArtworkTarget::ForYouTopArtist { index: i },
            });
        }
    }
    for (i, a) in data.artists_to_follow.iter().enumerate() {
        if !a.artwork_url.is_empty() {
            jobs.push(ArtworkJob {
                url: a.artwork_url.clone(),
                target: ArtworkTarget::ForYouToFollow { index: i },
            });
        }
    }
    for (i, r) in data.radio_stations.iter().enumerate() {
        if !r.artwork_url.is_empty() {
            jobs.push(ArtworkJob {
                url: r.artwork_url.clone(),
                target: ArtworkTarget::ForYouRadioStation { index: i },
            });
        }
    }
    for (i, c) in data.more_from_library.iter().enumerate() {
        if !c.artwork_url.is_empty() {
            jobs.push(ArtworkJob {
                url: c.artwork_url.clone(),
                target: ArtworkTarget::ForYouMoreFromLibrary { index: i },
            });
        }
    }
    for (i, c) in data.rediscover.iter().enumerate() {
        if !c.artwork_url.is_empty() {
            jobs.push(ArtworkJob {
                url: c.artwork_url.clone(),
                target: ArtworkTarget::ForYouRediscover { index: i },
            });
        }
    }
    if let Some(sp) = &data.spotlight {
        if !sp.image_url.is_empty() {
            jobs.push(ArtworkJob {
                url: sp.image_url.clone(),
                target: ArtworkTarget::ForYouSpotlightArtist,
            });
        }
        for (i, c) in sp.albums.iter().enumerate() {
            if !c.artwork_url.is_empty() {
                jobs.push(ArtworkJob {
                    url: c.artwork_url.clone(),
                    target: ArtworkTarget::ForYouSpotlightAlbum { index: i },
                });
            }
        }
    }
    jobs
}
