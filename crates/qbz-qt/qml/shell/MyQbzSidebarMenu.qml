// MyQbzSidebarMenu — the row menu for a collapsible-sidebar My QBZ item
// (a mixtape or a collection). A deliberately MINIMAL sibling of
// SidebarRowMenu: one action, "Hide from sidebar", reusing the SAME translated
// label and the SAME per-element hidden-flag mechanism playlists carry, without
// threading a collection mode through the playlist menu's nine arms.
//
// The row's TEXT already navigates (openCard) and the chevron already
// collapses; this menu is the row's right-click affordance. Unhiding lives on
// the MyQBZ grid card, the way a hidden playlist is unhidden from the Playlist
// Manager rather than from the sidebar.
import QtQuick
import QtQuick.Controls
import com.blitzfc.qbz
import "../controls"
import "../theme"

QbzContextMenu {
    id: root

    menuWidth: 210
    leftPadding: 0
    rightPadding: 0
    topPadding: 5
    bottomPadding: 5

    /// The clicked item's id (a v4 UUID). Set on every open.
    property string itemId: ""

    /// Open row-anchored, matching SidebarRowMenu's placement.
    function openForItem(id, anchorItem, localX, localY) {
        root.itemId = String(id)
        root.openAtCursor(anchorItem, localX, localY)
    }

    // A QML file has no cross-file lexical scope, so QbzContextMenu's own
    // `theme` is not reachable here — declare a local one (SidebarRowMenu:137).
    QbzTheme { id: theme }

    Rectangle {
        id: hideRow
        width: parent ? parent.width : 0
        height: 33
        color: hideArea.containsMouse ? theme.surfaceHover : "transparent"

        Row {
            anchors.fill: parent
            anchors.leftMargin: 11
            anchors.rightMargin: 16
            spacing: 10
            QbzIcon {
                name: "eye-off"
                width: 15
                height: 15
                anchors.verticalCenter: parent.verticalCenter
                tintName: hideArea.containsMouse ? "textPrimary" : "secondary"
            }
            Text {
                width: parent.width - 25
                height: parent.height
                text: QbzSession.tr("Hide from sidebar", QbzSession.trRev)
                color: hideArea.containsMouse ? theme.textPrimary : theme.textSecondary
                font.pixelSize: 13
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }
        MouseArea {
            id: hideArea
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: {
                root.close()
                QbzShell.myqbzSetHidden(root.itemId, true)
            }
        }
    }
}
