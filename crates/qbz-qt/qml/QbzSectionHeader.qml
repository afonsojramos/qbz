// QbzSectionHeader — the section/rail header (Slint Carousel header:
// title + [View all] LEFT of the page chevrons), consolidated in phase 22
// from HomeView.RailHeader (+ its ViewAllLink), SectionRail's header and
// SearchView.SectionHeader. Slint has NO shared SectionHeader primitive
// (each carousel inlines one) — this is a POC consolidation of the three
// identical-in-intent copies.
//
// Arms: `title` (fontSection semibold); `showViewAll` + `viewAllLabel` +
// `viewAllAccent` (Search's accent chip vs Home's secondary link) +
// viewAllClicked(); `showChevrons` + `leftEnabled`/`rightEnabled` +
// pageLeft()/pageRight() (the host owns the page-step math).
// Deliberately NOT absorbed: ArtistView.ReleaseSection's header (adds a
// sort dropdown + "See discography" — distinct arms, still stubs) and the
// uppercase 10-11px eyebrow mini-headers (different type scale).

import QtQuick
import com.blitzfc.qbz

Item {
    property string title: ""
    property bool showViewAll: false
    property string viewAllLabel: ""
    property bool viewAllAccent: false
    property bool showChevrons: true
    property bool leftEnabled: false
    property bool rightEnabled: false
    signal viewAllClicked()
    signal pageLeft()
    signal pageRight()

    QbzTheme { id: theme }

    width: parent ? parent.width : 0
    height: 28

    Text {
        anchors.left: parent.left
        anchors.verticalCenter: parent.verticalCenter
        text: parent.title
        color: theme.textPrimary
        font.pixelSize: theme.fontSection
        font.weight: theme.weightSemibold
    }
    Row {
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        spacing: 4
        // "View all" link/chip (accent = Search's, secondary = Home's).
        Rectangle {
            visible: parent.parent.showViewAll
            anchors.verticalCenter: parent.verticalCenter
            width: vaText.implicitWidth + 16
            height: 26
            radius: 4
            color: vaArea.containsMouse ? theme.surfaceHover : "transparent"
            Text {
                id: vaText
                anchors.centerIn: parent
                text: parent.parent.parent.viewAllLabel !== ""
                    ? parent.parent.parent.viewAllLabel
                    : QbzBridge.tr("View all →", QbzBridge.trRev)
                color: parent.parent.parent.viewAllAccent ? theme.accent
                    : vaArea.containsMouse ? theme.textPrimary : theme.textSecondary
                font.pixelSize: 14
                font.weight: theme.weightMedium
            }
            MouseArea {
                id: vaArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: parent.parent.parent.viewAllClicked()
            }
        }
        QbzNavButton {
            visible: parent.parent.showChevrons
            name: "chevron-left"
            btnEnabled: parent.parent.leftEnabled
            onClicked: parent.parent.pageLeft()
        }
        QbzNavButton {
            visible: parent.parent.showChevrons
            name: "chevron-right"
            btnEnabled: parent.parent.rightEnabled
            onClicked: parent.parent.pageRight()
        }
    }
}
