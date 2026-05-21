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
}

#[derive(Clone)]
pub struct AlbumCard {
    pub id: String,
    pub title: String,
    pub artist: String,
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

    ForYouData {
        release_watch,
        recent_albums,
        recent_tracks,
        top_artists,
        artists_to_follow: to_follow,
    }
}

fn album_items(cards: &[AlbumCard]) -> Vec<AlbumCardItem> {
    cards
        .iter()
        .map(|c| AlbumCardItem {
            id: c.id.clone().into(),
            title: c.title.clone().into(),
            artist: c.artist.clone().into(),
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
    jobs
}
