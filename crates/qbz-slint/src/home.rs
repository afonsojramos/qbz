//! Discover / Home controller.
//!
//! Fetches the Qobuz discover index through `QbzCore`, maps it into plain
//! (Send) section data on the worker thread, and — separately, on the
//! Slint event loop — converts that into Slint models pushed onto the
//! `HomeState` global. Domain types never reach the `.slint` files.

use std::sync::Arc;

use qbz_app::shell::AppRuntime;
use qbz_core::FrontendAdapter;
use qbz_models::{AlbumAward, DiscoverAlbum, DiscoverAudioInfo, DiscoverContainer};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::{AlbumCardItem, AppWindow, DiscoverSection, HomeState};

/// Plain, `Send` section data produced on the worker thread.
pub struct SectionData {
    pub title: String,
    pub albums: Vec<CardData>,
}

pub struct CardData {
    pub id: String,
    pub title: String,
    pub artist: String,
    /// "hires" | "cd" | "" — drives the icon-only quality badge.
    pub quality_tier: String,
    pub ribbon: String,
    pub ribbon_kind: String,
    pub artwork_url: String,
}

/// Fetch the discover index and map it into Home sections.
pub async fn load_home<A>(runtime: &Arc<AppRuntime<A>>) -> Result<Vec<SectionData>, String>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    let response = runtime
        .core()
        .get_discover_index(None)
        .await
        .map_err(|e| e.to_string())?;
    let containers = response.containers;

    let mut sections = Vec::new();
    push_section(&mut sections, "New Releases", containers.new_releases);
    push_section(&mut sections, "Press Accolades", containers.press_awards);
    push_section(
        &mut sections,
        "Ideal Discography",
        containers.ideal_discography,
    );
    push_section(&mut sections, "Qobuzissimes", containers.qobuzissims);
    push_section(
        &mut sections,
        "Albums of the Week",
        containers.album_of_the_week,
    );
    Ok(sections)
}

fn push_section(
    out: &mut Vec<SectionData>,
    title: &str,
    container: Option<DiscoverContainer<DiscoverAlbum>>,
) {
    let Some(container) = container else {
        return;
    };
    if container.data.items.is_empty() {
        return;
    }
    out.push(SectionData {
        title: title.to_string(),
        albums: container.data.items.into_iter().map(map_album).collect(),
    });
}

fn map_album(album: DiscoverAlbum) -> CardData {
    let artist = album
        .artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let (ribbon, ribbon_kind) = pick_ribbon(album.awards.as_deref());
    let artwork_url = album
        .image
        .large
        .or(album.image.thumbnail)
        .or(album.image.small)
        .unwrap_or_default();
    CardData {
        id: album.id,
        title: album.title,
        artist,
        quality_tier: quality_tier(album.audio_info.as_ref()).to_string(),
        ribbon,
        ribbon_kind,
        artwork_url,
    }
}

/// Pick the single award ribbon, mirroring `pickAlbumRibbon` in data.ts:
/// award id 151 = Album of the Week, 88 = Qobuzissime, otherwise the last
/// award becomes a generic "press" ribbon.
fn pick_ribbon(awards: Option<&[AlbumAward]>) -> (String, String) {
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

/// Classify the quality tier for the icon-only badge: 24-bit and up is
/// Hi-Res, anything else with audio info is CD-quality.
fn quality_tier(audio: Option<&DiscoverAudioInfo>) -> &'static str {
    let Some(audio) = audio else {
        return "";
    };
    match audio.maximum_bit_depth {
        Some(depth) if depth >= 24 => "hires",
        _ => "cd",
    }
}

/// Convert worker-thread section data into Slint models and push them onto
/// the `HomeState` global. Must run on the Slint event loop.
pub fn apply_sections(window: &AppWindow, data: Vec<SectionData>) {
    let sections: Vec<DiscoverSection> = data
        .into_iter()
        .map(|section| {
            let albums: Vec<AlbumCardItem> = section
                .albums
                .into_iter()
                .map(|card| AlbumCardItem {
                    id: card.id.into(),
                    title: card.title.into(),
                    artist: card.artist.into(),
                    quality_tier: card.quality_tier.into(),
                    ribbon: card.ribbon.into(),
                    ribbon_kind: card.ribbon_kind.into(),
                    artwork_url: card.artwork_url.into(),
                    artwork: slint::Image::default(),
                })
                .collect();
            DiscoverSection {
                title: section.title.into(),
                albums: ModelRc::new(VecModel::from(albums)),
            }
        })
        .collect();
    window
        .global::<HomeState>()
        .set_sections(ModelRc::new(VecModel::from(sections)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(bit_depth: Option<u32>) -> DiscoverAudioInfo {
        DiscoverAudioInfo {
            maximum_bit_depth: bit_depth,
            maximum_sampling_rate: Some(96.0),
            maximum_channel_count: Some(2),
        }
    }

    #[test]
    fn quality_tier_hires_for_24_bit() {
        assert_eq!(quality_tier(Some(&audio(Some(24)))), "hires");
    }

    #[test]
    fn quality_tier_cd_for_16_bit() {
        assert_eq!(quality_tier(Some(&audio(Some(16)))), "cd");
    }

    #[test]
    fn quality_tier_empty_without_audio_info() {
        assert_eq!(quality_tier(None), "");
    }

    #[test]
    fn ribbon_prioritizes_album_of_the_week() {
        let awards = vec![
            AlbumAward {
                id: Some("88".into()),
                name: "Qobuzissime".into(),
                awarded_at: None,
            },
            AlbumAward {
                id: Some("151".into()),
                name: "Album of the Week".into(),
                awarded_at: None,
            },
        ];
        let (label, kind) = pick_ribbon(Some(&awards));
        assert_eq!(kind, "albumOfTheWeek");
        assert_eq!(label, "Album of the Week");
    }

    #[test]
    fn ribbon_falls_back_to_press() {
        let awards = vec![AlbumAward {
            id: Some("7".into()),
            name: "Gramophone Editor's Choice".into(),
            awarded_at: None,
        }];
        let (label, kind) = pick_ribbon(Some(&awards));
        assert_eq!(kind, "press");
        assert_eq!(label, "Gramophone Editor's Choice");
    }

    #[test]
    fn ribbon_empty_when_no_awards() {
        assert_eq!(pick_ribbon(None), (String::new(), String::new()));
    }
}
