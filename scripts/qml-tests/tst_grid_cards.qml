import QtQuick
import QtQuick.Window
import QtTest
import com.blitzfc.qbz
import "../../crates/qbz-qt/qml/views/library" as Library
Item {
    width: 240; height: 270
    QtObject {
        id: host
        property var artMap: ({})
        property bool showSourceBadges: false
        property string activeTab: "all"
        property bool skelPhase: false
        function askRemoveReleaseFavorites(item) {}
    }
    Library.FeedGridCell { id: cell; view: host; item: ({kind: "group-header", id: "", title: ""}) }
    GridView {
        id: bufferedGrid
        width: 880; height: 266; cellWidth: 220; cellHeight: 266
        cacheBuffer: 2 * cellHeight
        model: 0
        delegate: Library.FeedGridCell {
            required property int index
            view: host
            item: ({kind:"album", id:String(index), artKey:"buffer"+index,
                title:"Buffered", imageUrl:"fixture", artist:"Artist"})
        }
    }
    TestCase {
        name: "GridCards"; when: windowShown
        function test_two_rows_decode_before_scrolling() {
            bufferedGrid.model = 40
            var paths = {}
            for (var i=0;i<12;i++) paths["buffer"+i]=Qt.resolvedUrl("fixtures/red.ppm").toString()
            host.artMap = paths
            function ready(index) {
                var delegate=bufferedGrid.itemAtIndex(index)
                if (!delegate) return false
                for (var i=0;i<delegate.children.length;i++) {
                    var child=delegate.children[i]
                    if (child.item && child.item.artworkReady === true) return true
                }
                return false
            }
            for (var index=4;index<12;index++) {
                tryVerify(function() {return ready(index)}, 3000,
                    "buffered row must be decoded before entering the viewport")
            }
            compare(bufferedGrid.contentY, 0)
            bufferedGrid.contentY = 532
            for (index=8;index<12;index++) verify(ready(index))
            bufferedGrid.model=0
        }
        function test_real_card_handover_data() {
            return ["album", "track", "artist", "label", "playlist"].map(function(kind) {
                return {tag: kind, kind: kind}
            })
        }
        function test_real_card_handover(data) {
            host.artMap = ({})
            cell.item = {kind:data.kind, id:"test", artKey:"cover", title:"Fixture", artist:"Artist",
                artistId:"1", genre:"", year:"2000", qualityTier:"", source:"qobuz",
                imageUrl:"fixture", isFavorite:false, isPinned:false, playlistOwnImage:true}
            function card() {
                for (var i=0;i<cell.children.length;i++) {
                    var child=cell.children[i]
                    if (child.item && child.item.artworkReady !== undefined) return child.item
                }
                return null
            }
            tryVerify(function() { return card() !== null })
            compare(card().artworkReady, false)
            host.artMap = {cover:Qt.resolvedUrl("fixtures/red.ppm").toString()}
            tryVerify(function() { return card().artworkReady })
            compare(card().artworkImmediate, Screen.devicePixelRatio <= 1)
            if (Screen.devicePixelRatio > 1) wait(250)
            waitForRendering(cell, 100)
            var shot=grabImage(cell)
            verify(shot.red(100,100)>240 && shot.green(100,100)<10, "cover must replace grey at center")
            cell.item = {kind:"group-header", id:"", title:""}
        }
    }
}
