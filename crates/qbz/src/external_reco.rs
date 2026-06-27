//! Discover > Recommendations (the 4th tab) controller.
//!
//! Wires the `qbz-external-reco` engine to Slint: a RecoCatalog over QbzCore,
//! the per-user resolution-cache lifecycle, the scrobbler-username gate, and a
//! PROGRESSIVE apply — each row paints the moment its builder resolves (the For
//! You branch pattern), so the tab fills in incrementally instead of all at once.
//!
//! Lineup: Recommended Artists + Recommended Albums (Last.fm), Fresh Releases +
//! Weekly Exploration/Jams (ListenBrainz), Deep-cut albums, and a Qobuz editorial
//! cold-start fallback.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use qbz_app::shell::AppRuntime;
use qbz_external_reco::{
    build_deep_cut_albums, build_editorial, build_fresh_releases, build_rec_albums,
    build_rec_artists, build_weekly_exploration, build_weekly_jams, gather_history, is_cold_start,
    AlbumReco, ArtistReco, LastFmHandle, ListenBrainzHandle, LocalHistory, RecoCache, RecoCatalog,
    RecoInputs, TrackReco,
};
use qbz_integrations::{LastFmClient, ListenBrainzClient, MusicBrainzClient};
use qbz_models::{Album, Artist, Track};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::adapter::SlintAdapter;
use crate::artwork::{ArtworkJob, ArtworkTarget, ImageCache};
use crate::{AlbumCardItem, AppWindow, DiscoverSection, ExternalRecoState, SlimItem};

static CACHE_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn init_for_user(base_dir: &Path) {
    if let Ok(mut g) = CACHE_DIR.lock() {
        *g = Some(base_dir.to_path_buf());
    }
    if let Ok(cache) = RecoCache::open_at(base_dir) {
        let _ = cache.cleanup_expired();
    }
}

#[allow(dead_code)]
pub fn teardown() {
    if let Ok(mut g) = CACHE_DIR.lock() {
        *g = None;
    }
}

// ── RecoCatalog over QbzCore (errors -> empty) ──────────────────────────────

struct CoreRecoCatalog {
    runtime: Arc<AppRuntime<SlintAdapter>>,
}

#[async_trait]
impl RecoCatalog for CoreRecoCatalog {
    async fn search_tracks(&self, query: &str, limit: usize) -> Vec<Track> {
        self.runtime
            .core()
            .search_tracks(query, limit as u32, 0, None)
            .await
            .map(|p| p.items)
            .unwrap_or_default()
    }
    async fn search_artists(&self, query: &str, limit: usize) -> Vec<Artist> {
        self.runtime
            .core()
            .search_artists(query, limit as u32, 0, None)
            .await
            .map(|p| p.items)
            .unwrap_or_default()
    }
    async fn search_albums(&self, query: &str, limit: usize) -> Vec<Album> {
        self.runtime
            .core()
            .search_albums(query, limit as u32, 0, None)
            .await
            .map(|p| p.items)
            .unwrap_or_default()
    }
    async fn artist_top_tracks(&self, artist_id: u64, limit: usize) -> Vec<Track> {
        self.runtime
            .core()
            .get_artist_tracks(artist_id, limit as u32, 0)
            .await
            .map(|c| c.items)
            .unwrap_or_default()
    }
    async fn artist_albums(&self, artist_id: u64, limit: usize) -> Vec<Album> {
        self.runtime
            .core()
            .get_artist_albums(artist_id, Some(limit as u32), Some(0))
            .await
            .map(|a| a.items)
            .unwrap_or_default()
    }
    async fn featured_albums(&self, kind: &str, limit: usize) -> Vec<Album> {
        self.runtime
            .core()
            .get_featured_albums(kind, limit as u32, 0, None)
            .await
            .map(|p| p.items)
            .unwrap_or_default()
    }
    async fn get_artist(&self, artist_id: u64) -> Option<Artist> {
        self.runtime.core().get_artist(artist_id).await.ok()
    }
}

// ── Loader ──────────────────────────────────────────────────────────────────

pub fn ensure_loaded(
    runtime: &Arc<AppRuntime<SlintAdapter>>,
    weak: &slint::Weak<AppWindow>,
    handle: &tokio::runtime::Handle,
    image_cache: &ImageCache,
) {
    let Some(w) = weak.upgrade() else {
        return;
    };
    if w.global::<ExternalRecoState>().get_loaded() {
        return;
    }
    w.global::<ExternalRecoState>().set_loading(true);
    spawn(runtime.clone(), weak.clone(), handle, image_cache.clone());
}

fn rotation_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

fn spawn(
    runtime: Arc<AppRuntime<SlintAdapter>>,
    weak: slint::Weak<AppWindow>,
    handle: &tokio::runtime::Handle,
    image_cache: ImageCache,
) {
    handle.spawn(async move {
        let cfg = crate::scrobbler_settings::get();

        let lastfm_client = LastFmClient::new();
        let lb_client = ListenBrainzClient::new();
        if cfg.listenbrainz_is_authed() {
            lb_client
                .restore_token(cfg.listenbrainz_token.clone(), cfg.listenbrainz_username.clone())
                .await;
        }
        let mb_client = MusicBrainzClient::new();

        let lastfm = if cfg.lastfm_is_authed() && !cfg.lastfm_username.is_empty() {
            Some(LastFmHandle {
                username: cfg.lastfm_username.clone(),
                client: &lastfm_client,
            })
        } else {
            None
        };
        let listenbrainz = if cfg.listenbrainz_is_authed() && !cfg.listenbrainz_username.is_empty() {
            Some(ListenBrainzHandle {
                username: cfg.listenbrainz_username.clone(),
                client: &lb_client,
            })
        } else {
            None
        };

        let local = LocalHistory {
            known_artist_ids: crate::reco::known_artist_ids(2).unwrap_or_default(),
            ..Default::default()
        };

        let catalog = CoreRecoCatalog {
            runtime: runtime.clone(),
        };
        let cache = CACHE_DIR
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .and_then(|dir| RecoCache::open_at(&dir).ok())
            .map(Mutex::new);

        let inputs = RecoInputs {
            lastfm,
            listenbrainz,
            musicbrainz: &mb_client,
            catalog: &catalog,
            cache: cache.as_ref(),
            local,
            rotation_seed: rotation_seed(),
        };

        if is_cold_start(&inputs) {
            let (albums, artists) = build_editorial(&inputs).await;
            apply_albums(&weak, &image_cache, albums, AlbumRow::TopAlbums);
            apply_artists(&weak, &image_cache, artists, ArtistRow::TopArtists);
        } else {
            let history = gather_history(&inputs).await;
            // Progressive: each branch paints its row the moment it resolves.
            let b_artists = async {
                let r = build_rec_artists(&inputs, &history).await;
                apply_artists(&weak, &image_cache, r, ArtistRow::RecArtists);
            };
            let b_albums = async {
                let r = build_rec_albums(&inputs, &history).await;
                apply_albums(&weak, &image_cache, r, AlbumRow::RecAlbums);
            };
            let b_fresh = async {
                let r = build_fresh_releases(&inputs).await;
                apply_albums(&weak, &image_cache, r, AlbumRow::FreshReleases);
            };
            let b_explore = async {
                let r = build_weekly_exploration(&inputs).await;
                apply_tracks(&weak, &image_cache, r, TrackRow::WeeklyExploration);
            };
            let b_jams = async {
                let r = build_weekly_jams(&inputs).await;
                apply_tracks(&weak, &image_cache, r, TrackRow::WeeklyJams);
            };
            let b_deep = async {
                let r = build_deep_cut_albums(&inputs).await;
                apply_albums(&weak, &image_cache, r, AlbumRow::DeepCuts);
            };
            tokio::join!(b_artists, b_albums, b_fresh, b_explore, b_jams, b_deep);
        }

        let _ = weak.upgrade_in_event_loop(|w| {
            let s = w.global::<ExternalRecoState>();
            s.set_loading(false);
            s.set_loaded(true);
        });
    });
}

// ── Per-row apply (models built on the UI thread; slint::Image is !Send) ────

#[derive(Clone, Copy)]
enum ArtistRow {
    RecArtists,
    TopArtists,
}
#[derive(Clone, Copy)]
enum AlbumRow {
    RecAlbums,
    FreshReleases,
    DeepCuts,
    TopAlbums,
}
#[derive(Clone, Copy)]
enum TrackRow {
    WeeklyExploration,
    WeeklyJams,
}

fn slim_from_artist(a: &ArtistReco) -> SlimItem {
    SlimItem {
        id: a.qobuz_artist_id.to_string().into(),
        title: a.name.clone().into(),
        subtitle: a.subtitle.clone().into(),
        rank: "".into(),
        artwork_url: a.image_url.clone().into(),
        artwork: slint::Image::default(),
        following: false,
    }
}
fn slim_from_track(t: &TrackReco) -> SlimItem {
    SlimItem {
        id: t.qobuz_track_id.to_string().into(),
        title: t.title.clone().into(),
        subtitle: t.artist.clone().into(),
        rank: "".into(),
        artwork_url: t.artwork_url.clone().into(),
        artwork: slint::Image::default(),
        following: false,
    }
}
fn album_card(a: &AlbumReco) -> AlbumCardItem {
    AlbumCardItem {
        id: a.qobuz_album_id.clone().into(),
        title: a.title.clone().into(),
        artist: a.artist.clone().into(),
        artist_id: a.artist_id.clone().into(),
        genre: "".into(),
        year: a.year.clone().into(),
        quality_tier: a.quality_tier.clone().into(),
        quality_label: a.quality_label.clone().into(),
        ribbon: "".into(),
        ribbon_kind: "".into(),
        artwork_url: a.artwork_url.clone().into(),
        artwork: slint::Image::default(),
        ..Default::default()
    }
}

fn apply_artists(
    weak: &slint::Weak<AppWindow>,
    cache: &ImageCache,
    rows: Vec<ArtistReco>,
    which: ArtistRow,
) {
    let jobs: Vec<ArtworkJob> = rows
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.image_url.is_empty())
        .map(|(i, a)| ArtworkJob {
            url: a.image_url.clone(),
            target: match which {
                ArtistRow::RecArtists => ArtworkTarget::ExtRecoRecArtist { index: i },
                ArtistRow::TopArtists => ArtworkTarget::ExtRecoTopArtist { index: i },
            },
        })
        .collect();
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |w| {
        let model = ModelRc::new(VecModel::from(
            rows.iter().map(slim_from_artist).collect::<Vec<_>>(),
        ));
        let s = w.global::<ExternalRecoState>();
        match which {
            ArtistRow::RecArtists => s.set_rec_artists(model),
            ArtistRow::TopArtists => s.set_top_artists(model),
        }
    });
    crate::artwork::spawn_loads(jobs, weak.clone(), cache.clone());
}

fn apply_tracks(
    weak: &slint::Weak<AppWindow>,
    cache: &ImageCache,
    rows: Vec<TrackReco>,
    which: TrackRow,
) {
    let jobs: Vec<ArtworkJob> = rows
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.artwork_url.is_empty())
        .map(|(i, t)| ArtworkJob {
            url: t.artwork_url.clone(),
            target: match which {
                TrackRow::WeeklyExploration => ArtworkTarget::ExtRecoWeeklyExploration { index: i },
                TrackRow::WeeklyJams => ArtworkTarget::ExtRecoWeeklyJams { index: i },
            },
        })
        .collect();
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |w| {
        let model = ModelRc::new(VecModel::from(
            rows.iter().map(slim_from_track).collect::<Vec<_>>(),
        ));
        let s = w.global::<ExternalRecoState>();
        match which {
            TrackRow::WeeklyExploration => s.set_weekly_exploration(model),
            TrackRow::WeeklyJams => s.set_weekly_jams(model),
        }
    });
    crate::artwork::spawn_loads(jobs, weak.clone(), cache.clone());
}

fn album_row_title(which: AlbumRow) -> String {
    match which {
        AlbumRow::RecAlbums => qbz_i18n::t("Recommended Albums"),
        AlbumRow::FreshReleases => qbz_i18n::t("Fresh Releases"),
        AlbumRow::DeepCuts => qbz_i18n::t("Deep cuts from artists you know"),
        AlbumRow::TopAlbums => qbz_i18n::t("Top albums on Qobuz"),
    }
}

fn apply_albums(
    weak: &slint::Weak<AppWindow>,
    cache: &ImageCache,
    rows: Vec<AlbumReco>,
    which: AlbumRow,
) {
    let jobs: Vec<ArtworkJob> = rows
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.artwork_url.is_empty())
        .map(|(i, a)| ArtworkJob {
            url: a.artwork_url.clone(),
            target: match which {
                AlbumRow::RecAlbums => ArtworkTarget::ExtRecoRecAlbum { index: i },
                AlbumRow::FreshReleases => ArtworkTarget::ExtRecoFreshAlbum { index: i },
                AlbumRow::DeepCuts => ArtworkTarget::ExtRecoDeepAlbum { index: i },
                AlbumRow::TopAlbums => ArtworkTarget::ExtRecoTopAlbum { index: i },
            },
        })
        .collect();
    let title = album_row_title(which);
    let w = weak.clone();
    let _ = w.upgrade_in_event_loop(move |w| {
        let section = DiscoverSection {
            title: title.into(),
            endpoint: "".into(),
            albums: ModelRc::new(VecModel::from(
                rows.iter().map(album_card).collect::<Vec<_>>(),
            )),
        };
        let s = w.global::<ExternalRecoState>();
        match which {
            AlbumRow::RecAlbums => s.set_rec_albums(section),
            AlbumRow::FreshReleases => s.set_fresh_releases(section),
            AlbumRow::DeepCuts => s.set_deep_cut_albums(section),
            AlbumRow::TopAlbums => s.set_top_albums(section),
        }
    });
    crate::artwork::spawn_loads(jobs, weak.clone(), cache.clone());
}
