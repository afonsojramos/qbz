//! MusicianPageView controller — loads the resolved musician + the
//! "Appears On" album grid and pushes them into `MusicianState`.
//!
//! Mirrors the Tauri MusicianPageView.svelte flow:
//!   1. Resolve the (name, role) via QbzCore::musicbrainz_resolve_musician.
//!   2. Fetch the first page of appearances
//!      (QbzCore::musicbrainz_get_musician_appearances).
//!   3. Subsequent pages come from the MusicianActions::load-more
//!      callback the view emits at the bottom of the grid.

use std::sync::Arc;

use qbz_app::shell::AppRuntime;
use qbz_core::FrontendAdapter;
use qbz_integrations::musicbrainz::MusicianConfidence;
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::artwork::{ArtworkJob, ArtworkTarget};
use crate::{AppWindow, MusicianAppearanceItem, MusicianState};

/// Page size — kept in sync with the Tauri view's ITEMS_PER_PAGE.
pub const PAGE_SIZE: u32 = 20;

/// Resolved-musician metadata + the current page bookkeeping. Plain
/// `Send` so the load step can run on a worker.
pub struct MusicianData {
    pub name: String,
    pub role: String,
    pub confidence: MusicianConfidence,
    pub appearances: Vec<AppearanceData>,
    pub total: usize,
}

#[derive(Clone)]
pub struct AppearanceData {
    pub album_id: String,
    pub album_title: String,
    pub artist_name: String,
    pub year: String,
    pub role_on_album: String,
    pub artwork_url: String,
}

/// Resolve the musician + fetch the first page of appearances.
pub async fn load_musician<A>(
    runtime: &Arc<AppRuntime<A>>,
    name: &str,
    role: &str,
) -> Result<MusicianData, String>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    let resolved = runtime
        .core()
        .musicbrainz_resolve_musician(name, role)
        .await
        .map_err(|e| e.to_string())?;

    let page = runtime
        .core()
        .musicbrainz_get_musician_appearances(name, role, PAGE_SIZE, 0)
        .await
        .map_err(|e| e.to_string())?;

    let appearances: Vec<AppearanceData> = page
        .albums
        .into_iter()
        .map(|a| AppearanceData {
            album_id: a.album_id,
            album_title: a.album_title,
            artist_name: a.artist_name,
            year: a.year.unwrap_or_default(),
            role_on_album: a.role_on_album,
            artwork_url: a.album_artwork,
        })
        .collect();

    Ok(MusicianData {
        name: resolved.name,
        role: resolved.role,
        confidence: resolved.confidence,
        appearances,
        total: page.total,
    })
}

/// Fetch one more page of appearances, append to the current list.
pub async fn load_more_appearances<A>(
    runtime: &Arc<AppRuntime<A>>,
    name: &str,
    role: &str,
    offset: u32,
) -> Result<(Vec<AppearanceData>, usize), String>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    let page = runtime
        .core()
        .musicbrainz_get_musician_appearances(name, role, PAGE_SIZE, offset)
        .await
        .map_err(|e| e.to_string())?;
    let appearances = page
        .albums
        .into_iter()
        .map(|a| AppearanceData {
            album_id: a.album_id,
            album_title: a.album_title,
            artist_name: a.artist_name,
            year: a.year.unwrap_or_default(),
            role_on_album: a.role_on_album,
            artwork_url: a.album_artwork,
        })
        .collect();
    Ok((appearances, page.total))
}

/// Apply the freshly loaded musician page to MusicianState.
pub fn apply_musician(window: &AppWindow, data: MusicianData) {
    let items: Vec<MusicianAppearanceItem> = data
        .appearances
        .into_iter()
        .map(|a| MusicianAppearanceItem {
            album_id: a.album_id.into(),
            album_title: a.album_title.into(),
            artist_name: a.artist_name.into(),
            year: a.year.into(),
            role_on_album: a.role_on_album.into(),
            artwork_url: a.artwork_url.into(),
            artwork: slint::Image::default(),
        })
        .collect();
    let state = window.global::<MusicianState>();
    state.set_name(data.name.into());
    state.set_role(data.role.into());
    state.set_confidence(confidence_label(data.confidence).into());
    state.set_appearances(ModelRc::new(VecModel::from(items)));
    state.set_total(data.total as i32);
    state.set_loading(false);
}

/// Append a freshly fetched page of appearances onto the existing
/// model. Called by the MusicianActions::load-more handler.
pub fn append_appearances(
    window: &AppWindow,
    appearances: Vec<AppearanceData>,
    total: usize,
) {
    let state = window.global::<MusicianState>();
    let model = state.get_appearances();
    let mut combined: Vec<MusicianAppearanceItem> = (0..model.row_count())
        .filter_map(|i| model.row_data(i))
        .collect();
    for a in appearances {
        combined.push(MusicianAppearanceItem {
            album_id: a.album_id.into(),
            album_title: a.album_title.into(),
            artist_name: a.artist_name.into(),
            year: a.year.into(),
            role_on_album: a.role_on_album.into(),
            artwork_url: a.artwork_url.into(),
            artwork: slint::Image::default(),
        });
    }
    state.set_appearances(ModelRc::new(VecModel::from(combined)));
    state.set_total(total as i32);
    state.set_load_more_loading(false);
}

pub fn reset_musician(window: &AppWindow) {
    let state = window.global::<MusicianState>();
    state.set_name("".into());
    state.set_role("".into());
    state.set_confidence("".into());
    state.set_appearances(ModelRc::new(VecModel::from(
        Vec::<MusicianAppearanceItem>::new(),
    )));
    state.set_total(0);
    state.set_loading(true);
    state.set_load_more_loading(false);
}

/// Artwork download jobs for the appearance grid — same pipeline
/// the Discover album cards use, so covers fill in progressively.
pub fn artwork_jobs(data: &MusicianData) -> Vec<ArtworkJob> {
    data.appearances
        .iter()
        .enumerate()
        .filter(|(_, a)| !a.artwork_url.is_empty())
        .map(|(i, a)| ArtworkJob {
            url: a.artwork_url.clone(),
            target: ArtworkTarget::MusicianAppearance { index: i },
        })
        .collect()
}

fn confidence_label(c: MusicianConfidence) -> &'static str {
    match c {
        MusicianConfidence::Confirmed => "confirmed",
        MusicianConfidence::Contextual => "contextual",
        MusicianConfidence::Weak => "weak",
        MusicianConfidence::None => "none",
    }
}
