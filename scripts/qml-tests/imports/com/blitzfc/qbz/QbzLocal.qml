pragma Singleton
import QtQuick
QtObject {
    property string mediaStatus: "{}"
    property int pairingCancels: 0
    function mediaCancelQuickConnect() { pairingCancels++ }
    function mediaQuickConnect(url) {}
}
