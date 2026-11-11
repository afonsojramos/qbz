// Shared scrolling for the house scrollbar. Mouse-wheel steps accumulate a
// destination and finish with a short easing; touchpad pixel gestures keep
// their native handling and existing missing-momentum fallback.
// Animation runs only while consuming wheel input, never while idle.

import QtQuick

Item {
    id: root

    objectName: "qbzWheelScroll"
    required property Flickable target
    /// User-originated wheel/touchpad takeover. Consumers with an opt-in
    /// auto-follow mode use this to yield immediately; programmatic
    /// `positionViewAtIndex` calls never emit it.
    signal userScrollStarted()

    // QbzScrollBar declares us beside the view. Reparent to the Flickable
    // explicitly so this observer covers its whole VIEWPORT, including child
    // rails and cards, rather than the scrollbar's 14px gutter.
    parent: target
    anchors.fill: parent
    z: 2147483646

    property real _destination: 0
    property real _position: 0
    property int _direction: 0
    property bool _writing: false
    readonly property bool wheelScrolling: wheelAnimation.running

    function stopWheel() {
        wheelAnimation.stop();
        root._direction = 0;
    }

    // Let nested vertical views consume their own wheel. Horizontal rails keep
    // tilt/shift gestures; an ordinary vertical wheel still scrolls the page.
    function _nestedVerticalAt(x, y, angleY) {
        let item = root.target.contentItem;
        let point = root.mapToItem(item, x, y);
        while (item) {
            const child = item.childAt(point.x, point.y);
            if (!child)
                return false;
            const flickable = child as Flickable;
            if (flickable && flickable.interactive) {
                const minimum = flickable.originY - flickable.topMargin;
                const maximum = flickable.originY + flickable.contentHeight
                        - flickable.height + flickable.bottomMargin;
                if (angleY > 0 ? flickable.contentY > minimum : flickable.contentY < maximum)
                    return true;
            }
            point = item.mapToItem(child, point.x, point.y);
            item = child;
        }
        return false;
    }

    function _scrollWheel(delta) {
        const direction = delta > 0 ? 1 : -1;
        const base = wheelAnimation.running && root._direction === direction
                ? root._destination : root.target.contentY;
        const minimum = root.target.originY - root.target.topMargin;
        const maximum = Math.max(minimum, root.target.originY + root.target.contentHeight
                - root.target.height + root.target.bottomMargin);
        const destination = Math.max(minimum, Math.min(maximum, base + delta));
        root.stopWheel();
        root.target.cancelFlick();
        if (Math.abs(destination - root.target.contentY) < 0.01)
            return;
        root._position = root.target.contentY;
        root._direction = direction;
        root._destination = destination;
        wheelAnimation.from = root._position;
        wheelAnimation.to = destination;
        wheelAnimation.start();
    }

    NumberAnimation {
        id: wheelAnimation
        target: root
        property: "_position"
        duration: 150
        easing.type: Easing.OutCubic
    }
    on_PositionChanged: {
        if (!wheelAnimation.running)
            return;
        root._writing = true;
        root.target.contentY = root._position;
        root._writing = false;
    }
    Connections {
        target: root.target
        function onContentYChanged() { if (!root._writing) root.stopWheel(); }
        function onContentHeightChanged() {
            // Virtualized delegates refine contentHeight during a scroll.
            // Keep the destination unless it has actually left the new bounds.
            if (root.wheelScrolling && root._destination > root.target.originY
                    + root.target.contentHeight - root.target.height + root.target.bottomMargin) {
                root.stopWheel();
                root.target.returnToBounds();
            }
        }
        function onOriginYChanged() { root.stopWheel(); }
        function onHeightChanged() { root.stopWheel(); }
        function onVisibleChanged() { if (!root.target.visible) root.stopWheel(); }
        function onInteractiveChanged() { if (!root.target.interactive) root.stopWheel(); }
        function onDraggingChanged() { if (root.target.dragging) root.stopWheel(); }
    }

    property real _sumX: 0
    property real _sumY: 0
    property real _velocityY: 0
    property double _lastMs: 0
    property bool _vertical: false
    property bool _horizontal: false
    property bool _nativeMomentum: false
    property int _generation: 0

    function _resetGesture() {
        root._sumX = 0;
        root._sumY = 0;
        root._velocityY = 0;
        root._lastMs = Date.now();
        root._vertical = false;
        root._horizontal = false;
        root._nativeMomentum = false;
        ++root._generation;
    }

    function _sample(dx, dy) {
        const now = Date.now();
        // A suspended event loop must not turn one stale delta into an absurd
        // velocity; 1..80ms covers the useful gesture sample window.
        const dt = Math.max(1, Math.min(80, now - root._lastMs));
        root._lastMs = now;
        root._sumX += dx;
        root._sumY += dy;

        // Match Qt's own nested-Flickable decision: one axis must beat the
        // other 2:1 before it owns the gesture. A horizontal carousel therefore
        // keeps horizontal/diagonal swipes and can never launch the page tail.
        if (!root._vertical && !root._horizontal) {
            if (Math.abs(root._sumY) > Math.abs(root._sumX) * 2)
                root._vertical = true;
            else if (Math.abs(root._sumX) > Math.abs(root._sumY) * 2)
                root._horizontal = true;
        }

        if (root._vertical) {
            const instant = dy * 1000 / dt;
            // Recent samples matter most at finger lift, without letting one
            // noisy event replace the entire gesture estimate.
            root._velocityY = root._velocityY === 0 ? instant : root._velocityY * 0.35 + instant * 0.65;
        }
    }

    function _launchTail() {
        let velocity = root._velocityY;
        const serial = root._generation;
        if (!root._vertical || root._nativeMomentum || Math.abs(root._sumY) < 12 || Math.abs(velocity) < 220)
            return;

        // Respect each view's platform-tuned ceiling. `-1` means unlimited;
        // use the platform's usual 2500 px/s rather than allowing a noisy
        // high-resolution sample to fling tens of screens.
        const cap = root.target.maximumFlickVelocity > 0 ? root.target.maximumFlickVelocity : 2500;
        velocity = Math.max(-cap, Math.min(cap, velocity));

        // WheelHandler observes before Flickable finishes ScrollEnd. Launch on
        // the next event-loop turn or that cleanup would cancel the new tail.
        Qt.callLater(function () {
            if (serial !== root._generation || !root.target || !root.target.visible || !root.target.interactive)
                return;
            root.target.flick(0, velocity);
        });
    }

    WheelHandler {
        target: null
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        blocking: false
        enabled: root.target.interactive

        onWheel: function (event) {
            // Pixel gestures and horizontal input remain with native Qt.
            event.accepted = false;

            if (event.phase === Qt.NoScrollPhase && event.pixelDelta.x === 0
                    && event.pixelDelta.y === 0 && event.angleDelta.y !== 0
                    && Math.abs(event.angleDelta.y) > Math.abs(event.angleDelta.x)
                    && !(event.modifiers & (Qt.ShiftModifier | Qt.ControlModifier | Qt.MetaModifier))
                    && !root._nestedVerticalAt(event.x, event.y, event.angleDelta.y)) {
                root.userScrollStarted();
                // Honor system scroll lines and high-resolution fractional
                // notches. Natural-scroll direction is already in Qt's delta.
                root._scrollWheel(-event.angleDelta.y / 120 * Qt.styleHints.wheelScrollLines * 24);
                event.accepted = true;
                return;
            }
            root.stopWheel();

            if (event.phase === Qt.ScrollBegin) {
                root.userScrollStarted();
                root._resetGesture();
                return;
            }
            if (event.phase === Qt.ScrollMomentum) {
                // macOS and some compositors already provide the kinetic tail.
                // Never stack ours on top of theirs.
                root._nativeMomentum = true;
                return;
            }
            if (event.phase === Qt.ScrollEnd) {
                root._launchTail();
                return;
            }
            if (event.phase !== Qt.ScrollUpdate)
            {
                // Physical mouse wheels usually carry NoScrollPhase rather
                // than Begin/Update/End. They still represent an explicit
                // user takeover and must cancel a consumer's auto-follow.
                if (event.angleDelta.x !== 0 || event.angleDelta.y !== 0
                        || event.pixelDelta.x !== 0 || event.pixelDelta.y !== 0)
                    root.userScrollStarted();
                return;
            }

            // Only high-resolution pixel gestures need the touchpad sampler.
            root.userScrollStarted();
            const dx = event.pixelDelta.x;
            const dy = event.pixelDelta.y;
            if (dx !== 0 || dy !== 0)
                root._sample(dx, dy);
        }
    }
}
