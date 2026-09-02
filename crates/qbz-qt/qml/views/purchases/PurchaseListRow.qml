// PurchaseListRow — the LIST arm of the Purchases albums tab. 1:1 port of
// purchases/PurchasesView.slint's `PurchaseListRow` (:305-395).
//
// NOT views/AlbumListRow.qml, for the same three reasons the .slint gives for
// not reusing its own shared row: the columns are different (this row carries a
// PURCHASE-DATE column and no ⋯ menu), the unavailable state has no arm there,
// and AlbumListRow's body click is hardwired to `QbzAlbum.openAlbum` — which
// would open the catalog page instead of the purchase detail.
//
// 60px, Radius.sm, 48x48 art, title over artist, a 110px quality column and a
// 110px purchase-date column, both right-aligned. Unavailable rows dim to .5,
// lose their hover fill, gain a trailing "Unavailable" label and do not open.
//
// NO ZEBRA: the .slint gives this row a transparent resting fill (:314), unlike
// the catalog AlbumListRow's odd-row alpha-4. Adding one here would be a stripe
// pattern this screen never had.

import QtQuick
import com.blitzfc.qbz
import "../../controls"
import "../../theme"

Rectangle {
    id: root

    /// One row of `list_json.albums` (§G.2).
    property var album: ({})
    signal clicked()

    QbzTheme { id: theme }

    readonly property bool avail: root.album.downloadable !== false

    function t(s) { return QbzSession.tr(s, QbzSession.trRev) }

    /// Epoch -> the viewer's short date. "" for a row that carries no stamp,
    /// so the column collapses to blank rather than to 1970.
    function purchaseDate() {
        var ts = root.album.purchasedAt || 0
        if (ts <= 0)
            return ""
        // Defensive: the seam says seconds, but a producer that ever hands
        // milliseconds through would otherwise print a date in the year 55000.
        var ms = ts > 100000000000 ? ts : ts * 1000
        return new Date(ms).toLocaleDateString(Qt.locale(), Locale.ShortFormat)
    }

    width: parent ? parent.width : 0
    height: 60
    radius: theme.radiusSm
    color: (rowArea.containsMouse && root.avail) ? theme.surfaceHover : "transparent"
    opacity: root.avail ? 1.0 : 0.5

    MouseArea {
        id: rowArea
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: root.avail ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: if (root.avail) root.clicked()
    }

    Row {
        anchors.fill: parent
        anchors.leftMargin: 8
        anchors.rightMargin: 12
        spacing: 12

        // 48x48 art.
        Item {
            width: 48
            height: parent.height
            Rectangle {
                width: 48
                height: 48
                anchors.verticalCenter: parent.verticalCenter
                radius: theme.radiusSm
                color: theme.surfaceElevated
                RoundedImage {
                    id: art
                    anchors.fill: parent
                    // `artPath`, never `artworkUrl` — see PurchaseGridCard.
                    source: root.album.artPath || ""
                    radius: theme.radiusSm
                }
                QbzIcon {
                    visible: !art.ready
                    name: "disc-3"
                    width: 20
                    height: 20
                    anchors.centerIn: parent
                    tintName: "muted"
                }
            }
        }

        // Title over artist — the stretch column.
        Column {
            width: Math.max(0, parent.width - 48 - 110 - 110
                - (unavailLabel.visible ? unavailLabel.width + 12 : 0)
                - 3 * 12)
            anchors.verticalCenter: parent.verticalCenter
            spacing: 2
            Text {
                width: parent.width
                text: root.album.title || ""
                color: theme.textPrimary
                font.pixelSize: theme.fontBody
                font.weight: theme.weightMedium
                elide: Text.ElideRight
            }
            Text {
                width: parent.width
                text: root.album.artist || ""
                color: theme.textMuted
                font.pixelSize: theme.fontLegal
                elide: Text.ElideRight
            }
        }

        // Quality column — the SAME badge the album list rows show
        // (views/AlbumListRow.qml): tier label over the exact quality line.
        Item {
            width: 110
            height: parent.height
            QualityBadgeFull {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                tier: root.album.qualityTier || ""
                detail: root.album.qualityDetail || ""
            }
        }

        // Purchase-date column.
        Text {
            width: 110
            height: parent.height
            text: root.purchaseDate()
            color: theme.textMuted
            font.pixelSize: theme.fontLegal
            horizontalAlignment: Text.AlignRight
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }

        // Unavailable label — an EXTRA column that exists only on those rows
        // (.slint:384-391), which is why the stretch column above subtracts it
        // conditionally.
        Text {
            id: unavailLabel
            visible: !root.avail
            height: parent.height
            text: root.t("Unavailable")
            color: theme.textMuted
            font.pixelSize: theme.fontLegal
            verticalAlignment: Text.AlignVCenter
        }
    }
}
