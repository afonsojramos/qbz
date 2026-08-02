// NowPlayingBar SHELL — the mode seam (NowPlayingBar.slint, phase 18):
// mounts NowPlayingBarSmall for mode 2 (Small) and the full PlayerBar for
// modes 0 (New) / 1 (Classic) / 3 (Large). AppShell pins the height
// mode-aware (42px Small / 112px otherwise).

import QtQuick
import com.blitzfc.qbz

Item {
    id: root

    // The shell's shared hover-tooltip overlay (controls/QbzTooltip.qml),
    // fed in by AppShell (same pattern as Sidebar.tooltip). Forwarded into
    // the loaded bar below. Only the FULL bar consumes it — the Qobuz
    // Connect button's "Qobuz Connect: On/Off" bubble (contract §8: the
    // small bar's button has no tooltip in the reference).
    property Item tooltip: null

    Loader {
        id: barLoader
        anchors.fill: parent
        source: QbzShell.npbMode === 2 ? "NowPlayingBarSmall.qml" : "PlayerBar.qml"
    }

    // Same shape as AppShell's viewLoader "kind" Binding: applies the moment
    // the item exists, re-applies on a mode switch, RestoreNone because the
    // target is DESTROYED on unload. The `when` source guard keeps it off
    // NowPlayingBarSmall, which declares no `tooltip` property (a grouped
    // assignment against a missing property would fail the load).
    Binding {
        target: barLoader.item
        property: "tooltip"
        value: root.tooltip
        when: barLoader.item !== null
              && barLoader.source.toString().indexOf("PlayerBar.qml") !== -1
        restoreMode: Binding.RestoreNone
    }
}
