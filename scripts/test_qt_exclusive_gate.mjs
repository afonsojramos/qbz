#!/usr/bin/env node
// Executes the REAL gate expressions of the Settings > Audio "Exclusive mode"
// row (visible / rowEnabled / toggle enabled) against a platform x backend
// matrix, the way test_qt_release_sort.mjs evaluates LibraryView bindings.
//
// #748: the row must be reachable on macOS, whose only backend (System
// default) honours `exclusive_mode` through CoreAudio Hog Mode (PR #391,
// qbz-audio backend.rs `create_output_stream_with_exclusive_guard`). It was
// hidden there by fc0d35e29 on the wrong premise that no macOS backend could
// honour it. Linux and Windows must keep their 2026-08-27 behaviour.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const qml = fs.readFileSync(
  new URL('../crates/qbz-qt/qml/settings/AudioSettings.qml', import.meta.url),
  'utf8',
);

// Locate the SettingRow that carries the "Exclusive mode" label and return
// its full block (balanced braces), with // comments stripped so a comment
// can never satisfy a binding match.
function exclusiveRowBlock(source) {
  const label = source.indexOf('QbzSession.tr("Exclusive mode"');
  assert.notEqual(label, -1, 'Exclusive mode row must exist');
  const start = source.lastIndexOf('SettingRow {', label);
  assert.notEqual(start, -1, 'Exclusive mode label must sit inside a SettingRow');
  let end = source.indexOf('{', start) + 1;
  let depth = 1;
  while (depth && end < source.length) {
    if (source[end] === '{') ++depth;
    if (source[end] === '}') --depth;
    ++end;
  }
  assert.equal(depth, 0, 'SettingRow block must be balanced');
  return source
    .slice(start, end)
    .split('\n')
    .map(line => line.replace(/\/\/.*$/, ''))
    .join('\n');
}

const block = exclusiveRowBlock(qml);
const binding = (text, name) => {
  const m = text.match(new RegExp(`(?:^|[\\s;{])${name}:\\s*([^\\n]+)`));
  assert.ok(m, `binding "${name}" must be present`);
  return m[1].trim();
};
const toggleStart = block.indexOf('QbzToggle {');
assert.notEqual(toggleStart, -1, 'row must contain a QbzToggle');
const rowPart = block.slice(0, toggleStart);
const togglePart = block.slice(toggleStart);

const visibleExpr = binding(rowPart, 'visible');
const rowEnabledExpr = binding(rowPart, 'rowEnabled');
const toggleEnabledExpr = binding(togglePart, 'enabled');

// Evaluate a binding as QML would: an absent document field reads as
// undefined, so `undefined === true` is false and the row stays hidden.
function evaluate(expr, platform, doc) {
  const QbzShell = {
    isLinux: platform === 'linux',
    isMacos: platform === 'macos',
    isWindows: platform === 'windows',
  };
  return Boolean(vm.runInNewContext(expr, { QbzShell, root: { doc } }));
}

const cases = [
  // Linux: unchanged since 2026-08-27 — always visible, enabled only on ALSA.
  { platform: 'linux', doc: { backendIsAlsa: true }, visible: true, enabled: true },
  { platform: 'linux', doc: { backendIsPipewire: true }, visible: true, enabled: false },
  { platform: 'linux', doc: {}, visible: true, enabled: false },
  // macOS: System default IS the CoreAudio exclusive backend (Hog Mode).
  { platform: 'macos', doc: { backendIsCoreAudio: true }, visible: true, enabled: true },
  // macOS with the flag absent (older document): stays hidden, never a grey row.
  { platform: 'macos', doc: {}, visible: false, enabled: false },
  // Windows: hidden on purpose (5ba49a739) — the exclusive path is the
  // backend dropdown, the toggle changes nothing there.
  { platform: 'windows', doc: {}, visible: false, enabled: false },
  { platform: 'windows', doc: { backendIsWasapi: true }, visible: false, enabled: true },
];

for (const c of cases) {
  const tag = `${c.platform} ${JSON.stringify(c.doc)}`;
  assert.equal(evaluate(visibleExpr, c.platform, c.doc), c.visible, `visible on ${tag}`);
  assert.equal(evaluate(rowEnabledExpr, c.platform, c.doc), c.enabled, `rowEnabled on ${tag}`);
  assert.equal(evaluate(toggleEnabledExpr, c.platform, c.doc), c.enabled, `toggle enabled on ${tag}`);
}

// The toggle must keep writing the same settings key the bridge dispatches
// ("exclusive-mode" -> AudioSettingsStore::set_exclusive_mode, Apply::Reinit).
assert.match(togglePart, /QbzBridge\.settingsBool\("exclusive-mode", v\)/);

console.log('Exclusive mode row gates: Linux unchanged, macOS CoreAudio reachable, Windows hidden: passed');
