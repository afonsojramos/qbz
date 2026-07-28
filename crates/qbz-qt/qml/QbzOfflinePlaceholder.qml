// QbzOfflinePlaceholder — the offline gate (offline/OfflinePlaceholder
// .slint), consolidated in phase 22 from the TWO verbatim copies
// (HomeView + LibraryView). Arms: `induced` (offlineMode === 2) switches
// the body copy; `showSettingsAction` mounts the induced-only "Open
// Settings" button (POC-NOTE: navigates via QbzBridge.navigateTo —
// the Slint opens Settings the same way).

import QtQuick
import com.blitzfc.qbz

Column {
    property bool induced: QbzBridge.offlineMode === 2
    property bool showSettingsAction: false
    signal settingsClicked()

    QbzTheme { id: theme }

    spacing: 0
    QbzIcon {
        name: "cloud-off"
        width: 56
        height: 56
        anchors.horizontalCenter: parent.horizontalCenter
        tintName: "muted"
    }
    Item { width: 1; height: 18 }
    Text {
        anchors.horizontalCenter: parent.horizontalCenter
        text: QbzBridge.tr("You're offline", QbzBridge.trRev)
        color: theme.textPrimary
        font.pixelSize: theme.fontHeading
        font.weight: theme.weightSemibold
    }
    Item { width: 1; height: 8 }
    Text {
        width: 420
        text: induced
            ? QbzBridge.tr("Offline mode is enabled. Disable it in Settings to use Qobuz.", QbzBridge.trRev)
            : QbzBridge.tr("No internet connection. Your local library and downloads keep working.", QbzBridge.trRev)
        color: theme.textSecondary
        font.pixelSize: theme.fontBody
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
    }
    Item { visible: showSettingsAction && induced; width: 1; height: 18 }
    SettingsButton {
        visible: showSettingsAction && induced
        anchors.horizontalCenter: parent.horizontalCenter
        text: QbzBridge.tr("Open Settings", QbzBridge.trRev)
        onClicked: parent.settingsClicked()
    }
}
