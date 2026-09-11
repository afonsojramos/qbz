pragma Singleton
import QtQuick
QtObject {
    function artworkImmediateEnabled() { return true }
    property string mediaStatus: "{}"
    property int pairingCancels: 0
    function mediaCancelQuickConnect() { pairingCancels++ }
    function mediaQuickConnect(url) {}
}
