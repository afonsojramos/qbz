// BrowseGenreButton — the "Filter by genre" trigger
// (discover/BrowseHeaderTools.slint:102-145): accent fill + "N genres" while
// the SHARED selection is non-empty, else a surface-elevated pill reading
// "Filter by genre".
//
// Extracted from HomeView.qml's inline copy so the Home toolbar and the two
// browse pages draw the identical control. The one difference the Slint keeps
// is the height: HomeView.slint:85 is 32px, BrowseHeaderTools.slint:108 is
// 34px (the ui-control-alignment-standard toolbar size) — hence `btnHeight`.
//
// THE BADGE READS THE BRIDGE, NOT THE POPUP. `GenreFilterPopup` is declared
// LAST in every host (declaration order is z-order) and a creation-time
// binding that dereferences a not-yet-created id registers NO dependency, so
// it would never re-evaluate. The count is parsed straight off
// `QbzBridge.genreFilterJson` here, per context.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    /// "discover" | "library-all" — whose selection this button reports.
    property string context: "discover"
    property int btnHeight: 34
    signal clicked()

    QbzTheme { id: theme }

    readonly property var genreDoc: {
        try {
            return JSON.parse(QbzBridge.genreFilterJson)
        } catch (e) {
            return {}
        }
    }
    readonly property int count: (genreDoc.counts || {})[root.context] || 0
    readonly property bool active: root.count > 0

    width: genreRow.width
    height: btnHeight
    radius: 6
    color: root.active ? theme.accent
         : genreArea.containsMouse ? theme.surfaceHover : theme.surfaceElevated

    Row {
        id: genreRow
        height: parent.height
        leftPadding: 12
        rightPadding: 14
        spacing: 7
        QbzIcon {
            name: "list-filter"
            width: 14
            height: 14
            anchors.verticalCenter: parent.verticalCenter
            tintName: root.active ? "primary" : "secondary"
        }
        Text {
            text: root.count === 0
                ? QbzSession.tr("Filter by genre", QbzSession.trRev)
                : root.count === 1
                    ? QbzSession.tr("1 genre", QbzSession.trRev)
                    : QbzSession.tr("{} genres", QbzSession.trRev).replace("{}", root.count)
            color: root.active ? theme.accentText : theme.textSecondary
            font.pixelSize: 13
            font.weight: root.active ? theme.weightMedium : theme.weightRegular
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    MouseArea {
        id: genreArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: root.clicked()
    }
}
