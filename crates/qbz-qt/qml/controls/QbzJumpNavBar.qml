// QbzJumpNavBar — 1:1 port of primitives/JumpNavBar.slint.
//
// The section-jump strip that sits between a page header and its body:
//   [JUMP TO]  tab  tab  tab  …                                  [magnifier]
// The Slint mounts it on the artist page and (without the search arm) on the
// label page, which is why it lives in controls/ rather than inside a view.
//
// Numbers, read off primitives/JumpNavBar.slint:
//   :59  height 44          :60 background = `bar-bg` (host passes the page fill)
//   :65  1px border-subtle hairline pinned to the BOTTOM edge
//   :74  padding-left/right 32   :76 spacing 18 between label / tabs / search
//   :84  "JUMP TO" — legal (13px), semibold, text-muted, letter-spacing 0.5
//   :106 tab spacing 14; :111 32px tall cell, width = text + 4, text x = 2
//   :116 active -> text-primary + semibold, hover -> text-primary,
//        else text-secondary + regular; :128 2px accent underline when active
//   :157 open search field 216px wide, 200ms ease-in-out, surface-card
//   :260 closed toggle 32px, radius 6, hover surface-elevated, 16px glyph
//
// The search affordance is the SHARED controls/QbzLineEdit expandable arm
// (ExpandableSearch.slint), not a second inline input: same interaction —
// closed magnifier slot, opens right-anchored growing LEFT over the tabs,
// X clears and closes, Esc closes. Its closed slot is 34px against the
// .slint's 32px; that 2px is the price of not owning a fourth search box.
//
// Presentation only: sticky positioning and what a tab scrolls to are the
// host's business (the .slint says the same at :12).

import QtQuick
import com.blitzfc.qbz
import "../theme"

Rectangle {
    id: root

    QbzTheme { id: theme }

    /// [{ id, label }] — the sections present on the page.
    property var tabs: []
    property string activeTabId: ""
    /// Page fill, so the bar reads as part of the page (and can go
    /// translucent under the dynamic background, like the .slint's bar-bg).
    property color barBg: theme.surfaceMain
    /// The label page mounts the bar WITHOUT the search affordance.
    property bool showSearch: true
    /// Horizontal inset. 32 = the .slint's own padding for a full-bleed
    /// mount; a host that already pads its column passes 0.
    property real padH: 32

    signal tabClicked(string id)
    signal searchEdited(string text)

    /// Host hook: clear the filter when the field closes itself (Esc / X).
    property alias searchOpen: searchBox.open

    height: 44
    color: root.barBg

    Rectangle {
        x: 0
        y: parent.height - 1
        width: parent.width
        height: 1
        color: theme.borderSubtle
    }

    Text {
        id: jumpLabel
        x: root.padH
        anchors.verticalCenter: parent.verticalCenter
        text: QbzSession.tr("JUMP TO", QbzSession.trRev)
        color: theme.textMuted
        font.pixelSize: theme.fontLegal
        font.weight: theme.weightSemibold
        font.letterSpacing: 0.5
    }

    // Tab strip. Clipped so a long discography's tabs never spill under the
    // search slot (the .slint's `center-region { clip: true }`).
    Item {
        id: centerRegion
        x: jumpLabel.x + jumpLabel.width + 18
        y: 0
        width: Math.max(0, root.width - root.padH
                           - (searchSlot.visible ? searchSlot.width + 18 : 0)
                           - x)
        height: parent.height
        clip: true

        Row {
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            spacing: 14
            Repeater {
                model: root.tabs
                delegate: Item {
                    id: tabCell
                    required property var modelData
                    readonly property bool active: modelData.id === root.activeTabId

                    width: tabText.implicitWidth + 4
                    height: 32

                    Text {
                        id: tabText
                        x: 2
                        height: parent.height
                        verticalAlignment: Text.AlignVCenter
                        text: tabCell.modelData.label
                        color: (tabCell.active || tabArea.containsMouse)
                            ? theme.textPrimary : theme.textSecondary
                        font.pixelSize: theme.fontBody
                        font.weight: tabCell.active ? theme.weightSemibold
                                                    : theme.weightRegular
                    }
                    Rectangle {
                        visible: tabCell.active
                        x: 0
                        y: parent.height - 2
                        width: parent.width
                        height: 2
                        color: theme.accent
                    }
                    MouseArea {
                        id: tabArea
                        anchors.fill: parent
                        // The .slint disables the tabs while the field is
                        // open (they are behind it).
                        enabled: !searchBox.open
                        hoverEnabled: true
                        cursorShape: Qt.PointingHandCursor
                        onClicked: root.tabClicked(tabCell.modelData.id)
                    }
                }
            }
        }
    }

    // Search slot — keeps the CLOSED footprint; the field itself grows left
    // over the tabs, so nothing in the bar reflows when it opens.
    Item {
        id: searchSlot
        visible: root.showSearch
        width: searchBox.width
        height: searchBox.height
        anchors.right: parent.right
        anchors.rightMargin: root.padH
        anchors.verticalCenter: parent.verticalCenter

        QbzLineEdit {
            id: searchBox
            searchMode: true
            expandable: true
            // surface-card overlay fill (JumpNavBar.slint:164), not the
            // toolbar's elevated.
            elevated: false
            openWidth: Math.min(216, Math.max(120, centerRegion.width))
            placeholder: QbzSession.tr("Search this page…", QbzSession.trRev)
            onEdited: function (v) { root.searchEdited(v) }
        }
    }
}
