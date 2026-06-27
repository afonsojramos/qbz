//! Per-carousel candidate generation, blending, filtering, validation, rotation.

use std::collections::HashSet;
use std::sync::Mutex;

use futures_util::stream::{self, StreamExt};
use qbz_integrations::MusicBrainzClient;
use qbz_models::Album;

use crate::cache::RecoCache;
use crate::matching::normalize;
use crate::types::{
    AlbumReco, ArtistCandidate, ArtistReco, ExternalCarousels, RecoSource, TrackCandidate, TrackReco,
};
use crate::validate::{validate_artist, validate_track};
use crate::{RecoCatalog, RecoInputs};

const DISPLAY_CAP: usize = 20;
const VALIDATE_CONCURRENCY: usize = 6;
const ARTIST_SEEDS: usize = 5;
const SIMILAR_PER_SEED: u32 = 15;
const KNOWN_ARTISTS_PER_BUILD: usize = 8;
const DEEP_CUT_SKIP_TOP: usize = 3;

type Cache<'a> = Option<&'a Mutex<RecoCache>>;

/// Normalized "artist|title" key for the listened/scrobbled exclusion sets.
fn track_key(artist: &str, title: &str) -> String {
    format!("{}|{}", normalize(artist), normalize(title))
}

/// External listening signal (normalized) used to filter "not heard" rows.
struct ExtHistory {
    /// Artists the user has listened to (Last.fm/LB) — excludes C1.
    artist_names: HashSet<String>,
    /// Tracks the user has scrobbled — excludes C2 / C4.
    track_keys: HashSet<String>,
}

/// Interleave candidate groups round-robin so every connected source is fairly
/// represented (the Option-B blend).
fn round_robin<T>(groups: Vec<Vec<T>>) -> Vec<T> {
    let mut iters: Vec<std::vec::IntoIter<T>> = groups.into_iter().map(|g| g.into_iter()).collect();
    let mut out = Vec::new();
    loop {
        let mut any = false;
        for it in iters.iter_mut() {
            if let Some(x) = it.next() {
                out.push(x);
                any = true;
            }
        }
        if !any {
            break;
        }
    }
    out
}

/// Rotate the pool by the daily seed, then take the display cap — gives "change"
/// across days without re-hitting the APIs (the pool is cache-backed).
fn rotate_take<T>(mut pool: Vec<T>, seed: u64, take: usize) -> Vec<T> {
    if pool.is_empty() {
        return pool;
    }
    let off = (seed as usize) % pool.len();
    pool.rotate_left(off);
    pool.truncate(take);
    pool
}

fn map_album_reco(a: &Album) -> AlbumReco {
    let year = a
        .release_date_original
        .as_deref()
        .and_then(|s| s.get(..4).map(|y| y.to_string()))
        .unwrap_or_default();
    let quality_tier = match a.maximum_bit_depth {
        Some(d) if d >= 24 => "hires",
        Some(_) => "cd",
        None => "",
    }
    .to_string();
    let quality_label = match (a.maximum_bit_depth, a.maximum_sampling_rate) {
        (Some(bd), Some(sr)) => format!("{}-bit / {} kHz", bd, sr),
        _ => String::new(),
    };
    AlbumReco {
        qobuz_album_id: a.id.clone(),
        title: a.title.clone(),
        artist: a.artist.name.clone(),
        artist_id: a.artist.id.to_string(),
        year,
        quality_tier,
        quality_label,
        artwork_url: a.image.best().cloned().unwrap_or_default(),
    }
}

/// Validate a track-candidate pool concurrently, preserving blend order, deduped
/// by Qobuz track id.
async fn validate_track_pool(
    catalog: &dyn RecoCatalog,
    mb: &MusicBrainzClient,
    cache: Cache<'_>,
    cands: Vec<TrackCandidate>,
) -> Vec<TrackReco> {
    let resolved: Vec<Option<TrackReco>> = stream::iter(
        cands
            .into_iter()
            .map(|cand| async move { validate_track(catalog, mb, cache, &cand).await }),
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

/// Validate an artist-candidate pool concurrently, deduped by Qobuz artist id.
async fn validate_artist_pool(
    catalog: &dyn RecoCatalog,
    cache: Cache<'_>,
    cands: Vec<ArtistCandidate>,
) -> Vec<ArtistReco> {
    let resolved: Vec<Option<ArtistReco>> = stream::iter(
        cands
            .into_iter()
            .map(|cand| async move { validate_artist(catalog, cache, &cand).await }),
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

pub async fn build(inputs: RecoInputs<'_>) -> ExternalCarousels {
    let has_external = inputs.lastfm.is_some() || inputs.listenbrainz.is_some();
    if !has_external {
        return build_editorial_fallback(&inputs).await;
    }

    let history = gather_external_history(&inputs).await;

    // Shared ListenBrainz CF (the personalized recommender), kept to the
    // never-listened slice — feeds BOTH C1 (its artists) and C2 (its tracks).
    let lb_cf_unheard = fetch_lb_cf_unheard(&inputs).await;

    let (c1, c2, c3, c4) = tokio::join!(
        build_similar_artists(&inputs, &history, &lb_cf_unheard),
        build_similar_tracks(&inputs, &history, &lb_cf_unheard),
        build_rediscover_tracks(&inputs, &history),
        build_deep_cut_tracks(&inputs, &history),
    );

    ExternalCarousels {
        editorial_fallback: false,
        similar_artists: c1,
        similar_tracks: c2,
        rediscover_tracks: c3,
        deep_cut_tracks: c4,
        top_albums: Vec::new(),
        top_artists: Vec::new(),
    }
}

async fn gather_external_history(inputs: &RecoInputs<'_>) -> ExtHistory {
    let mut artist_names = HashSet::new();
    let mut track_keys = HashSet::new();

    if let Some(lf) = &inputs.lastfm {
        let (tops, recents, loved) = tokio::join!(
            lf.client.get_top_artists(&lf.username, "overall", 200),
            lf.client.get_recent_tracks(&lf.username, 200, 1),
            lf.client.get_loved_tracks(&lf.username, 100),
        );
        for a in tops.unwrap_or_default() {
            artist_names.insert(normalize(&a.name));
        }
        for t in recents.unwrap_or_default() {
            artist_names.insert(normalize(&t.artist));
            track_keys.insert(track_key(&t.artist, &t.name));
        }
        for t in loved.unwrap_or_default() {
            artist_names.insert(normalize(&t.artist));
            track_keys.insert(track_key(&t.artist, &t.name));
        }
    }

    if let Some(lb) = &inputs.listenbrainz {
        let listens = lb
            .client
            .get_recent_listens(&lb.username, 1000)
            .await
            .unwrap_or_default();
        for l in listens {
            artist_names.insert(normalize(&l.artist_name));
            track_keys.insert(track_key(&l.artist_name, &l.track_name));
        }
    }

    ExtHistory {
        artist_names,
        track_keys,
    }
}

/// ListenBrainz CF recommendations, never-listened slice, hydrated to names.
async fn fetch_lb_cf_unheard(
    inputs: &RecoInputs<'_>,
) -> Vec<qbz_integrations::listenbrainz::LbRecordingMeta> {
    let Some(lb) = &inputs.listenbrainz else {
        return Vec::new();
    };
    let recs = lb
        .client
        .get_cf_recommendations(&lb.username, 200)
        .await
        .unwrap_or_default();
    let unheard_mbids: Vec<String> = recs
        .into_iter()
        .filter(|r| r.latest_listened_at.is_none())
        .map(|r| r.recording_mbid)
        .filter(|m| !m.is_empty())
        .take(80)
        .collect();
    if unheard_mbids.is_empty() {
        return Vec::new();
    }
    lb.client
        .get_metadata_recordings(&unheard_mbids)
        .await
        .unwrap_or_default()
}

// ── C1 — Similar artists you haven't heard ─────────────────────────────────
async fn build_similar_artists(
    inputs: &RecoInputs<'_>,
    history: &ExtHistory,
    lb_cf_unheard: &[qbz_integrations::listenbrainz::LbRecordingMeta],
) -> Vec<ArtistReco> {
    let mut lastfm_group: Vec<ArtistCandidate> = Vec::new();
    if let Some(lf) = &inputs.lastfm {
        let seeds = lf
            .client
            .get_top_artists(&lf.username, "6month", 10)
            .await
            .unwrap_or_default();
        let seed_names: Vec<String> = seeds.into_iter().take(ARTIST_SEEDS).map(|a| a.name).collect();
        let sims = stream::iter(seed_names.into_iter().map(|name| {
            let lf = lf;
            async move {
                lf.client
                    .get_similar_artists(&name, SIMILAR_PER_SEED)
                    .await
                    .unwrap_or_default()
            }
        }))
        .buffered(4)
        .collect::<Vec<_>>()
        .await;
        for group in sims {
            for s in group {
                lastfm_group.push(ArtistCandidate {
                    name: s.name,
                    source: RecoSource::LastFm,
                    score: s.match_score as f32,
                });
            }
        }
    }

    let mut lb_group: Vec<ArtistCandidate> = Vec::new();
    let total = lb_cf_unheard.len().max(1) as f32;
    for (i, meta) in lb_cf_unheard.iter().enumerate() {
        if meta.artist_name.is_empty() {
            continue;
        }
        lb_group.push(ArtistCandidate {
            name: meta.artist_name.clone(),
            source: RecoSource::ListenBrainz,
            score: 1.0 - (i as f32 / total),
        });
    }

    // Blend, drop already-heard, dedup by normalized name.
    let mut seen: HashSet<String> = HashSet::new();
    let candidates: Vec<ArtistCandidate> = round_robin(vec![lb_group, lastfm_group])
        .into_iter()
        .filter(|c| {
            let n = normalize(&c.name);
            !n.is_empty() && !history.artist_names.contains(&n) && seen.insert(n)
        })
        .take(45)
        .collect();

    let pool = validate_artist_pool(inputs.catalog, inputs.cache, candidates).await;
    rotate_take(pool, inputs.rotation_seed, DISPLAY_CAP)
}

// ── C2 — Similar tracks you haven't heard ──────────────────────────────────
async fn build_similar_tracks(
    inputs: &RecoInputs<'_>,
    history: &ExtHistory,
    lb_cf_unheard: &[qbz_integrations::listenbrainz::LbRecordingMeta],
) -> Vec<TrackReco> {
    // LB: CF recordings (already never-listened).
    let total = lb_cf_unheard.len().max(1) as f32;
    let lb_group: Vec<TrackCandidate> = lb_cf_unheard
        .iter()
        .enumerate()
        .filter(|(_, m)| !m.recording_name.is_empty() && !m.artist_name.is_empty())
        .map(|(i, m)| TrackCandidate {
            artist: m.artist_name.clone(),
            title: m.recording_name.clone(),
            album: m.release_name.clone(),
            duration_ms: None,
            isrc: None,
            recording_mbid: Some(m.recording_mbid.clone()).filter(|s| !s.is_empty()),
            source: RecoSource::ListenBrainz,
            score: 1.0 - (i as f32 / total),
        })
        .collect();

    // Last.fm: track.getSimilar seeded by loved tracks.
    let mut lastfm_group: Vec<TrackCandidate> = Vec::new();
    if let Some(lf) = &inputs.lastfm {
        let loved = lf
            .client
            .get_loved_tracks(&lf.username, 30)
            .await
            .unwrap_or_default();
        let seeds: Vec<(String, String)> = loved
            .into_iter()
            .take(8)
            .map(|t| (t.artist, t.name))
            .collect();
        let sims = stream::iter(seeds.into_iter().map(|(artist, title)| {
            let lf = lf;
            async move {
                lf.client
                    .get_similar_tracks(&artist, &title, 10)
                    .await
                    .unwrap_or_default()
            }
        }))
        .buffered(4)
        .collect::<Vec<_>>()
        .await;
        for group in sims {
            for s in group {
                lastfm_group.push(TrackCandidate {
                    artist: s.artist,
                    title: s.name,
                    album: None,
                    duration_ms: None,
                    isrc: None,
                    recording_mbid: None,
                    source: RecoSource::LastFm,
                    score: s.match_score as f32,
                });
            }
        }
    }

    let mut seen: HashSet<String> = HashSet::new();
    let candidates: Vec<TrackCandidate> = round_robin(vec![lb_group, lastfm_group])
        .into_iter()
        .filter(|c| {
            let k = track_key(&c.artist, &c.title);
            !history.track_keys.contains(&k) && seen.insert(k)
        })
        .take(50)
        .collect();

    let pool = validate_track_pool(inputs.catalog, inputs.musicbrainz, inputs.cache, candidates).await;
    rotate_take(pool, inputs.rotation_seed, DISPLAY_CAP)
}

// ── C3 — Listened but not recently (Last.fm long-term minus recent) ─────────
async fn build_rediscover_tracks(inputs: &RecoInputs<'_>, _history: &ExtHistory) -> Vec<TrackReco> {
    let Some(lf) = &inputs.lastfm else {
        return Vec::new();
    };
    let (long_term, recent) = tokio::join!(
        lf.client.get_top_tracks(&lf.username, "12month", 60),
        lf.client.get_top_tracks(&lf.username, "1month", 60),
    );
    let recent_keys: HashSet<String> = recent
        .unwrap_or_default()
        .into_iter()
        .map(|t| track_key(&t.artist, &t.name))
        .collect();

    let mut seen: HashSet<String> = HashSet::new();
    let candidates: Vec<TrackCandidate> = long_term
        .unwrap_or_default()
        .into_iter()
        .filter_map(|t| {
            let k = track_key(&t.artist, &t.name);
            if recent_keys.contains(&k) || !seen.insert(k) {
                return None;
            }
            Some(TrackCandidate {
                artist: t.artist,
                title: t.name,
                album: t.album,
                duration_ms: None,
                isrc: None,
                recording_mbid: None,
                source: RecoSource::LastFm,
                score: 0.0,
            })
        })
        .take(50)
        .collect();

    let pool = validate_track_pool(inputs.catalog, inputs.musicbrainz, inputs.cache, candidates).await;
    rotate_take(pool, inputs.rotation_seed, DISPLAY_CAP)
}

// ── C4 — From artists you know but not scrobbled (deep cuts) ────────────────
async fn build_deep_cut_tracks(inputs: &RecoInputs<'_>, history: &ExtHistory) -> Vec<TrackReco> {
    if inputs.local.known_artist_ids.is_empty() {
        return Vec::new();
    }
    // Rotate which known artists we mine today.
    let mut ids: Vec<u64> = inputs.local.known_artist_ids.iter().copied().collect();
    ids.sort_unstable();
    let ids = rotate_take(ids, inputs.rotation_seed, KNOWN_ARTISTS_PER_BUILD);

    let per_artist = stream::iter(ids.into_iter().map(|id| {
        let catalog = inputs.catalog;
        async move { catalog.artist_top_tracks(id, 15).await }
    }))
    .buffered(4)
    .collect::<Vec<_>>()
    .await;

    let mut seen: HashSet<u64> = HashSet::new();
    let mut pool: Vec<TrackReco> = Vec::new();
    for tracks in per_artist {
        // Skip the artist's top hits to favor deep cuts.
        for track in tracks.into_iter().skip(DEEP_CUT_SKIP_TOP) {
            if !track.streamable {
                continue;
            }
            let artist = track
                .performer
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_default();
            let k = track_key(&artist, &track.title);
            // Not scrobbled externally AND not played in-app.
            if history.track_keys.contains(&k) || inputs.local.played_track_ids.contains(&track.id) {
                continue;
            }
            if !seen.insert(track.id) {
                continue;
            }
            pool.push(TrackReco {
                qobuz_track_id: track.id,
                title: track.title.clone(),
                artist,
                artwork_url: track
                    .album
                    .as_ref()
                    .and_then(|al| al.image.best().cloned())
                    .unwrap_or_default(),
                source: RecoSource::Internal,
            });
        }
    }

    rotate_take(pool, inputs.rotation_seed, DISPLAY_CAP)
}

// ── Cold-start fallback — Qobuz editorial top (albums + artists) ────────────
async fn build_editorial_fallback(inputs: &RecoInputs<'_>) -> ExternalCarousels {
    let (most_streamed, new_releases) = tokio::join!(
        inputs.catalog.featured_albums("most-streamed", 20),
        inputs.catalog.featured_albums("new-releases", 20),
    );

    let mut seen_albums: HashSet<String> = HashSet::new();
    let mut top_albums: Vec<AlbumReco> = Vec::new();
    for album in most_streamed.iter().chain(new_releases.iter()) {
        if !album.id.is_empty() && seen_albums.insert(album.id.clone()) {
            top_albums.push(map_album_reco(album));
        }
    }
    top_albums.truncate(20);

    // Top artists: distinct album artists, portraits fetched by id.
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
                source: RecoSource::Editorial,
            }
        }
    }))
    .buffered(4)
    .collect()
    .await;

    ExternalCarousels {
        editorial_fallback: true,
        similar_artists: Vec::new(),
        similar_tracks: Vec::new(),
        rediscover_tracks: Vec::new(),
        deep_cut_tracks: Vec::new(),
        top_albums,
        top_artists,
    }
}
