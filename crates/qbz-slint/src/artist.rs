//! Artist detail controller.
//!
//! Fetches an artist page through `QbzCore`, maps it to plain (Send)
//! data on the worker thread, and applies it to the `ArtistState`
//! global on the Slint event loop.

use std::sync::Arc;

use qbz_app::shell::AppRuntime;
use qbz_core::FrontendAdapter;
use qbz_models::{PageArtistRelease, PageArtistResponse, PageArtistTrack};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::album::TrackData;
use crate::home::CardData;
use crate::{AlbumCardItem, AlbumTrackItem, AppWindow, ArtistState};

/// Plain, `Send` artist data produced on the worker thread.
pub struct ArtistData {
    pub name: String,
    pub bio: String,
    pub artwork_url: String,
    pub top_tracks: Vec<TrackData>,
    pub releases: Vec<CardData>,
}

/// Fetch and map an artist page by id.
pub async fn load_artist<A>(
    runtime: &Arc<AppRuntime<A>>,
    artist_id: &str,
) -> Result<ArtistData, String>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
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

fn map_artist(page: PageArtistResponse) -> ArtistData {
    let name = page.name.display;

    let bio = page
        .biography
        .and_then(|b| b.content)
        .map(|content| strip_html(&content))
        .unwrap_or_default();

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

    let releases = page
        .releases
        .unwrap_or_default()
        .into_iter()
        .flat_map(|group| group.items)
        .map(map_release)
        .collect();

    ArtistData {
        name,
        bio,
        artwork_url,
        top_tracks,
        releases,
    }
}

fn map_track(index: usize, track: PageArtistTrack) -> TrackData {
    let mut title = track.title;
    if let Some(version) = track.version.as_ref().filter(|v| !v.is_empty()) {
        title = format!("{title} ({version})");
    }
    TrackData {
        number: (index + 1).to_string(),
        title,
        artist: track.artist.map(|a| a.name.display).unwrap_or_default(),
        duration: mmss(track.duration.unwrap_or(0)),
        quality_tier: tier(track.audio_info.and_then(|a| a.maximum_bit_depth)).to_string(),
        explicit: track.parental_warning.unwrap_or(false),
    }
}

fn map_release(release: PageArtistRelease) -> CardData {
    let artist = release
        .artist
        .map(|a| a.name.display)
        .or_else(|| release.artists.and_then(|list| list.into_iter().next().map(|a| a.name)))
        .unwrap_or_default();
    let artwork_url = release
        .image
        .and_then(|img| img.best().cloned())
        .unwrap_or_default();
    CardData {
        id: release.id,
        title: release.title,
        artist,
        genre: release.genre.map(|g| g.name).unwrap_or_default(),
        year: String::new(),
        quality_tier: String::new(),
        quality_label: String::new(),
        ribbon: String::new(),
        ribbon_kind: String::new(),
        artwork_url,
    }
}

fn tier(bit_depth: Option<u32>) -> &'static str {
    match bit_depth {
        Some(depth) if depth >= 24 => "hires",
        Some(_) => "cd",
        None => "",
    }
}

fn mmss(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Crude HTML strip for Qobuz biographies (tags + a few entities).
fn strip_html(input: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

/// Apply artist data to the `ArtistState` global. Runs on the Slint event loop.
pub fn apply_artist(window: &AppWindow, data: ArtistData) {
    let top_tracks: Vec<AlbumTrackItem> = data
        .top_tracks
        .into_iter()
        .map(|track| AlbumTrackItem {
            number: track.number.into(),
            title: track.title.into(),
            artist: track.artist.into(),
            duration: track.duration.into(),
            quality_tier: track.quality_tier.into(),
            explicit: track.explicit,
        })
        .collect();
    let releases: Vec<AlbumCardItem> = data
        .releases
        .into_iter()
        .map(|card| AlbumCardItem {
            id: card.id.into(),
            title: card.title.into(),
            artist: card.artist.into(),
            genre: card.genre.into(),
            year: card.year.into(),
            quality_tier: card.quality_tier.into(),
            quality_label: slint::SharedString::new(),
            ribbon: card.ribbon.into(),
            ribbon_kind: card.ribbon_kind.into(),
            artwork_url: card.artwork_url.into(),
            artwork: slint::Image::default(),
        })
        .collect();

    let state = window.global::<ArtistState>();
    state.set_name(data.name.into());
    state.set_bio(data.bio.into());
    state.set_top_tracks(ModelRc::new(VecModel::from(top_tracks)));
    state.set_releases(ModelRc::new(VecModel::from(releases)));
}

/// Clear artist state before loading a new artist.
pub fn reset_artist(window: &AppWindow) {
    let state = window.global::<ArtistState>();
    state.set_top_tracks(ModelRc::new(VecModel::from(Vec::<AlbumTrackItem>::new())));
    state.set_releases(ModelRc::new(VecModel::from(Vec::<AlbumCardItem>::new())));
    state.set_artwork(slint::Image::default());
    state.set_name("".into());
    state.set_bio("".into());
    state.set_loading(true);
}

/// Apply decoded portrait artwork. Runs on the Slint event loop.
pub fn apply_artwork(window: &AppWindow, pixels: &[u8], width: u32, height: u32) {
    let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(width, height);
    let dst = buffer.make_mut_bytes();
    if dst.len() != pixels.len() {
        return;
    }
    dst.copy_from_slice(pixels);
    window
        .global::<ArtistState>()
        .set_artwork(slint::Image::from_rgba8(buffer));
}
