// Settings > Local Library — the QML port of crates/qbz-ui/ui/settings/
// LocalLibrarySettings.slint (the folder half; the PLEX half is the sibling
// PlexSettings.qml, mounted at the bottom exactly where the Slint has it).
//
// Group order is the Slint's: LIBRARY FOLDERS (toolbar + filter + table +
// scan progress, in LibraryFolderTable.qml) · ALBUMS VIEW · MAINTENANCE ·
// DANGER ZONE · PLEX (PlexSettings.qml).
//
// Wiring: the folder table + the scan ride the settings document
// (settings_qt/library.rs over `qbz_library`'s own scan engine); the album
// grouping row drives the SAME QbzLocal.setAlbumMode the Local Library
// header dropdown uses, so both stay one setting.
//
// Deltas vs the Slint (deliberate, no dead rows):
// - "Add folder" is a path field + button, not the native folder chooser
//   (no picker seam in this port — settings_qt/library.rs explains).
// - No per-folder EDIT affordance: LibFolderEditModal (alias + network
//   overrides) is not ported, so the pencil would open nothing.
// - "Local items in Library All" is NOT shipped: library_qt.rs documents the
//   "all" scope as not wired, so the selector would persist a preference no
//   consumer reads.
// - The folder table scrolls with the page instead of in a capped 360px
//   inner scroller (a nested Flickable inside the settings Flickable is a
//   scroll trap).

import QtQuick
import com.blitzfc.qbz
import "../controls"
import "../theme"

Column {
    id: root

    property var doc: ({})
    readonly property var lib: doc.library || ({})

    QbzTheme { id: theme }

    spacing: 4

    // Re-read the folder list every time the section is shown — the Slint's
    // `LibraryManageActions.load()` in the panel's init (the panel is mounted
    // by an `if section == 4` there, by visibility here).
    onVisibleChanged: if (visible) QbzBridge.settingsString("refresh", "")

    // ========================= LIBRARY FOLDERS ===========================
    // Toolbar + add-path row + filter + the folder table + scan progress.
    LibraryFolderTable {
        width: parent.width
        lib: root.lib
    }

    Item { width: 1; height: 22 }

    // ============================ ALBUMS VIEW ============================
    GroupHeader { text: QbzSession.tr("ALBUMS VIEW", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Album grouping", QbzSession.trRev)
        description: QbzSession.tr("Folders: one card per album folder — best for compilations and box sets. Metadata: split by album + artist tags.", QbzSession.trRev)
        QbzSelect {
            menuWidth: 210
            options: [
                QbzSession.tr("Albums by folder", QbzSession.trRev),
                QbzSession.tr("Albums by metadata", QbzSession.trRev)
            ]
            // Same setting as the Albums-tab header dropdown (one store).
            currentIndex: QbzLocal.localAlbumMode === "metadata" ? 1 : 0
            onSelected: function (i) { QbzLocal.setAlbumMode(i === 1 ? "metadata" : "folder") }
        }
    }

    Item { width: 1; height: 22 }

    // ============================ MAINTENANCE ============================
    GroupHeader { text: QbzSession.tr("MAINTENANCE", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Cleanup missing files", QbzSession.trRev)
        description: QbzSession.tr("Remove tracks whose files no longer exist on disk.", QbzSession.trRev)
        Column {
            spacing: 4
            SettingsButton {
                text: root.lib.cleaning === true
                    ? QbzSession.tr("Cleaning up...", QbzSession.trRev)
                    : QbzSession.tr("Cleanup", QbzSession.trRev)
                enabled: root.lib.cleaning !== true
                onClicked: QbzBridge.settingsString("library-cleanup-missing", "")
            }
            Text {
                visible: (root.lib.cleanupStatus || "") !== ""
                width: parent.width
                horizontalAlignment: Text.AlignRight
                text: root.lib.cleanupStatus || ""
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
            }
        }
    }

    Item { width: 1; height: 22 }

    // ============================ DANGER ZONE ============================
    GroupHeader { text: QbzSession.tr("DANGER ZONE", QbzSession.trRev) }
    SettingRow {
        label: QbzSession.tr("Clear library database", QbzSession.trRev)
        description: QbzSession.tr("Remove all indexed tracks. Your audio files are not deleted.", QbzSession.trRev)
        SettingsButton {
            danger: true
            text: root.lib.clearing === true
                ? QbzSession.tr("Clearing...", QbzSession.trRev)
                : QbzSession.tr("Clear all", QbzSession.trRev)
            enabled: root.lib.clearing !== true
            onClicked: QbzBridge.settingsString("library-clear", "")
        }
    }

    Item { width: 1; height: 28 }

    // ================================ PLEX ===============================
    PlexSettings {
        width: parent.width
        doc: root.doc
    }
}
