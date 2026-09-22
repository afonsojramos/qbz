import QtQuick
import QtTest
import "../../crates/qbz-qt/qml/controls"

Item {
    id: root
    width: 300; height: 200
    QbzLineEdit { id: field }
    TestCase {
        name: "DeferredFieldFocus"; when: windowShown
        function init() {
            field.visible = true; field.enabled = true
            field.expandable = false; field.open = false
            root.forceActiveFocus()
        }
        function test_visible_field_focuses_repeatedly() {
            for (var i = 0; i < 3; ++i) {
                root.forceActiveFocus(); field.focusField()
                tryCompare(field, "fieldActive", true)
            }
        }
        function test_closed_dialog_cannot_steal_search_focus() {
            field.focusField(); field.visible = false
            root.forceActiveFocus(); wait(80)
            verify(root.activeFocus)
        }
        function test_closed_search_cannot_regain_focus() {
            field.expandable = true; field.open = true; field.closeSearch()
            root.forceActiveFocus(); wait(80)
            verify(root.activeFocus)
        }
    }
}
