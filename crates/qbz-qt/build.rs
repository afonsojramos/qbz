use std::path::Path;

use cxx_qt_build::{CxxQtBuilder, QmlModule};

/// Collect every file under `dir` (recursive), crate-root-relative — the
/// baked icon variants (qml/assets/icons/<tint>/<name>.svg) are too many to
/// list by hand.
fn collect_qrc_files(dir: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display())) {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_qrc_files(&path, out);
        } else {
            out.push(path.to_string_lossy().into_owned());
        }
    }
}

fn main() {
    // The WHOLE asset tree, recursively. This used to name the root-level
    // files by hand with only icons/ and fonts/ collected, so dropping a new
    // asset next to hi-res.svg compiled fine and then failed at runtime with
    // "QQuickImage: Cannot open" — invisible to cargo check and to the tests.
    let mut qrc_files: Vec<String> = Vec::new();
    collect_qrc_files(Path::new("qml/assets"), &mut qrc_files);
    // Non-QML resources land at qrc:/qt/qml/com/blitzfc/qbz/<path> — QML
    // files in qml/ reference them relatively (e.g.
    // "assets/icons/primary/plus.svg").
    let qrc_refs: Vec<&str> = qrc_files.iter().map(String::as_str).collect();
    // Cargo's default is to rerun build.rs only when a source file changes, so
    // dropping a new icon into qml/assets/ produced a binary whose qrc did not
    // contain it — the file was on disk and the app logged "Cannot open".
    // Watch the whole tree instead.
    println!("cargo:rerun-if-changed=qml");
    for f in &qrc_files {
        println!("cargo:rerun-if-changed={f}");
    }

    CxxQtBuilder::new()
        // Qt modules the bridge links against (Qt6 CMake in /usr/lib64/cmake).
        .qt_module("Qml")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qml_module(QmlModule {
            uri: "com.blitzfc.qbz",
            // EVERY #[cxx_qt::bridge] file. A bridge missing here does not
            // fail the build: its QML singleton simply does not exist, and
            // every `QbzFoo.bar()` in QML becomes a runtime ReferenceError
            // that `cargo check` cannot see. The four MyQBZ/Blacklist
            // singletons are the newest arrivals; the domain CONTROLLERS
            // (myqbz_qt.rs, blacklist_qt.rs, toast_qt.rs, …) are plain
            // modules and must NOT be listed — only files that declare a
            // #[cxx_qt::bridge] mod belong in this array.
            rust_files: &["src/bridge.rs", "src/session_bridge.rs", "src/shell_bridge.rs", "src/player_bridge.rs", "src/queue_bridge.rs", "src/home_bridge.rs", "src/viz_bridge.rs", "src/immersive_bridge.rs", "src/suggestions_bridge.rs", "src/hotkeys_bridge.rs", "src/search_bridge.rs", "src/local_bridge.rs", "src/library_bridge.rs", "src/album_bridge.rs", "src/artist_bridge.rs", "src/lyrics_qt.rs", "src/icon_tint_qt.rs", "src/cast_bridge.rs", "src/myqbz_bridge.rs", "src/myqbz_add_bridge.rs", "src/disco_bridge.rs", "src/blacklist_bridge.rs", "src/playlist_picker_bridge.rs", "src/playlist_manager_bridge.rs", "src/playlist_import_bridge.rs", "src/folder_edit_bridge.rs", "src/playlist_edit_bridge.rs", "src/qconnect_bridge.rs", "src/kiosk_nav_bridge.rs", "src/mini_bridge.rs", "src/tray_bridge.rs"],
            qml_files: &[
                "qml/LoginScreen.qml",
                "qml/Main.qml",
                "qml/cards/AlbumCard.qml",
                "qml/cards/ArtistCard.qml",
                "qml/cards/CollectionMosaic.qml",
                "qml/cards/LabelCard.qml",
                "qml/cards/MixArtwork.qml",
                "qml/cards/PlaylistCard.qml",
                "qml/cards/PlaylistCollage.qml",
                "qml/cards/RadioCard.qml",
                "qml/cards/SlimCard.qml",
                "qml/cards/TrackCard.qml",
                "qml/controls/AddToMixtapeModal.qml",
                "qml/controls/CardMenu.qml",
                "qml/controls/CardOverlayButton.qml",
                "qml/controls/CardOverlayRow.qml",
                "qml/controls/FolderEditPanel.qml",
                "qml/controls/FolderModals.qml",
                "qml/controls/GroupHeader.qml",
                "qml/controls/MyQbzModals.qml",
                "qml/controls/PlaylistEditModal.qml",
                "qml/controls/PlaylistImportModal.qml",
                "qml/controls/PlaylistPickerModal.qml",
                "qml/controls/PmFolderIcon.qml",
                "qml/controls/QbzCircleAction.qml",
                "qml/controls/QbzConfirmModal.qml",
                "qml/controls/QbzContextMenu.qml",
                "qml/controls/QbzEmptyState.qml",
                "qml/controls/QbzIconButton.qml",
                "qml/controls/QbzLineEdit.qml",
                "qml/controls/QbzLoadingDots.qml",
                "qml/controls/QbzLoadMore.qml",
                "qml/controls/QbzMultiSelectBar.qml",
                "qml/controls/QbzNavButton.qml",
                "qml/controls/QbzOfflinePlaceholder.qml",
                "qml/controls/QbzPrimaryButton.qml",
                "qml/controls/QbzRadioOption.qml",
                "qml/controls/QbzSectionHeader.qml",
                "qml/controls/QbzSegToggle.qml",
                "qml/controls/QbzSelect.qml",
                "qml/controls/QbzSlider.qml",
                "qml/controls/QbzTabBar.qml",
                "qml/controls/QbzTextArea.qml",
                "qml/controls/QbzToast.qml",
                "qml/controls/QbzToggle.qml",
                "qml/controls/QbzToolButton.qml",
                "qml/controls/QbzTooltip.qml",
                "qml/controls/QualityBadge.qml",
                "qml/controls/QualityMini.qml",
                "qml/controls/SettingRow.qml",
                "qml/controls/SettingsButton.qml",
                "qml/controls/SettingsDivider.qml",
                "qml/controls/SettingsSpacer.qml",
                // Moved out of views/local/ on 2026-07-31: the album/track
                // CARD badges mount it too, and Slint keeps its counterpart in
                // primitives/ (SourceGlyph.slint).
                "qml/controls/SourceIcon.qml",
                "qml/rows/BlacklistRow.qml",
                "qml/rows/TrackCols.qml",
                "qml/rows/TrackListHeader.qml",
                "qml/rows/TrackRow.qml",
                "qml/settings/AppearanceSettings.qml",
                "qml/settings/IntegrationsSettings.qml",
                "qml/settings/SettingsView.qml",
                "qml/shell/AmbientField.qml",
                "qml/shell/AppShell.qml",
                "qml/shell/ArtPreviewOverlay.qml",
                "qml/shell/Cortinilla.qml",
                "qml/shell/HeaderBar.qml",
                "qml/shell/LyricsPanel.qml",
                "qml/shell/NowPlayingBar.qml",
                "qml/shell/NowPlayingBarSmall.qml",
                "qml/shell/PlayerBar.qml",
                "qml/shell/QueuePanel.qml",
                // Kiosk shell (2026-08-02 kiosk-port contract). The router is
                // SHARED with AppShell (contract D3) and lives here rather
                // than under kiosk/ for that reason.
                "qml/shell/ContentRouter.qml",
                "qml/shell/KioskShell.qml",
                "qml/kiosk/NavRail.qml",
                "qml/kiosk/KioskCard.qml",
                "qml/kiosk/KioskAlbumGrid.qml",
                "qml/kiosk/KioskTrackRow.qml",
                "qml/kiosk/KioskArtistCard.qml",
                "qml/kiosk/KioskSearch.qml",
                "qml/kiosk/KioskNowPlaying.qml",
                "qml/kiosk/KioskDiscover.qml",
                "qml/kiosk/KioskLibrary.qml",
                "qml/kiosk/KioskLocalLibrary.qml",
                "qml/kiosk/KioskMyQBZ.qml",
                "qml/kiosk/KioskArtist.qml",
                "qml/kiosk/KioskAlbum.qml",
                "qml/shell/Sidebar.qml",
                "qml/shell/SidebarFolderFlyout.qml",
                "qml/shell/SidebarRowMenu.qml",
                "qml/shell/SidebarNowPlayingDock.qml",
                "qml/shell/AudioSettingsMenu.qml",
                "qml/shell/ViewModeMenu.qml",
                // Hotkeys (2026-08-03 hotkeys-port contract §4.4/§4.5, block
                // B3): the read-only cheatsheet + the editable customize
                // editor, both self-gated on QbzHotkeys and mounted in
                // AppShell with the global overlays.
                "qml/shell/KeyboardShortcutsModal.qml",
                "qml/shell/CustomizeShortcutsModal.qml",
                // Qobuz Connect (2026-08-01 contract §2): the ONE shared
                // device flyout both bars mount + the diagnostics modal
                // AppShell mounts last.
                "qml/shell/QconnectFlyout.qml",
                "qml/shell/QconnectDevModal.qml",
                // Immersive mode (2026-08-02 immersive-port contract §2) —
                // its own module directory like views/local/ and
                // views/playlistmanager/. B2 shipped the root overlay + the
                // header band; B3 adds the atmosphere underlay, the five
                // FOCUS panels and the song card / track meta / equalizer;
                // B4 adds the SPLIT panels (lyrics split mount lives in
                // ImmersiveView) + the two remaining FOCUS panels; B5 adds
                // the player bar + the search cortinilla.
                "qml/immersive/AlbumReactivePanel.qml",
                "qml/immersive/CoverflowPanel.qml",
                "qml/immersive/EqualizerBars.qml",
                "qml/immersive/ImmersiveAtmosphere.qml",
                "qml/immersive/ImmersiveHeader.qml",
                "qml/immersive/ImmersivePlayerBar.qml",
                "qml/immersive/ImmersiveSearchCortinilla.qml",
                "qml/immersive/ImmersiveSongCard.qml",
                "qml/immersive/ImmersiveTrackMeta.qml",
                "qml/immersive/ImmersiveView.qml",
                "qml/immersive/LyricsFocusPanel.qml",
                "qml/immersive/QueueTabsPanel.qml",
                "qml/immersive/SpectrumPanel.qml",
                "qml/immersive/StaticPanel.qml",
                "qml/immersive/SuggestionsPanel.qml",
                "qml/immersive/TinyBar.qml",
                "qml/immersive/TrackInfoPanel.qml",
                "qml/immersive/VolumeBar.qml",
                "qml/immersive/WaveBedPanel.qml",
                "qml/theme/QbzIcon.qml",
                "qml/theme/QbzScrollBar.qml",
                "qml/theme/QbzSpinner.qml",
                "qml/theme/QbzTheme.qml",
                "qml/theme/RoundedImage.qml",
                "qml/views/AlbumView.qml",
                "qml/views/ArtistReleasesView.qml",
                "qml/views/ArtistView.qml",
                "qml/views/BlacklistManagerView.qml",
                "qml/views/HomeView.qml",
                "qml/views/LibraryView.qml",
                "qml/views/LocalLibraryView.qml",
                "qml/views/PlaylistView.qml",
                "qml/views/SearchView.qml",
                "qml/views/SectionRail.qml",
                "qml/shell/CastPicker.qml",
                "qml/controls/BrowseGenreButton.qml",
                "qml/controls/QbzToggleButton.qml",
                "qml/controls/ViewModeToggle.qml",
                "qml/views/AlbumCollection.qml",
                "qml/views/AlbumListHeader.qml",
                "qml/views/AlbumListRow.qml",
                "qml/views/DiscoverBrowseView.qml",
                "qml/views/LabelReleasesView.qml",
                "qml/views/LabelView.qml",
                "qml/views/MixView.qml",
                "qml/views/PlayHistoryView.qml",
                "qml/views/PlaylistBrowseView.qml",
                "qml/views/PlaylistListRow.qml",
                "qml/controls/DiscoverConfigModal.qml",
                "qml/controls/GenreFilterPopup.qml",
                "qml/shell/NavFlyout.qml",
                "qml/shell/NavSectionGlyph.qml",
                "qml/controls/HeaderGradient.qml",
                "qml/controls/QbzJumpNavBar.qml",
                "qml/shell/LyricsControlsFlyout.qml",
                "qml/shell/LyricsLineRow.qml",
                "qml/shell/LyricsLinesView.qml",
                "qml/shell/LyricsSyncEngine.qml",
                "qml/shell/NavGestureLayer.qml",
                "qml/controls/QbzCheckbox.qml",
                // Library lane (B1-B5): the promoted A-Z strip + the
                // per-surface bodies LibraryView.qml was split into.
                "qml/controls/QbzAlphaStrip.qml",
                "qml/views/library/FeedGridCell.qml",
                "qml/views/library/FeedListRow.qml",
                "qml/views/library/LibraryAlbumsList.qml",
                "qml/views/library/LibraryArtistsPanel.qml",
                "qml/views/library/LibraryToolbar.qml",
                "qml/controls/WarningBanner.qml",
                "qml/settings/AudioSettings.qml",
                "qml/settings/BlacklistSettings.qml",
                "qml/settings/DeveloperSettings.qml",
                "qml/settings/LibraryFolderTable.qml",
                "qml/settings/LocalLibrarySettings.qml",
                "qml/settings/OfflineSettings.qml",
                "qml/settings/PlaybackSettings.qml",
                "qml/settings/PlexSettings.qml",
                "qml/settings/SandboxSettings.qml",
                "qml/controls/QbzSkeleton.qml",
                "qml/shell/FavToggle.qml",
                "qml/shell/InfoCreditCell.qml",
                "qml/shell/InfoMetaCell.qml",
                "qml/shell/SongCard.qml",
                "qml/shell/TrackInfoBody.qml",
                "qml/shell/TrackInfoModal.qml",
                "qml/shell/TransportControls.qml",
                "qml/views/local/LocalIconSelect.qml",
                "qml/views/local/LocalTip.qml",
                "qml/controls/QualityBadgeFull.qml",
                "qml/shell/AudioStamp.qml",
                "qml/shell/SongCardStamp.qml",
                "qml/shell/SpectrumBand.qml",
                "qml/shell/VizSettle.qml",
                "qml/views/LocalAlbumView.qml",
                "qml/views/local/FilterChip.qml",
                "qml/views/local/FolderSubcard.qml",
                "qml/views/local/LocalAlbumCollection.qml",
                "qml/views/local/LocalAlbumHeader.qml",
                "qml/views/local/LocalAlbumRow.qml",
                "qml/views/local/LocalAlbumsTab.qml",
                "qml/views/local/LocalArtistRow.qml",
                "qml/views/local/LocalArtistsTab.qml",
                "qml/views/local/LocalChrome.qml",
                "qml/views/local/LocalEphemeralPane.qml",
                "qml/views/local/LocalFilterPopup.qml",
                "qml/views/local/LocalFolderDetail.qml",
                "qml/views/local/LocalFoldersTab.qml",
                "qml/views/local/LocalNote.qml",
                "qml/views/local/LocalSearchBox.qml",
                "qml/views/local/LocalToolbar.qml",
                "qml/views/local/LocalTrackRow.qml",
                "qml/views/local/LocalTracksTab.qml",
                "qml/views/local/LocalTreeRail.qml",
                "qml/views/local/SelectCheck.qml",
                "qml/views/local/TreeRow.qml",
                "qml/views/local/VersionPicker.qml",
                // MyQBZ (Mixtapes / Collections / Artist-Collection builder).
                // Its own subdirectory, like views/local/, because the five
                // files only ever mount each other.
                "qml/views/myqbz/DiscoBuilderView.qml",
                "qml/views/myqbz/DiscoCandidateRow.qml",
                "qml/views/myqbz/MyQbzCard.qml",
                "qml/views/myqbz/MyQbzDetailRow.qml",
                "qml/views/myqbz/MyQbzDetailView.qml",
                "qml/views/myqbz/MyQbzGridView.qml",
                // Playlist Manager (route "playlistmanager"): the router target
                // plus the TWELVE files of its own module directory. A .qml
                // missing from this array is absent from the qrc and fails its
                // PARENT file at load with "… is not a type" — invisible to
                // cargo check, and it takes the whole view down, not one row.
                "qml/views/PlaylistManagerView.qml",
                "qml/views/playlistmanager/PmActionButton.qml",
                "qml/views/playlistmanager/PmFolderCard.qml",
                "qml/views/playlistmanager/PmFolderChip.qml",
                "qml/views/playlistmanager/PmFolderMenu.qml",
                "qml/views/playlistmanager/PmGridCard.qml",
                "qml/views/playlistmanager/PmListRow.qml",
                "qml/views/playlistmanager/PmLocalBadge.qml",
                "qml/views/playlistmanager/PmMenuRow.qml",
                "qml/views/playlistmanager/PmPageHead.qml",
                "qml/views/playlistmanager/PmToolbar.qml",
                "qml/views/playlistmanager/PmTreeFolderRow.qml",
                "qml/views/playlistmanager/PmTreePlaylistRow.qml",
                // Miniplayer (2026-08-03 miniplayer/tray-port contract §3.4) —
                // its own module directory, like immersive/ and kiosk/. B2
                // ships the window, the card shell and the two DISPLAY
                // surfaces; B3 adds the footer + capsule, B4 the queue and
                // lyrics surfaces.
                "qml/miniplayer/MiniWindow.qml",
                "qml/miniplayer/MiniShell.qml",
                "qml/miniplayer/MiniExplicitBadge.qml",
                "qml/miniplayer/MiniCoverArt.qml",
                "qml/miniplayer/MiniCompactSurface.qml",
                "qml/miniplayer/MiniArtworkSurface.qml",
            ],
            qrc_files: &qrc_refs,
            ..Default::default()
        })
        .build();
}
