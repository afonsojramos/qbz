import QtQuick
import QtTest
import "../../crates/qbz-qt/qml/theme" as Theme
import "../../crates/qbz-qt/qml/controls" as Controls
import com.blitzfc.qbz

Item {
    width: 500; height: 500
    Flickable {
        id: view
        anchors.fill: parent
        contentHeight: 100000
        contentWidth: width
        boundsBehavior: Flickable.StopAtBounds
        Flickable {
            id: nested
            x: 50; y: view.contentY + 50; width: 200; height: 100
            contentHeight: 1000; contentWidth: width; visible: false
            Theme.QbzKineticScroll { id: nestedScroll; target: nested }
        }
        ListView {
            id: list
            x: 50; y: view.contentY + 50; width: 200; height: 100
            model: 1000; visible: false
            delegate: Item { width: 200; height: 30 }
            Theme.QbzKineticScroll { id: listScroll; target: list }
        }
        GridView {
            id: grid
            x: 50; y: view.contentY + 50; width: 200; height: 100
            cellWidth: 100; cellHeight: 50; model: 1000; visible: false
            delegate: Item { width: 100; height: 50 }
            Theme.QbzKineticScroll { id: gridScroll; target: grid }
        }
    }
    Theme.QbzKineticScroll { id: scroll; target: view }
    Theme.QbzScrollBar { id: bar; target: gutterView; x: 450; height: 400 }
    Flickable { id: gutterView; x: 420; width: 20; height: 400; contentHeight: 10000 }
    Controls.ScrollMemory { id: memory; target: view; scope: "wheel-test" }
    TestCase {
        name: "WheelPhysics"; when: windowShown
        readonly property real step: Qt.styleHints.wheelScrollLines * 24
        function init() {
            memory._disarm("test reset"); scroll.stopWheel(); view.cancelFlick()
            view.visible = true; view.interactive = true; view.contentY = 10000
            nested.visible = false; list.visible = false; grid.visible = false
            wait(50)
        }
        function turn(target, delta, x, y) { mouseWheel(target,x || 20,y || 20,0,delta) }
        function test_distance_data() {
            return [{tag:"single",count:1,spacing:40}, {tag:"burst",count:10,spacing:20},
                    {tag:"slow",count:10,spacing:100}, {tag:"fractional",count:10,spacing:20,delta:-12}]
        }
        function test_distance(data) {
            const delta = data.delta || -120
            for (let i=0;i<data.count;++i) { turn(view,delta); wait(data.spacing) }
            tryCompare(scroll,"wheelScrolling",false,400)
            fuzzyCompare(view.contentY,10000 + data.count * -delta / 120 * step,0.5)
            const settled = view.contentY; wait(200); compare(view.contentY,settled)
        }
        function test_reverse() {
            for (let i=0;i<6;++i) { turn(view,-120); wait(10) }
            const reversed = view.contentY
            turn(view,120)
            tryCompare(scroll,"wheelScrolling",false,400)
            fuzzyCompare(view.contentY,reversed-step,0.5)
        }
        function test_external_position() {
            turn(view,-120); wait(30); view.contentY = 2500
            wait(250); compare(view.contentY,2500); verify(!scroll.wheelScrolling)
        }
        function test_hidden() {
            turn(view,-120); wait(30); view.visible = false
            const at = view.contentY; wait(250); compare(view.contentY,at)
        }
        function test_bounds() {
            view.contentY = 0; turn(view,120); wait(200); compare(view.contentY,0)
            view.contentY = view.contentHeight-view.height-10
            turn(view,-120); tryCompare(scroll,"wheelScrolling",false,400)
            compare(view.contentY,view.contentHeight-view.height)
        }
        function test_nested_data() {
            return [{tag:"flickable",item:nested,handler:nestedScroll},
                    {tag:"list",item:list,handler:listScroll}, {tag:"grid",item:grid,handler:gridScroll}]
        }
        function test_nested(data) {
            data.item.visible=true; data.item.contentY=0; wait(50)
            turn(data.item,-120)
            tryCompare(data.handler,"wheelScrolling",false,400)
            fuzzyCompare(data.item.contentY-data.item.originY,step,0.5)
            compare(view.contentY,10000)
        }
        function test_nested_boundary_chains() {
            nested.visible=true; nested.contentY=nested.contentHeight-nested.height; wait(50)
            turn(nested,-120)
            tryCompare(scroll,"wheelScrolling",false,400)
            fuzzyCompare(view.contentY,10000+step,0.5)
            compare(nested.contentY,nested.contentHeight-nested.height)
        }
        function test_horizontal_passthrough() {
            mouseWheel(view,20,20,-120,0); wait(200)
            compare(view.contentY,10000); verify(!scroll.wheelScrolling)
        }
        function test_restore_yields() {
            QbzShell.scrollRestore=10000; QbzShell.restoreScope="wheel-test"
            verify(memory._armed)
            turn(view,-120)
            verify(!memory._armed)
            tryCompare(scroll,"wheelScrolling",false,400)
            fuzzyCompare(view.contentY,10000+step,0.5)
        }
        function test_scrollbar_takeover() {
            gutterView.contentY=1000
            turn(gutterView,-120); wait(30)
            mousePress(bar,7,200)
            const at=gutterView.contentY; wait(250); compare(gutterView.contentY,at)
            mouseRelease(bar,7,200)
        }
    }
}
