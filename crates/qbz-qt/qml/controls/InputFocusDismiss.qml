import QtQuick
import QtQuick.Window

// Observe presses without taking the target control's mouse/touch grab.
Item {
    id: observer
    anchors.fill: parent
    // Above modal MouseAreas: a passive handler observes without blocking them.
    z: 100000
    property Item focusRoot: parent

    function dismiss() {
        var window = observer.Window.window
        var item = window ? window.activeFocusItem : null
        if (item instanceof TextInput || item instanceof TextEdit)
            observer.focusRoot.forceActiveFocus()
    }

    PointHandler {
        target: null
        acceptedButtons: Qt.LeftButton
        onActiveChanged: {
            if (!active)
                return
            var window = observer.Window.window
            var item = window ? window.activeFocusItem : null
            if (!(item instanceof TextInput || item instanceof TextEdit))
                return
            var localPoint = item.mapFromItem(observer, point.position.x, point.position.y)
            if (!item.contains(localPoint))
                observer.focusRoot.forceActiveFocus()
        }
    }
}
