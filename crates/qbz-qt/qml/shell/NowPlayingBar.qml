// NowPlayingBar SHELL — the mode seam (NowPlayingBar.slint, phase 18):
// mounts NowPlayingBarSmall for mode 2 (Small) and the full PlayerBar for
// modes 0 (New) / 1 (Classic) / 3 (Large). AppShell pins the height
// mode-aware (42px Small / 112px otherwise).

import QtQuick
import com.blitzfc.qbz

Item {
    id: root

    Loader {
        anchors.fill: parent
        source: QbzShell.npbMode === 2 ? "NowPlayingBarSmall.qml" : "PlayerBar.qml"
    }
}
