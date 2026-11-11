import QtQuick
import QtTest
import "../../crates/qbz-qt/qml/controls" as Controls
Item {
    width: 800
    height: 600
    Controls.QbzConfirmModal {
        id: modal
        anchors.fill: parent
        title: "Close QBZ?"
        confirmLabel: "Yes"
        cancelLabel: "No"
        checkboxLabel: "Don't show again"
        danger: false
    }
    SignalSpy { id: confirmed; target: modal; signalName: "confirmed" }
    SignalSpy { id: cancelled; target: modal; signalName: "cancelled" }
    TestCase {
        name: "QuitConfirmation"
        when: windowShown
        function init() {
            confirmed.clear(); cancelled.clear()
            modal.checkboxChecked = false
            modal.open()
            wait(30)
        }
        function cleanup() { modal.close() }
        function test_escape_cancels_without_confirming() {
            keyClick(Qt.Key_Escape)
            compare(cancelled.count, 1)
            compare(confirmed.count, 0)
            verify(!modal.opened)
        }
        function test_initial_enter_is_no() {
            keyClick(Qt.Key_Return)
            compare(cancelled.count, 1)
            compare(confirmed.count, 0)
        }
        function test_keyboard_yes_preserves_checkbox_for_handler() {
            modal.checkboxChecked = true
            keyClick(Qt.Key_Tab)
            keyClick(Qt.Key_Return)
            compare(confirmed.count, 1)
            compare(cancelled.count, 0)
            verify(modal.checkboxChecked)
            verify(!modal.opened)
        }
    }
}
