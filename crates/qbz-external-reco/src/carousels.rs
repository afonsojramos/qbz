//! Per-row candidate generation, blending, filtering, Qobuz validation, rotation.
//!
//! The documented Last.fm "artist discovery" recipe (api-evangelist/lastfm
//! arazzo workflow): top artists -> artist.getSimilar -> top albums. There is no
//! recommendation endpoint, so recommendations are replicated from similarity.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use futures_util::stream::{self, StreamExt};
use qbz_models::Album;

use crate::cache::RecoCache;
use crate::matching::normalize;
use crate::types::{
    AlbumCandidate, AlbumReco, ArtistCandidate, ArtistReco, ExtHistory, RecoSource, TrackCandidate,
    TrackReco,
};
use crate::validate::{build_album_reco, validate_album, validate_artist, validate_track};
use crate::{RecoCatalog, RecoInputs};

const DISPLAY_CAP: usize = 20;
const PLAYLIST_CAP: usize = 30;
const VALIDATE_CONCURRENCY: usize = 6;
const ARTIST_SEEDS: usize = 6;
const SIMILAR_PER_SEED: u32 = 12;
const KNOWN_ARTISTS_PER_BUILD: usize = 8;

type Cache<'a> = Option<&'a Mutex<RecoCache>>;

fn track_key(artist: &str, title: &str) -> String {
    format!("{}|{}", normalize(artist), normalize(title))
}
fn album_key(artist: &str, album: &str) -> String {
    format!("{}|{}", normalize(artist), normalize(album))
}

fn rotate_take<T>(mut pool: Vec<T>, seed: u64, take: usize) -> Vec<T> {
    if pool.is_empty() {
        return pool;
    }
    let off = (seed as usize) % pool.len();
    pool.rotate_left(off);
    pool.truncate(take);
    pool
}

// ── Validation pools (concurrent, blend-ordered, deduped) ───────────────────

async fn validate_artist_pool(
    catalog: &dyn RecoCatalog,
    cache: Cache<'_>,
    cands: Vec<ArtistCandidate>,
) -> Vec<ArtistReco> {
    let resolved: Vec<Option<ArtistReco>> = stream::iter(
        cands.into_iter().map(|cand| async move { validate_artist(catalog, cache, &cand).await }),
    )
    .buffered(VALIDATE_CONCURRENCY)
    .collect()
    .await;
    let mut seen = HashSet::new();
    resolved
        .into_iter()
        .flatten()
        .filter(|r| seen.insert(r.qobuz_artist_id))
        .collect()
}

async fn validate_album_pool(
    catalog: &dyn RecoCatalog,
    cache: Cache<'_>,
    cands: Vec<AlbumCandidate>,
) -> Vec<AlbumReco> {
    let resolved: Vec<Option<AlbumReco>> = stream::iter(
        cands.into_iter().map(|cand| async move { validate_album(catalog, cache, &cand).await }),
    )
    .buffered(VALIDATE_CONCURRENCY)
    .collect()
    .await;
    let mut seen = HashSet::new();
    resolved
        .into_iter()
        .flatten()
        .filter(|r| seen.insert(r.qobuz_album_id.clone()))
        .collect()
}

async fn validate_track_pool(
    catalog: &dyn RecoCatalog,
    mb: &qbz_integrations::MusicBrainzClient,
    cache: Cache<'_>,
    cands: Vec<TrackCandidate>,
) -> Vec<TrackReco> {
    let resolved: Vec<Option<TrackReco>> = stream::iter(
        cands.into_iter().map(|cand| async move { validate_track(catalog, mb, cache, &cand).await }),
    )
    .buffered(VALIDATE_CONCURRENCY)
    .collect()
    .await;
    let mut seen = HashSet::new();
    resolved
        .into_iter()
        .flatten()
        .filter(|r| seen.insert(r.qobuz_track_id))
        .collect()
}

// ── Shared external history ─────────────────────────────────────────────────

pub async fn gather_history(inputs: &RecoInputs<'_>) -> ExtHistory {
    let mut artist_names = HashSet::new();
    let mut track_keys = HashSet::new();
    let mut album_keys = HashSet::new();

    if let Some(lf) = &inputs.lastfm {
        let (tops, recents, albums) = tokio::join!(
            lf.client.get_top_artists(&lf.username, "overall", 300),
            lf.client.get_recent_tracks(&lf.username, 200, 1),
            lf.client.get_user_top_albums(&lf.username, "overall", 300, 1),
        );
        for a in tops.unwrap_or_default() {
            artist_names.insert(normalize(&a.name));
        }
        for t in recents.unwrap_or_default() {
            artist_names.insert(normalize(&t.artist));
            track_keys.insert(track_key(&t.artist, &t.name));
        }
        for al in albums.unwrap_or_default() {
            artist_names.insert(normalize(&al.artist));
            album_keys.insert(album_key(&al.artist, &al.name));
        }
    }
    if let Some(lb) = &inputs.listenbrainz {
        let listens = lb.client.get_recent_listens(&lb.username, 1000).await.unwrap_or_default();
        for l in listens {
            artist_names.insert(normalize(&l.artist_name));
            track_keys.insert(track_key(&l.artist_name, &l.track_name));
        }
    }
    ExtHistory {
        artist_names,
        track_keys,
        album_keys,
    }
}

// ── Recommended Artists (Last.fm similar of recent top, not heard) ──────────

pub async fn build_rec_artists(inputs: &RecoInputs<'_>, history: &ExtHistory) -> Vec<ArtistReco> {
    let Some(lf) = &inputs.lastfm else {
        return Vec::new();
    };
    let seeds: Vec<String> = lf
        .client
        .get_top_artists(&lf.username, "1month", 12)
        .await
        .unwrap_or_default()
        .into_iter()
        .take(ARTIST_SEEDS)
        .map(|a| a.name)
        .collect();
    let seeds_norm: HashSet<String> = seeds.iter().map(|s| normalize(s)).collect();

    let sim_results: Vec<(String, Vec<qbz_integrations::lastfm::LastFmSimilarArtist>)> =
        stream::iter(seeds.into_iter().map(|seed| {
            let lf = lf;
            async move {
                let sims = lf
                    .client
                    .get_similar_artists(&seed, SIMILAR_PER_SEED)
                    .await
                    .unwrap_or_default();
                (seed, sims)
            }
        }))
        .buffered(4)
        .collect()
        .await;

    // Aggregate candidate -> the seeds that surfaced it (for the subtitle).
    let mut agg: HashMap<String, (String, Vec<String>, f32)> = HashMap::new();
    for (seed, sims) in sim_results {
        for s in sims {
            let nk = normalize(&s.name);
            if nk.is_empty() || history.artist_names.contains(&nk) || seeds_norm.contains(&nk) {
                continue;
            }
            let e = agg.entry(nk).or_insert((s.name.clone(), Vec::new(), 0.0));
            if !e.1.contains(&seed) {
                e.1.push(seed.clone());
            }
            e.2 = e.2.max(s.match_score as f32);
        }
    }

    let mut candidates: Vec<ArtistCandidate> = agg
        .into_values()
        .map(|(name, seeds, score)| {
            let subtitle = if seeds.is_empty() {
                String::new()
            } else {
                format!(
                    "Similar to {}",
                    seeds.iter().take(2).cloned().collect::<Vec<_>>().join(", ")
                )
            };
            ArtistCandidate {
                name,
                source: RecoSource::LastFm,
                score,
                subtitle,
            }
        })
        .collect();
    candidates.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    candidates.truncate(45);

    let pool = validate_artist_pool(inputs.catalog, inputs.cache, candidates).await;
    rotate_take(pool, inputs.rotation_seed, DISPLAY_CAP)
}

// ── Recommended Albums (Last.fm: your artists' top albums, not scrobbled) ───

pub async fn build_rec_albums(inputs: &RecoInputs<'_>, history: &ExtHistory) -> Vec<AlbumReco> {
    let Some(lf) = &inputs.lastfm else {
        return Vec::new();
    };
    let artists: Vec<String> = lf
        .client
        .get_top_artists(&lf.username, "1month", 10)
        .await
        .unwrap_or_default()
        .into_iter()
        .take(KNOWN_ARTISTS_PER_BUILD)
        .map(|a| a.name)
        .collect();

    let per_artist: Vec<(String, Vec<qbz_integrations::lastfm::LastFmAlbum>)> =
        stream::iter(artists.into_iter().map(|name| {
            let lf = lf;
            async move {
                let albums = lf.client.get_artist_top_albums(&name, 6).await.unwrap_or_default();
                (name, albums)
            }
        }))
        .buffered(4)
        .collect()
        .await;

    let mut seen: HashSet<String> = HashSet::new();
    let mut candidates: Vec<AlbumCandidate> = Vec::new();
    for (artist, albums) in per_artist {
        for al in albums {
            let k = album_key(&al.artist, &al.name);
            if history.album_keys.contains(&k) || !seen.insert(k) {
                continue;
            }
            candidates.push(AlbumCandidate {
                artist: al.artist.clone(),
                title: al.name,
                upc: None,
                source: RecoSource::LastFm,
                score: al.playcount as f32,
                subtitle: format!("From {} — you haven't heard this one", artist),
            });
        }
    }
    candidates.truncate(50);
    let pool = validate_album_pool(inputs.catalog, inputs.cache, candidates).await;
    rotate_take(pool, inputs.rotation_seed, DISPLAY_CAP)
}

// ── Fresh Releases (ListenBrainz, from artists you follow) ──────────────────

pub async fn build_fresh_releases(inputs: &RecoInputs<'_>) -> Vec<AlbumReco> {
    let Some(lb) = &inputs.listenbrainz else {
        return Vec::new();
    };
    let releases = lb.client.get_fresh_releases(&lb.username, 30).await.unwrap_or_default();
    let candidates: Vec<AlbumCandidate> = releases
        .into_iter()
        .filter(|r| !r.release_name.is_empty() && !r.artist_credit_name.is_empty())
        .take(50)
        .map(|r| AlbumCandidate {
            artist: r.artist_credit_name,
            title: r.release_name,
            upc: None,
            source: RecoSource::ListenBrainz,
            score: 0.0,
            subtitle: r
                .release_date
                .map(|d| format!("New release · {}", d))
                .unwrap_or_else(|| "New release".to_string()),
        })
        .collect();
    let pool = validate_album_pool(inputs.catalog, inputs.cache, candidates).await;
    rotate_take(pool, inputs.rotation_seed, DISPLAY_CAP)
}

// ── Weekly playlists (ListenBrainz curated: exploration / jams) ─────────────

pub async fn build_weekly(inputs: &RecoInputs<'_>, source_patch: &str) -> Vec<TrackReco> {
    let Some(lb) = &inputs.listenbrainz else {
        return Vec::new();
    };
    let playlists = lb
        .client
        .get_created_for_playlists(&lb.username, 50)
        .await
        .unwrap_or_default();
    // Newest playlist matching the source_patch (created_at desc).
    let chosen = playlists
        .into_iter()
        .filter(|p| p.source_patch.as_deref() == Some(source_patch))
        .max_by(|a, b| a.created_at.cmp(&b.created_at));
    let Some(meta) = chosen else {
        return Vec::new();
    };
    let tracks = lb
        .client
        .get_playlist_tracks(&meta.playlist_mbid)
        .await
        .unwrap_or_default();
    let candidates: Vec<TrackCandidate> = tracks
        .into_iter()
        .filter(|t| !t.title.is_empty() && !t.artist_name.is_empty())
        .map(|t| TrackCandidate {
            artist: t.artist_name,
            title: t.title,
            album: t.release_name,
            duration_ms: None,
            isrc: None,
            recording_mbid: t.recording_mbid,
            source: RecoSource::ListenBrainz,
            score: 0.0,
        })
        .collect();
    let pool =
        validate_track_pool(inputs.catalog, inputs.musicbrainz, inputs.cache, candidates).await;
    pool.into_iter().take(PLAYLIST_CAP).collect()
}

// ── Deep-cut albums from artists you know (Qobuz catalog, not heard) ────────

pub async fn build_deep_cut_albums(inputs: &RecoInputs<'_>) -> Vec<AlbumReco> {
    if inputs.local.known_artist_ids.is_empty() {
        return Vec::new();
    }
    let mut ids: Vec<u64> = inputs.local.known_artist_ids.iter().copied().collect();
    ids.sort_unstable();
    let ids = rotate_take(ids, inputs.rotation_seed, KNOWN_ARTISTS_PER_BUILD);

    let per_artist: Vec<Vec<Album>> = stream::iter(ids.into_iter().map(|id| {
        let catalog = inputs.catalog;
        async move { catalog.artist_albums(id, 12).await }
    }))
    .buffered(4)
    .collect()
    .await;

    let mut seen: HashSet<String> = HashSet::new();
    let mut pool: Vec<AlbumReco> = Vec::new();
    for albums in per_artist {
        for album in albums.into_iter().skip(2) {
            if album.id.is_empty()
                || inputs.local.played_album_ids.contains(&album.id)
                || !seen.insert(album.id.clone())
            {
                continue;
            }
            let subtitle = format!("Deep cut · {}", album.artist.name);
            pool.push(build_album_reco(&album, subtitle, RecoSource::Internal));
        }
    }
    rotate_take(pool, inputs.rotation_seed, DISPLAY_CAP)
}

// ── Cold-start editorial (top albums + artists) ─────────────────────────────

pub async fn build_editorial(inputs: &RecoInputs<'_>) -> (Vec<AlbumReco>, Vec<ArtistReco>) {
    let (most_streamed, new_releases) = tokio::join!(
        inputs.catalog.featured_albums("most-streamed", 20),
        inputs.catalog.featured_albums("new-releases", 20),
    );

    let mut seen_albums: HashSet<String> = HashSet::new();
    let mut top_albums: Vec<AlbumReco> = Vec::new();
    for album in most_streamed.iter().chain(new_releases.iter()) {
        if !album.id.is_empty() && seen_albums.insert(album.id.clone()) {
            top_albums.push(build_album_reco(album, String::new(), RecoSource::Editorial));
        }
    }
    top_albums.truncate(20);

    let mut seen_artists: HashSet<u64> = HashSet::new();
    let mut artist_ids: Vec<(u64, String)> = Vec::new();
    for album in most_streamed.iter().chain(new_releases.iter()) {
        let id = album.artist.id;
        if id != 0 && seen_artists.insert(id) {
            artist_ids.push((id, album.artist.name.clone()));
        }
        if artist_ids.len() >= 12 {
            break;
        }
    }
    let top_artists: Vec<ArtistReco> = stream::iter(artist_ids.into_iter().map(|(id, name)| {
        let catalog = inputs.catalog;
        async move {
            let image_url = catalog
                .get_artist(id)
                .await
                .and_then(|a| a.image.and_then(|i| i.best().cloned()))
                .unwrap_or_default();
            ArtistReco {
                qobuz_artist_id: id,
                name,
                image_url,
                subtitle: String::new(),
                source: RecoSource::Editorial,
            }
        }
    }))
    .buffered(4)
    .collect()
    .await;

    (top_albums, top_artists)
}
