import QtQuick
import QtQuick.Window
import QtTest
import com.blitzfc.qbz
import "../../crates/qbz-qt/qml/theme" as Theme
import "../../crates/qbz-qt/qml/controls" as Controls
Item {
    width: 240; height: 240
    Theme.RoundedImage { id: art; width: 200; height: 200; fadeMs: 0; radius: 8 }
    Controls.QbzSkeleton {
        id: skeleton; width: 200; height: 200; animated: false
        coverReady: art.ready; handoverFadeMs: 0
    }
    Theme.RoundedImage { id: gridArt; width: 200; height: 200; gridArtwork: true; visible: false }
    TestCase {
        name: "ArtworkHandover"; when: windowShown
        function init() { art.source = ""; art.fadeMs = 0; QbzShell.forceCanvasArt = false }
        function test_ready_image_replaces_placeholder_without_fade() {
            art.source = Qt.resolvedUrl("fixtures/red.ppm")
            tryCompare(art, "ready", true)
            tryCompare(art, "revealed", true)
            compare(skeleton.handedOver, true)
            tryCompare(skeleton, "opacity", 0, 100)
            tryCompare(skeleton, "opacity", 0, 100)
            waitForRendering(art, 100)
            var shot = grabImage(art)
            verify(shot.red(100,100) >= 254 && shot.green(100,100) <= 2 && shot.blue(100,100) <= 2)
        }
        function test_recycled_source_draws_new_pixels() {
            art.source = Qt.resolvedUrl("fixtures/red.ppm")
            tryCompare(art, "revealed", true)
            art.source = Qt.resolvedUrl("fixtures/blue.ppm")
            tryCompare(art, "revealed", true)
            tryCompare(skeleton, "opacity", 0, 100)
            waitForRendering(art, 100)
            var shot = grabImage(art)
            verify(shot.blue(100,100) >= 254 && shot.red(100,100) <= 2 && shot.green(100,100) <= 2)
        }
        function test_shared_grid_policy() {
            compare(gridArt.immediateGridArtwork, gridArt._dpr <= 1)
            compare(gridArt.fadeMs, gridArt.immediateGridArtwork ? 0 : 200)
            gridArt.gridArtwork = false
            compare(gridArt.fadeMs, 200)
            gridArt.gridArtwork = true
        }
        function test_existing_fade_remains_opt_in_default() {
            art.fadeMs = 200
            art.source = Qt.resolvedUrl("fixtures/blue.ppm")
            tryCompare(art, "ready", true)
            tryCompare(art, "revealed", true, 1000)
        }
    }
}
