// Compact album row for the LIST arm of the Albums / Folders-flat / Artists
// surfaces (AlbumCollectionView's list mode, mounted by
// LocalLibraryView.slint:1255 with show-source / show-source-badge on).
//
// 56px: cover, title + artist, year, track count, quality mark, the source
// column and the trailing ⋯ overflow. Multi-select puts the shared checkbox
// in front of the cover, as the collection view does.
//
// The ⋯ menu is AlbumListRow.slint:381 minus its two Qobuz-only entries: the
// favourite heart (local albums are not catalog favourites) and "Block this
// album" (which the Slint itself hides for source local/plex, :435). What is
// left is the five entries Slint always shows — Open album, Play, Play next,
// Play later, Add to queue — and all five are wired. Right-clicking the row
// opens the same menu at the pointer.

import QtQuick
import com.blitzfc.qbz
import "../../controls"
import "../../theme"

Rectangle {
    id: root

    property var item: ({})
    property string artSource: ""
    /// The LocalLibraryView root — the shared skeleton pulse, the per-item
    /// artwork gate and the settle bound all live there (never duplicated
    /// per row, so the rule cannot drift between surfaces).
    property var view: null
    property bool showSource: true
    property bool selectMode: false
    property bool checked: false
    signal opened()
    signal playRequested()
    signal enqueueRequested(string mode)
    /// `modifiers` rides straight off the mouse event: Shift is what turns
    /// a click into a range (controls/SelectionModel.qml).
    signal toggleSelect(int modifiers)

    QbzTheme { id: theme }

    height: 56
    radius: 6
    color: rowArea.containsMouse ? theme.surfaceHover : "transparent"

    MouseArea {
        id: rowArea
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        cursorShape: Qt.PointingHandCursor
        onClicked: function (mouse) {
            if (mouse.button === Qt.RightButton) {
                rowMenu.openAtCursor(rowArea, mouse.x, mouse.y)
                return
            }
            if (root.selectMode) root.toggleSelect(mouse.modifiers)
            else root.opened()
        }
        onDoubleClicked: if (!root.selectMode) root.playRequested()
    }

    CardMenu {
        id: rowMenu
        menuWidth: 196
        entries: [
            { "label": QbzSession.tr("Open album", QbzSession.trRev), "icon": "library-big", "action": "open" },
            { "label": QbzSession.tr("Play", QbzSession.trRev), "icon": "play-fill", "action": "play" },
            { "label": QbzSession.tr("Play next", QbzSession.trRev), "icon": "list-start", "action": "next" },
            { "label": QbzSession.tr("Play later", QbzSession.trRev), "icon": "list-plus", "action": "later" },
            { "label": QbzSession.tr("Add to queue", QbzSession.trRev), "icon": "list-end", "action": "queue" },
        ]
        onPicked: function (a) {
            if (a === "open") root.opened()
            else if (a === "play") root.playRequested()
            else root.enqueueRequested(a)
        }
    }

    Row {
        anchors.fill: parent
        anchors.leftMargin: 10
        anchors.rightMargin: 12
        spacing: 12

        Item {
            visible: root.selectMode
            width: visible ? 16 : 0
            height: parent.height
            SelectCheck {
                anchors.centerIn: parent
                on: root.checked
                onToggled: function (mods) { root.toggleSelect(mods) }
            }
        }
        Rectangle {
            width: 40
            height: 40
            anchors.verticalCenter: parent.verticalCenter
            radius: 4
            color: theme.surfaceElevated
            clip: true
            RoundedImage {
                id: rowArt
                anchors.fill: parent
                source: root.artSource
                radius: 4
            }
            // Per-item: hands over the instant the art is actually ON the
            // canvas (`rowArt.ready`, not "the path landed"); settles out
            // when the album simply has none (local artwork drops such keys).
            QbzSkeleton {
                variant: "art"
                anchors.fill: parent
                blockRadius: 4
                pending: root.view ? root.view.artWanted(root.item.artKey) : false
                coverReady: rowArt.ready
                phase: root.view ? root.view.skelPhase : false
                settleMs: root.view ? root.view.artSettleMs : 0
                settleHold: root.view ? root.view.artPulse : false
            }
        }
        Column {
            width: parent.width - 40 - 70 - 90 - 92 - 32
                - (root.showSource ? 34 : 0)
                - (root.selectMode ? 28 : 0) - 6 * 12
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                text: root.item.title || ""
                color: theme.textPrimary
                font.pixelSize: 14
                font.weight: theme.weightMedium
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: root.item.artist || ""
                color: theme.textMuted
                font.pixelSize: 12
                elide: Text.ElideRight
            }
        }
        Text {
            width: 70
            anchors.verticalCenter: parent.verticalCenter
            text: root.item.year || ""
            color: theme.textMuted
            font.pixelSize: 12
        }
        Text {
            width: 90
            anchors.verticalCenter: parent.verticalCenter
            text: (root.item.trackCount || 0) + " "
                + QbzSession.tr("tracks", QbzSession.trRev)
            color: theme.textMuted
            font.pixelSize: 12
        }
        Item {
            width: 92
            height: parent.height
            QualityMini {
                tier: root.item.qualityTier || ""
                anchors.verticalCenter: parent.verticalCenter
            }
        }
        Item {
            visible: root.showSource
            readonly property var sourceValues: root.item.sources
                && root.item.sources.length > 0
                ? root.item.sources
                : ((root.item.sourceRaw || root.item.source || "") !== ""
                   ? [root.item.sourceRaw || root.item.source] : [])
            width: visible ? Math.max(34, sourceIcons.implicitWidth) : 0
            height: parent.height
            // AlbumListRow.slint:308-322 — through controls/SourceIcon.qml,
            // never QbzIcon: the Plex and Qobuz marks are MULTI-COLOUR and a
            // tint flattens them to a silhouette. This row used to draw an
            // accent-tinted `hard-drive` for Plex (a blue hard drive) and
            // `cloud-download` for an offline copy.
            Row {
                id: sourceIcons
                spacing: 3
                anchors.verticalCenter: parent.verticalCenter
                Repeater {
                    model: sourceIcons.parent.sourceValues
                    delegate: SourceIcon {
                        required property string modelData
                        kind: modelData
                        mono: true
                        glyphSize: 15
                        plexSize: 16
                        qobuzSize: 16
                        localTint: "muted"
                    }
                }
            }
        }
        // Trailing ⋯ overflow (AlbumListRow.slint:360).
        Rectangle {
            width: 32
            height: 32
            radius: 6
            anchors.verticalCenter: parent.verticalCenter
            color: moreArea.containsMouse ? theme.surfaceElevated : "transparent"
            QbzIcon {
                name: "ellipsis"
                width: 18
                height: 18
                anchors.centerIn: parent
                tintName: moreArea.containsMouse ? "textPrimary" : "muted"
            }
            MouseArea {
                id: moreArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: function (mouse) { rowMenu.openAtCursor(moreArea, mouse.x, mouse.y) }
            }
        }
    }
}
