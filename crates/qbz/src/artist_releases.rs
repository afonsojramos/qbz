//! Dedicated artist discography page — the full, paginated listing for a
//! single release bucket reached via "See discography" on the artist page.
//! Reuses `artist::load_release_page` (get_releases_grid) for fetching and
//! `artist::card_to_item` for mapping; sort is the same client-side 5-mode
//! as the index sections (persisted per release_type via `artist_prefs`).

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::artwork::{ArtworkJob, ArtworkTarget};
use crate::home::CardData;
use crate::{AlbumCardItem, AppWindow, ArtistReleasesState};

/// Reset the page state for a fresh open (sets header + clears the grid).
pub fn reset(window: &AppWindow, artist_id: &str, name: &str, release_type: &str, title: &str) {
    let st = window.global::<ArtistReleasesState>();
    st.set_id(artist_id.into());
    st.set_name(name.into());
    st.set_release_type(release_type.into());
    st.set_title(title.into());
    st.set_albums(ModelRc::new(VecModel::from(Vec::<AlbumCardItem>::new())));
    st.set_has_more(false);
    st.set_loading(true);
    st.set_load_more_loading(false);
    st.set_load_error(false);
    st.set_sort_by(crate::artist_prefs::get_sort(release_type).into());
}

/// Apply a fetched page. `replace` = the first page (clears the grid);
/// otherwise the cards are appended (deduped). Re-sorts the full set by the
/// current sort and returns artwork jobs for the NEW cards at their final
/// positions.
pub fn apply_page(
    window: &AppWindow,
    cards: Vec<CardData>,
    has_more: bool,
    replace: bool,
) -> Vec<ArtworkJob> {
    let st = window.global::<ArtistReleasesState>();
    let sort = st.get_sort_by().to_string();

    let mut items: Vec<AlbumCardItem> = if replace {
        Vec::new()
    } else {
        st.get_albums().iter().collect()
    };
    let mut seen: std::collections::HashSet<String> =
        items.iter().map(|a| a.id.to_string()).collect();
    let mut new_ids: Vec<String> = Vec::new();
    for card in cards {
        if crate::artist_blacklist::card_blacklisted(&card.id, &card.artist_id) {
            continue;
        }
        let item = crate::artist::card_to_item(card);
        let id = item.id.to_string();
        if seen.contains(&id) {
            continue;
        }
        seen.insert(id.clone());
        new_ids.push(id);
        items.push(item);
    }
    crate::album_map::sort_album_items(&mut items, &sort);

    let mut jobs = Vec::new();
    for (idx, item) in items.iter().enumerate() {
        let is_new = new_ids.iter().any(|id| id == item.id.as_str());
        if is_new && !item.artwork_url.as_str().is_empty() {
            jobs.push(ArtworkJob {
                target: ArtworkTarget::ArtistReleasesAlbum { index: idx },
                url: item.artwork_url.to_string(),
            });
        }
    }

    st.set_albums(ModelRc::new(VecModel::from(items)));
    st.set_has_more(has_more);
    st.set_loading(false);
    st.set_load_more_loading(false);
    jobs
}

/// Re-sort the loaded grid in place (preserves artwork) and persist.
pub fn resort(window: &AppWindow, sort: &str) {
    let st = window.global::<ArtistReleasesState>();
    let release_type = st.get_release_type().to_string();
    crate::artist_prefs::set_sort(&release_type, sort);
    let mut items: Vec<AlbumCardItem> = st.get_albums().iter().collect();
    crate::album_map::sort_album_items(&mut items, sort);
    st.set_albums(ModelRc::new(VecModel::from(items)));
    st.set_sort_by(sort.into());
}

/// Current loaded item count — the offset for the next page.
pub fn loaded_count(window: &AppWindow) -> u32 {
    window
        .global::<ArtistReleasesState>()
        .get_albums()
        .row_count() as u32
}
