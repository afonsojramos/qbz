// Run the actual artwork batching callbacks; no account, disk cache or audio.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source = fs.readFileSync(new URL('../crates/qbz-qt/qml/views/LocalLibraryView.qml', import.meta.url), 'utf8');
function block(marker, from = 0) {
    const pos = source.indexOf(marker, from);
    assert.ok(pos >= 0, marker);
    const begin = source.indexOf('{', pos);
    let depth = 1;
    for (let end = begin + 1; end < source.length; end++) {
        if (source[end] === '{') depth++;
        if (source[end] === '}' && --depth === 0) return "(function () {" + source.slice(begin + 1, end) + "})()";
    }
    throw new Error(`unclosed ${marker}`);
}
let ticks = 0;
let pulses = 0;
let models = 0;
const model = {setArtwork() { models++; }};
const root = {
    immediateArtwork: true, artTimingEnabled: false,
    artMap: {}, _artInbox: {}, _artHeld: {}, _artKeep: {},
    _artOrder: {}, _artFront: 0, artRevealStep: 6,
    nativeTracksModel: model, nativeAlbumsModel: model,
    nativeArtistsModel: model, nativeArtistAlbumsModel: model,
    traceArt() {}, artPulse: false,
};
const ctx = vm.createContext({root,
    artFlush: {running: false, start() { ticks++; }},
    artPulseOff: {restart() { pulses++; }},
    QbzLocal: {artworkWindow() {}},
});
const flush = block('onTriggered:', source.indexOf('id: artFlush'));
const arrive = block('function onLocalArtworkReady(');
for (let i = 0; i < 30; i++) {
    root._artInbox[`cover${i}`] = `path${i}`;
    root._artOrder[`cover${i}`] = i;
}
vm.runInContext(flush, ctx);
assert.equal(Object.keys(root.artMap).length, 30, 'ready covers must not wait behind the reveal front');
assert.equal(Object.keys(root._artInbox).length, 0);
assert.equal(ticks, 0, 'no follow-up reveal timer');
root._artKeep = {visible: true};
ctx.key = 'old-window'; ctx.path = 'stale.png';
vm.runInContext(arrive, ctx);
assert.equal(root._artInbox['old-window'], undefined, 'late results cannot repopulate the viewport cache');
assert.equal(pulses, 0, 'late results cannot restart the placeholder animation');
assert.equal(models, 4, 'native catalog enrichment still receives the resolved path');
ctx.key = 'visible'; ctx.path = 'visible.png';
vm.runInContext(arrive, ctx);
assert.equal(root._artInbox.visible, 'visible.png');
assert.equal(ticks, 1);
assert.equal(pulses, 1);
// A/B baseline preserves the previous ordered reveal.
root.immediateArtwork = false;
root.artMap = {}; root._artInbox = {}; root._artHeld = {}; root._artFront = 0;
for (let i = 0; i < 12; i++) root._artInbox[`cover${i}`] = `path${i}`;
vm.runInContext(flush, ctx);
assert.equal(Object.keys(root.artMap).length, 6);
assert.equal(Object.keys(root._artHeld).length, 6);
root.immediateArtwork = true;
vm.runInContext(flush, ctx);
assert.equal(Object.keys(root.artMap).length, 12, 'moving to DPR 1 releases held covers');
assert.equal(Object.keys(root._artHeld).length, 0);
console.log('Artwork reveal: immediate batch, stale-result eviction, bounded timers and A/B baseline passed');
