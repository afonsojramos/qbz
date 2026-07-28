// Settings > Blacklist — the QML port of crates/qbz-ui/ui/settings/
// ContentFilteringSettings.slint: ONE row (blind-eye glyph + "Blacklist" +
// "{n} artists, {m} albums blocked", with a "(disabled)" suffix when the
// feature is off). There is no enable toggle here in the Slint either — the
// switch lives inside the Blacklist Manager.
//
// The counters are the REAL per-user store (artist_blacklist.db, read through
// the shared BlacklistService — settings_qt/devtools.rs).
//
// Delta vs the Slint: the "Manage" chevron is NOT shipped — this port has no
// Blacklist Manager view for it to open, and a button that opens nothing is
// exactly what this pass exists to remove.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Column {
    id: root

    property var doc: ({})
    readonly property var dev: doc.dev || ({})

    QbzTheme { id: theme }

    spacing: 4

    Item {
        width: parent.width
        height: 64
        Row {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: 12
            QbzIcon {
                anchors.verticalCenter: parent.verticalCenter
                name: "blind-eye"
                width: 18
                height: 18
                tintName: "secondary"
            }
            Column {
                width: parent.width - 30
                spacing: 3
                Text {
                    width: parent.width
                    text: QbzSession.tr("Blacklist", QbzSession.trRev)
                    color: theme.textPrimary
                    font.pixelSize: theme.fontBody
                    font.weight: theme.weightMedium
                }
                Text {
                    width: parent.width
                    text: (root.dev.blacklistEnabled === false
                            ? QbzSession.tr("{} artists, {} albums blocked (disabled)", QbzSession.trRev)
                            : QbzSession.tr("{} artists, {} albums blocked", QbzSession.trRev))
                        .replace("{}", root.dev.blacklistArtists || 0)
                        .replace("{}", root.dev.blacklistAlbums || 0)
                    color: theme.textMuted
                    font.pixelSize: 12
                    wrapMode: Text.WordWrap
                }
            }
        }
    }
}
