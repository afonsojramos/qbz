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
            // Both halves of this pill take the SAME decision now. They did
            // not before: the glyph said "primary" (the legacy alias of a
            // literal #ffffff) while the label below said accent-text, so on
            // every pale-accent theme the pill drew a black label next to a
            // white glyph. discover/BrowseHeaderTools.slint:123 and :132 are
            // at least self-consistent — they are BOTH #ffffff — but that
            // white measures 1.70:1 on high-contrast, 1.74 on ikari and
            // 1.82 on wcag-dark, under 2.6:1 on 16 of the 35 palettes, so
            // the port diverges from both lines at once, by measurement.
            // See theme/QbzTheme.qml, "ON AN ACCENT FILL".
            tintName: root.active ? theme.accentGlyphTint : "secondary"
        }
        Text {
            text: root.count === 0
                ? QbzSession.tr("Filter by genre", QbzSession.trRev)
                : root.count === 1
                    ? QbzSession.tr("1 genre", QbzSession.trRev)
                    : QbzSession.tr("{} genres", QbzSession.trRev).replace("{}", root.count)
            // The colour twin of the glyph's tint (see above) — accent-text
            // on 34 of the 35 palettes, so this is a no-op everywhere except
            // where accent-text itself falls under 3:1.
            color: root.active ? theme.accentGlyphColor : theme.textSecondary
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
