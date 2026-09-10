pragma Singleton
import QtQuick
QtObject {
 signal artScaledReady(string path, string scaled, int w, int h)
 function artScaledCached(path, w, h) { return "" }
 function artScaled(path, w, h) {}
 property int trRev: 0; function tr(text, rev) { return text } }
