import QtQuick
import QtTest
import QtQuick.Controls
import "../../crates/qbz-qt/qml/controls"
Item {
    id: root
    width: 300; height: 200
    property int clicks: 0
    property string selectedOnClick: ""
    InputFocusDismiss { id: dismissal }
    TextInput { id: first; width: 100; height: 30; text: "first" }
    TextInput { id: second; y: 40; width: 100; height: 30; text: "second" }
    MouseArea { x: 150; width: 100; height: 100; onClicked: { root.clicks++; root.selectedOnClick = first.selectedText } }
    TextEdit { id: multi; x: 150; y: 110; width: 140; height: 75; text: "multi-line" }
    Popup {
        id: editMenu
        parent: Overlay.overlay
        x: 150; y: 110; width: 100; height: 50
        contentItem: MouseArea {
            onClicked: { root.selectedOnClick = first.selectedText; editMenu.close() }
        }
    }
    TestCase {
        name: "InputFocusDismiss"; when: windowShown
        function test_inside() { first.forceActiveFocus(); mouseClick(root, 30, 15); verify(first.activeFocus) }
        function test_other_input() { first.forceActiveFocus(); mouseClick(root, 30, 55); verify(second.activeFocus) }
        function test_outside() { first.forceActiveFocus(); mouseClick(root, 20, 150); verify(root.activeFocus) }
        function test_button_keeps_click() { first.forceActiveFocus(); clicks=0; mouseClick(root, 170, 20); compare(clicks,1); verify(root.activeFocus) }
        function test_text_edit_outside() { multi.forceActiveFocus(); mouseClick(root, 20, 150); verify(root.activeFocus) }
        function test_text_survives_blur() { first.text = "edited"; first.forceActiveFocus(); mouseClick(root, 20, 150); compare(first.text,"edited") }
        function test_selection_available_to_clicked_action() {
            first.text = "copy me"; first.forceActiveFocus(); first.selectAll()
            editMenu.open(); tryCompare(editMenu, "visible", true)
            mouseClick(editMenu.contentItem, 10, 10)
            compare(selectedOnClick, "copy me")
        }
        function test_navigation() { first.forceActiveFocus(); dismissal.dismiss(); verify(root.activeFocus) }
    }
}
