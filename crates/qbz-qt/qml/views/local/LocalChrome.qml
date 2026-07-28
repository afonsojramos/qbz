// Local Library FIXED CHROME (LocalLibraryView.slint:710-1125) — the
// mandatory two-row header this app's list views all share:
//
//   row 1 (46px)  "Local Library" + the settings gear + the Plex re-sync
//                 button (#573, only when Plex is enabled + authenticated)
//   row 2 (44px)  the segmented tab bar with count badges on the left, the
//                 per-tab toolbar floated right
//   then the 1px divider.
//
// Total height 91px — the content area subtracts exactly this.

import QtQuick
import com.blitzfc.qbz
import "../../controls"
import "../../theme"

Item {
    id: root

    property var view: null

    QbzTheme { id: theme }

    height: 91

    // ---- Row 1: title + gear + Plex sync ----
    Item {
        width: parent.width
        height: 46
        Row {
            x: 32
            anchors.verticalCenter: parent.verticalCenter
            spacing: 12
            Text {
                anchors.verticalCenter: parent.verticalCenter
                text: QbzSession.tr("Local Library", QbzSession.trRev)
                color: theme.textPrimary
                font.pixelSize: theme.fontSection
                font.weight: theme.weightBold
            }
            // Folder management, scan and maintenance do NOT live in this
            // view (Slint header note) — the gear routes out.
            QbzNavButton {
                name: "settings-2"
                anchors.verticalCenter: parent.verticalCenter
                onClicked: QbzShell.navigateTo("settings")
            }
            Item {
                visible: QbzLocal.plexAvailable
                width: visible ? 28 : 0
                height: 28
                anchors.verticalCenter: parent.verticalCenter
                QbzNavButton {
                    anchors.fill: parent
                    name: "refresh-cw"
                    btnEnabled: !QbzLocal.plexSyncing
                    onClicked: QbzLocal.syncPlex()
                }
                QbzSpinner {
                    visible: QbzLocal.plexSyncing
                    anchors.centerIn: parent
                    size: 15
                }
            }
        }
    }

    // ---- Row 2: tabs (left) + per-tab toolbar (right) ----
    Item {
        y: 46
        width: parent.width
        height: 44

        QbzTabBar {
            x: 32
            anchors.verticalCenter: parent.verticalCenter
            counts: true
            underline: true
            activeId: root.view.activeTab
            // NOTE the `root.view.` prefix: QbzTabBar has its OWN `counts`
            // property (the badge arm), so an unqualified `counts` here
            // would resolve to that bool, not to the view's document.
            tabs: [
                { "id": "albums", "label": QbzSession.tr("Albums", QbzSession.trRev), "count": root.view.counts.albums || 0 },
                { "id": "artists", "label": QbzSession.tr("Artists", QbzSession.trRev), "count": root.view.counts.artists || 0 },
                { "id": "folders", "label": QbzSession.tr("Folders", QbzSession.trRev), "count": root.view.counts.folders || 0 },
                { "id": "tracks", "label": QbzSession.tr("Tracks", QbzSession.trRev), "count": root.view.counts.tracks || 0 },
            ]
            onSelected: function (id) { root.view.activeTab = id }
        }

        LocalToolbar {
            x: parent.width - width - 32
            anchors.verticalCenter: parent.verticalCenter
            view: root.view
        }
    }

    Rectangle {
        y: 90
        width: parent.width
        height: 1
        color: theme.borderSubtle
    }
}
