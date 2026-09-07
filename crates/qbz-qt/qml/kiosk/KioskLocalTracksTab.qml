// Local Library > Tracks, kiosk body — the second surface the release audit
// reported dead, and for the same reason as Artists: it read the legacy
// `localTracksJson`, which stops being republished once the paged
// `QbzLocalTracks` surface is active (the production default since the native
// model shipped).
//
// It issues NO query of its own. `QbzLocal.loadTab("tracks")` — which the host
// already calls on mount and on every tab switch — is the whole activation,
// and it reuses the user's PERSISTED funnel, sort and grouping.
//
// That is deliberate and load-bearing: `tracksSetFilterJson` is the desktop
// tab's entry point, and it WRITES the funnel through to `ui_prefs`. A kiosk
// that called it with a neutral descriptor would silently erase the Full UI's
// saved Tracks filter — a Full UI regression caused from a kiosk-only file.
//
// Everything else is KioskLocalTrackList.

import QtQuick
import com.blitzfc.qbz
import "../theme"

Item {
    id: root

    property var view: null

    readonly property string nativeError: root.nativeActive ? QbzLocal.localTracksNativeError : ""
    readonly property bool nativeActive: QbzLocal.localTracksNativeActive
    readonly property var nativeModel: QbzLocalTracks
    readonly property int trackTotal: root.nativeActive
        ? (QbzLocal.localTracksNativeTotal || 0)
        : (root.view ? root.view.tracks.length : 0)

    function t(s) { return QbzSession.tr(s, QbzSession.trRev) }

    // A track list is one column; the focus ring walks the model's rows.
    function publishNav() {
        if (root.view)
            root.view.publishNav(1, root.trackTotal)
    }
    onTrackTotalChanged: root.publishNav()
    Component.onCompleted: root.publishNav()

    // Only the mounted tab answers the model's page misses (the model does not
    // even emit them while the signal has no receiver).
    Connections {
        target: root.nativeModel
        function onPageMiss(page, generation) {
            QbzLocal.tracksNativePageMiss(page, generation)
        }
    }
    Connections {
        target: QbzLocal
        function onLocalArtworkReady(key, path) {
            root.nativeModel.setArtwork(key, path)
        }
    }

    KioskLocalTrackList {
        id: list
        anchors.fill: parent
        visible: !QbzLocal.localTracksLoading
            && root.nativeError === ""
            && root.trackTotal > 0
        view: root.view
        surface: "tracks"
        scrollScope: "local:tracks"
        nativeActive: root.nativeActive
        nativeModel: root.nativeModel
        rows: root.nativeActive ? [] : (root.view ? root.view.tracks : [])
    }

    KioskSkeleton {
        anchors.fill: parent
        kind: "list"
        rowHeight: 64
        rowArtSize: 46
        pad: root.view ? root.view.pad : 16
        loading: QbzLocal.localTracksLoading
        error: root.nativeError
        empty: !QbzLocal.localTracksLoading
            && root.nativeError === ""
            && root.trackTotal === 0
        emptyText: root.t("No tracks in your local library yet.")
    }
}
