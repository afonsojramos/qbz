//! Discover / Home controller.
//!
//! Fetches the Qobuz discover index through `QbzCore`, maps it into plain
//! (Send) data on the worker thread, and — separately, on the Slint event
//! loop — converts that into Slint models pushed onto the `HomeState`
//! global. Domain types never reach the `.slint` files.

use std::cell::RefCell;
use std::sync::Arc;

use qbz_app::shell::AppRuntime;
use qbz_core::FrontendAdapter;
use qbz_models::{
    Album, AlbumAward, DiscoverAlbum, DiscoverAudioInfo, DiscoverContainer,
};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::artwork::{ArtworkJob, ArtworkTarget};
use crate::{AlbumCardItem, AppWindow, DiscoverSection, HomeState, SlimItem};

/// Plain, `Send` home data produced on the worker thread.
pub struct HomeData {
    pub sections: Vec<SectionData>,
    /// Editorial-only section set for the Editor's Picks tab.
    pub editor_sections: Vec<SectionData>,
    /// Personalized section set for the For You tab.
    pub foryou_sections: Vec<SectionData>,
    pub popular: Vec<SlimData>,
    pub recent: Vec<SlimData>,
    pub recent_albums: Vec<CardData>,
}

thread_local! {
    /// The per-tab section sets, cached on the UI thread after a load
    /// so a tab switch can swap HomeState.sections without re-fetching.
    /// (home, editor, foryou)
    static TAB_SECTIONS: RefCell<TabSections> = RefCell::new(TabSections::default());
}

#[derive(Default)]
struct TabSections {
    home: Vec<SectionData>,
    editor: Vec<SectionData>,
    foryou: Vec<SectionData>,
}

#[derive(Clone)]
pub struct SectionData {
    pub title: String,
    pub albums: Vec<CardData>,
}

#[derive(Clone)]
pub struct CardData {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub genre: String,
    pub year: String,
    /// "hires" | "cd" | "" — drives the icon-only quality badge.
    pub quality_tier: String,
    /// "Hi-Res: 24-bit / 96 kHz" — shown when hovering the quality badge.
    pub quality_label: String,
    pub ribbon: String,
    pub ribbon_kind: String,
    pub artwork_url: String,
}

/// A compact ranked item for the slim grid sections.
pub struct SlimData {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub rank: String,
    pub artwork_url: String,
}

/// Fetch the discover index and map it into Home sections.
pub async fn load_home<A>(runtime: &Arc<AppRuntime<A>>) -> Result<HomeData, String>
where
    A: FrontendAdapter + Send + Sync + 'static,
{
    let response = runtime
        .core()
        .get_discover_index(None)
        .await
        .map_err(|e| e.to_string())?;
    let containers = response.containers;

    // Editorial-only set for the Editor's Picks tab — built first
    // (by cloning the containers) so the same data can also feed the
    // Home set and the most-streamed slim grid below. Order mirrors
    // Tauri's DEFAULT_PREFS.editorPicks.
    let mut editor_sections = Vec::new();
    push_section_ref(&mut editor_sections, "New Releases", &containers.new_releases);
    push_section_ref(&mut editor_sections, "Qobuzissimes", &containers.qobuzissims);
    push_section_ref(&mut editor_sections, "Press Accolades", &containers.press_awards);
    push_section_ref(&mut editor_sections, "Most Streamed", &containers.most_streamed);
    push_section_ref(
        &mut editor_sections,
        "Ideal Discography",
        &containers.ideal_discography,
    );
    push_section_ref(
        &mut editor_sections,
        "Albums of the Week",
        &containers.album_of_the_week,
    );

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

    // Capped at 24 (two carousel pages of 12) — the slim carousel
    // does not show beyond that.
    let popular = containers
        .most_streamed
        .map(|container| container.data.items)
        .unwrap_or_default()
        .into_iter()
        .take(24)
        .enumerate()
        .map(|(index, album)| map_slim(index, album))
        .collect();

    // For You — release watch (this week's releases from followed
    // artists). The deeper personalized sections from Tauri (qobuz
    // mixes, similar albums, spotlight) need reco-DB backend that the
    // Slint MVP does not have yet; this is the honest first cut.
    let mut foryou_sections = Vec::new();
    if let Ok(page) = runtime.core().get_release_watch("artists", 18, 0).await {
        let albums: Vec<CardData> = page.items.into_iter().map(map_full_album).collect();
        if !albums.is_empty() {
            foryou_sections.push(SectionData {
                title: "Release Watch".to_string(),
                albums,
            });
        }
    }

    // Recently played comes from the local play-history store, not the
    // discover index. Empty until the playback session records plays.
    // Capped at 24 (two carousel pages of 12).
    let recent = crate::recently::load()
        .into_iter()
        .take(24)
        .map(|track| SlimData {
            id: track.id,
            title: track.title,
            subtitle: track.subtitle,
            rank: String::new(),
            artwork_url: track.artwork_url,
        })
        .collect();
    let recent_albums = crate::recently::load_albums()
        .into_iter()
        .map(|album| CardData {
            id: album.id,
            title: album.title,
            artist: album.artist,
            genre: String::new(),
            year: String::new(),
            quality_tier: String::new(),
            quality_label: String::new(),
            ribbon: String::new(),
            ribbon_kind: String::new(),
            artwork_url: album.artwork_url,
        })
        .collect();

    Ok(HomeData {
        sections,
        editor_sections,
        foryou_sections,
        popular,
        recent,
        recent_albums,
    })
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

/// Like `push_section` but borrows the container (clones the items)
/// so the same data can feed more than one tab's section set.
fn push_section_ref(
    out: &mut Vec<SectionData>,
    title: &str,
    container: &Option<DiscoverContainer<DiscoverAlbum>>,
) {
    let Some(container) = container else {
        return;
    };
    if container.data.items.is_empty() {
        return;
    }
    out.push(SectionData {
        title: title.to_string(),
        albums: container.data.items.iter().cloned().map(map_album).collect(),
    });
}

/// Map a full catalog `Album` (release-watch result) to a card. Unlike
/// `map_album` (which takes the discover-index `DiscoverAlbum`), this
/// reads the standard Album shape.
fn map_full_album(album: Album) -> CardData {
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
    CardData {
        id: album.id,
        title: album.title,
        artist: album.artist.name,
        genre: album.genre.map(|g| g.name).unwrap_or_default(),
        year,
        quality_tier,
        quality_label,
        ribbon: String::new(),
        ribbon_kind: String::new(),
        artwork_url: album.image.best().cloned().unwrap_or_default(),
    }
}

fn map_album(album: DiscoverAlbum) -> CardData {
    let artist = album
        .artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let genre = album.genre.map(|g| g.name).unwrap_or_default();
    let year = album
        .dates
        .as_ref()
        .and_then(|d| d.original.as_ref().or(d.download.as_ref()).or(d.stream.as_ref()))
        .and_then(|date| date.get(0..4))
        .unwrap_or("")
        .to_string();
    let (ribbon, ribbon_kind) = pick_ribbon(album.awards.as_deref());
    let quality_tier = quality_tier(album.audio_info.as_ref()).to_string();
    let quality_label = quality_label(album.audio_info.as_ref());
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
        genre,
        year,
        quality_tier,
        quality_label,
        ribbon,
        ribbon_kind,
        artwork_url,
    }
}

fn map_slim(index: usize, album: DiscoverAlbum) -> SlimData {
    let subtitle = album
        .artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_default();
    let artwork_url = album
        .image
        .thumbnail
        .or(album.image.small)
        .or(album.image.large)
        .unwrap_or_default();
    SlimData {
        id: album.id,
        title: album.title,
        subtitle,
        rank: (index + 1).to_string(),
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

/// Exact-quality label for the badge hover tooltip, mirroring the Tauri
/// `QualityBadge` (`{tier}: {depth}-bit / {rate} kHz`). Empty when the
/// discover entry carries no audio info, matching `quality_tier`.
fn quality_label(audio: Option<&DiscoverAudioInfo>) -> String {
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

/// Format a kHz sample rate without a trailing `.0` (96.0 -> "96",
/// 44.1 -> "44.1").
fn format_rate(rate: f64) -> String {
    if (rate.fract()).abs() < f64::EPSILON {
        format!("{}", rate as i64)
    } else {
        format!("{rate}")
    }
}

/// Convert one `CardData` into the Slint `AlbumCardItem`.
fn card_to_item(card: CardData) -> AlbumCardItem {
    AlbumCardItem {
        id: card.id.into(),
        title: card.title.into(),
        artist: card.artist.into(),
        genre: card.genre.into(),
        year: card.year.into(),
        quality_tier: card.quality_tier.into(),
        quality_label: card.quality_label.into(),
        ribbon: card.ribbon.into(),
        ribbon_kind: card.ribbon_kind.into(),
        artwork_url: card.artwork_url.into(),
        artwork: slint::Image::default(),
    }
}

/// Build the Slint section model for one tab's section set.
fn build_sections(sections: &[SectionData]) -> Vec<DiscoverSection> {
    sections
        .iter()
        .map(|section| DiscoverSection {
            title: section.title.clone().into(),
            albums: ModelRc::new(VecModel::from(
                section.albums.iter().cloned().map(card_to_item).collect::<Vec<_>>(),
            )),
        })
        .collect()
}

/// Artwork jobs for a tab's section set (Section-targeted, so they
/// land in HomeState.sections — the model the active tab renders).
pub fn section_artwork_jobs(sections: &[SectionData]) -> Vec<ArtworkJob> {
    let mut jobs = Vec::new();
    for (section_idx, section) in sections.iter().enumerate() {
        for (album_idx, card) in section.albums.iter().enumerate() {
            if card.artwork_url.is_empty() {
                continue;
            }
            jobs.push(ArtworkJob {
                target: ArtworkTarget::Section {
                    section_idx,
                    album_idx,
                },
                url: card.artwork_url.clone(),
            });
        }
    }
    jobs
}

/// Switch the visible Discover tab. Reads the cached section set for
/// `tab` ("home" | "editorPicks" | "forYou"), swaps it into
/// HomeState.sections, and returns the artwork jobs to re-fire. No
/// re-fetch — the sets were cached by the last apply_home.
pub fn select_tab(window: &AppWindow, tab: &str) -> Vec<ArtworkJob> {
    TAB_SECTIONS.with(|cell| {
        let cache = cell.borrow();
        let set = match tab {
            "editorPicks" => &cache.editor,
            "forYou" => &cache.foryou,
            _ => &cache.home,
        };
        let state = window.global::<HomeState>();
        state.set_sections(ModelRc::new(VecModel::from(build_sections(set))));
        state.set_active_tab(tab.into());
        section_artwork_jobs(set)
    })
}

/// Convert worker-thread home data into Slint models and push them onto
/// the `HomeState` global. Must run on the Slint event loop.
pub fn apply_home(window: &AppWindow, data: HomeData) {
    let sections: Vec<DiscoverSection> = build_sections(&data.sections);

    // Cache all three tab section sets for instant tab switching.
    TAB_SECTIONS.with(|cell| {
        *cell.borrow_mut() = TabSections {
            home: data.sections.clone(),
            editor: data.editor_sections.clone(),
            foryou: data.foryou_sections.clone(),
        };
    });

    let to_slim_items = |items: Vec<SlimData>| -> Vec<SlimItem> {
        items
            .into_iter()
            .map(|slim| SlimItem {
                id: slim.id.into(),
                title: slim.title.into(),
                subtitle: slim.subtitle.into(),
                rank: slim.rank.into(),
                artwork_url: slim.artwork_url.into(),
                artwork: slint::Image::default(),
                following: false,
            })
            .collect()
    };
    let popular = to_slim_items(data.popular);
    let recent = to_slim_items(data.recent);
    let recent_albums: Vec<AlbumCardItem> =
        data.recent_albums.into_iter().map(card_to_item).collect();

    let state = window.global::<HomeState>();
    state.set_sections(ModelRc::new(VecModel::from(sections)));
    state.set_popular(ModelRc::new(VecModel::from(popular)));
    state.set_recent(ModelRc::new(VecModel::from(recent)));
    state.set_recent_albums(ModelRc::new(VecModel::from(recent_albums)));
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
