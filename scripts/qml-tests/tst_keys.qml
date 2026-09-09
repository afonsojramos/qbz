import QtQuick
import QtTest
Item {
    width: 300; height: 200
    property int lastKey: 0
    Keys.onPressed: function(event) { lastKey = event.key; event.accepted = true }
    TextInput { id: field; width: 150; height: 30 }
    TestCase {
        name: "TextInputPropagation"; when: windowShown
        function test_modified_data() {
            return [{tag: "search", key: Qt.Key_F}, {tag:"link",key:Qt.Key_L}, {tag:"settings",key:Qt.Key_Comma}]
        }
        function test_text_editing_stays_in_field() {
            field.text = "example"; field.forceActiveFocus()
            keyClick(Qt.Key_A, Qt.ControlModifier)
            compare(field.selectedText, "example")
            keyClick(Qt.Key_Backspace)
            compare(field.text, "")
            lastKey = 0
            keyClick(Qt.Key_S, Qt.ShiftModifier)
            compare(field.text.toLowerCase(), "s")
            verify(lastKey !== Qt.Key_S)
        }
        function test_modified(data) {
            for (var i = 0; i < 3; ++i) {
                field.forceActiveFocus(); lastKey = 0
                keyClick(data.key, Qt.ControlModifier)
                compare(lastKey, data.key)
            }
        }
    }
}
