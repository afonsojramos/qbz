// QbzEmptyState — the plain centered empty state (title + optional icon /
// body / action), consolidated in phase 22 from HomeView.RecentPlaceholder,
// QueuePanel's empty queue, SearchView's no-results and Cortinilla's
// no-results. Slint has no shared EmptyState either (per-surface inline) —
// this is a POC-only consolidation; arms: `iconName` ("" = none), `title`,
// `body` ("" = hidden), `actionLabel` + `actionClicked()` ("" = no action).

import QtQuick
import com.blitzfc.qbz
import "../theme"

Column {
    property string iconName: ""
    property string title: ""
    property string body: ""
    property string actionLabel: ""
    signal actionClicked()

    QbzTheme { id: theme }

    spacing: 0
    QbzIcon {
        visible: iconName !== ""
        name: iconName
        width: 40
        height: 40
        anchors.horizontalCenter: parent.horizontalCenter
        tintName: "muted"
    }
    Item { visible: iconName !== ""; width: 1; height: 14 }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        text: title
        color: theme.textPrimary
        font.pixelSize: theme.fontSection
        font.weight: theme.weightSemibold
    }
    Item { visible: body !== ""; width: 1; height: 6 }
    Text {
        visible: body !== ""
        anchors.horizontalCenter: parent.horizontalCenter
        text: body
        color: theme.textMuted
        font.pixelSize: 13
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
    }
    Item { visible: actionLabel !== ""; width: 1; height: 14 }
    SettingsButton {
        visible: actionLabel !== ""
        anchors.horizontalCenter: parent.horizontalCenter
        text: actionLabel
        onClicked: parent.actionClicked()
    }
}
