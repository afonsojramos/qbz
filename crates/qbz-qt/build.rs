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

    CxxQtBuilder::new()
        // Qt modules the bridge links against (Qt6 CMake in /usr/lib64/cmake).
        .qt_module("Qml")
        .qt_module("Quick")
        .qt_module("QuickControls2")
        .qml_module(QmlModule {
            uri: "com.blitzfc.qbz",
            rust_files: &["src/bridge.rs", "src/session_bridge.rs", "src/shell_bridge.rs", "src/player_bridge.rs", "src/queue_bridge.rs", "src/home_bridge.rs", "src/viz_bridge.rs"],
            qml_files: &[
                "qml/LoginScreen.qml",
                "qml/Main.qml",
                "qml/cards/AlbumCard.qml",
                "qml/cards/ArtistCard.qml",
                "qml/cards/LabelCard.qml",
                "qml/cards/PlaylistCard.qml",
                "qml/cards/PlaylistCollage.qml",
                "qml/cards/SlimCard.qml",
                "qml/cards/TrackCard.qml",
                "qml/controls/CardMenu.qml",
                "qml/controls/CardOverlayButton.qml",
                "qml/controls/CardOverlayRow.qml",
                "qml/controls/GroupHeader.qml",
                "qml/controls/QbzCircleAction.qml",
                "qml/controls/QbzContextMenu.qml",
                "qml/controls/QbzEmptyState.qml",
                "qml/controls/QbzIconButton.qml",
                "qml/controls/QbzLineEdit.qml",
                "qml/controls/QbzNavButton.qml",
                "qml/controls/QbzOfflinePlaceholder.qml",
                "qml/controls/QbzSectionHeader.qml",
                "qml/controls/QbzSelect.qml",
                "qml/controls/QbzSlider.qml",
                "qml/controls/QbzTabBar.qml",
                "qml/controls/QbzToggle.qml",
                "qml/controls/QualityBadge.qml",
                "qml/controls/QualityMini.qml",
                "qml/controls/SettingRow.qml",
                "qml/controls/SettingsButton.qml",
                "qml/controls/SettingsDivider.qml",
                "qml/controls/SettingsSpacer.qml",
                "qml/rows/TrackRow.qml",
                "qml/settings/AppearanceSettings.qml",
                "qml/settings/IntegrationsSettings.qml",
                "qml/settings/SettingsView.qml",
                "qml/shell/AmbientField.qml",
                "qml/shell/AppShell.qml",
                "qml/shell/Cortinilla.qml",
                "qml/shell/HeaderBar.qml",
                "qml/shell/LyricsPanel.qml",
                "qml/shell/NowPlayingBar.qml",
                "qml/shell/NowPlayingBarSmall.qml",
                "qml/shell/PlayerBar.qml",
                "qml/shell/QueuePanel.qml",
                "qml/shell/Sidebar.qml",
                "qml/shell/SidebarNowPlayingDock.qml",
                "qml/shell/ViewModeMenu.qml",
                "qml/theme/QbzIcon.qml",
                "qml/theme/QbzScrollBar.qml",
                "qml/theme/QbzSpinner.qml",
                "qml/theme/QbzTheme.qml",
                "qml/theme/RoundedImage.qml",
                "qml/views/AlbumView.qml",
                "qml/views/ArtistView.qml",
                "qml/views/HomeView.qml",
                "qml/views/LibraryView.qml",
                "qml/views/PlaylistView.qml",
                "qml/views/SearchView.qml",
                "qml/views/SectionRail.qml",
            ],
            qrc_files: &qrc_refs,
            ..Default::default()
        })
        .build();
}
