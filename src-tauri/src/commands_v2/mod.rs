//! V2 Commands - Using the new multi-crate architecture
//!
//! These commands use QbzCore via CoreBridge instead of the old AppState.
//! Runtime contract ensures proper lifecycle (see ADR_RUNTIME_SESSION_CONTRACT.md).
//!
//! Playback flows through CoreBridge -> QbzCore -> Player (qbz-player crate).

use tauri::State;

use qbz_models::{
    Album, Artist, DiscoverAlbum, DiscoverData, DiscoverPlaylistsResponse, DiscoverResponse,
    GenreInfo, LabelDetail, LabelExploreResponse, LabelPageData, PageArtistResponse, Playlist,
    PlaylistTag, SearchResultsPage,
    Track, UserSession,
};

use crate::api::models::{
    PlaylistDuplicateResult, PlaylistWithTrackIds,
};
use crate::artist_blacklist::BlacklistState;
use crate::audio::{AlsaPlugin, AudioBackendType, AudioDevice, BackendManager};
use crate::cache::CacheStats;
use crate::config::audio_settings::{AudioSettings, AudioSettingsState};
use crate::config::developer_settings::DeveloperSettingsState;
use crate::config::favorites_preferences::FavoritesPreferences;
use crate::config::graphics_settings::GraphicsSettingsState;
use crate::config::legal_settings::LegalSettingsState;
use crate::config::playback_preferences::{
    AutoplayMode, PlaybackPreferences, PlaybackPreferencesState,
};
use crate::config::tray_settings::TraySettings;
use crate::config::tray_settings::TraySettingsState;
use crate::config::window_settings::WindowSettingsState;
use crate::core_bridge::CoreBridgeState;
use crate::library::LibraryState;
use crate::reco_store::RecoState;
use crate::runtime::{
    CommandRequirement, RuntimeError, RuntimeManagerState,
};
use crate::AppState;
use crate::integrations_v2::MusicBrainzV2State;
use std::collections::HashSet;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct V2SuggestionArtistInput {
    pub name: String,
    pub qobuz_id: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct V2PlaylistSuggestionsInput {
    pub artists: Vec<V2SuggestionArtistInput>,
    pub exclude_track_ids: Vec<u64>,
    #[serde(default)]
    pub include_reasons: bool,
    pub config: Option<crate::artist_vectors::SuggestionConfig>,
}

mod helpers;
pub use helpers::*;

mod runtime;
pub use runtime::*;

mod playback;
pub use playback::*;

mod auth;
pub use auth::*;

mod settings;
pub use settings::*;

mod library;
pub use library::*;

mod link_resolver;
pub use link_resolver::*;

mod queue;
pub use queue::*;

mod search;
pub use search::*;

mod favorites;
pub use favorites::*;

mod audio;
pub use audio::*;

mod playlists;
pub use playlists::*;

mod catalog;
pub use catalog::*;

mod integrations;
pub use integrations::*;

mod session;
pub use session::*;

mod legacy_compat;
pub use legacy_compat::*;

mod image_cache;
pub use image_cache::*;

mod discovery;
pub use discovery::*;
pub(crate) use discovery::normalize_artist_name;

mod diagnostics;
pub use diagnostics::*;
