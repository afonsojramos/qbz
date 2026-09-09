#!/usr/bin/env node
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const code = fs.readFileSync(new URL('../crates/qbz-qt/qml/assets/release-sort.js', import.meta.url), 'utf8');
const sort = vm.createContext({});
vm.runInContext(code.replace(/^\.pragma library\s*/, ''), sort);
const rows = [
  { id: 'souls', title: 'The Book of Souls', artist: 'Iron Maiden', year: 'Sep 4, 2015', releaseSortKey: 20150904 },
  { id: 'iron', title: 'Iron Maiden', artist: 'Iron Maiden', year: 'Apr 1, 1980', releaseSortKey: 19800401 },
  { id: 'senjutsu', title: 'Senjutsu', artist: 'Iron Maiden', year: 'Sep 3, 2021', releaseSortKey: 20210903 },
  { id: 'live', title: 'Live After Death', artist: 'Iron Maiden', year: 'Aug 1, 1985', releaseSortKey: 19850801 },
  { id: 'killers', title: 'Killers', artist: 'Iron Maiden', year: 'Feb 1, 1981', releaseSortKey: 19810201 },
];
const ids = items => Array.from(items, x => x.id);
const oldest = ['iron', 'killers', 'live', 'souls', 'senjutsu'];
for (const group of ['off', 'artist', 'year', 'decade']) {
  assert.deepEqual(ids(sort.sortAlbums(rows, 'oldest', group)), oldest);
  assert.deepEqual(ids(sort.sortAlbums(rows, 'newest', group)), oldest.slice().reverse());
}
assert.deepEqual(ids(sort.sortAlbums(rows, 'default', 'off')), ids(rows));
const localized = rows.map(x => ({...x, year: 'fecha visible localizada'}));
assert.deepEqual(ids(sort.sortAlbums(localized, 'oldest', 'off')), oldest);
for (const direction of ['oldest', 'newest']) {
  assert.equal(sort.sortAlbums([{id: 'unknown'}, ...rows], direction, 'off').at(-1).id, 'unknown');
}
assert.deepEqual(ids(sort.sortAlbums([
  {id:'later', releaseSortKey: 20210904}, {id:'earlier', releaseSortKey: 20210903},
], 'oldest', 'off')), ['earlier', 'later']);
assert.equal(sort.groupLabel(rows[1], 'year'), '1980');
assert.equal(sort.groupLabel(rows[1], 'decade'), '1980–1989');
assert.equal(sort.groupLabel({}, 'decade'), '#');
assert.deepEqual(ids(rows), ['souls', 'iron', 'senjutsu', 'live', 'killers']);
console.log('Release chronology, original-date keys, grouping and localized-label independence: passed');

// Execute ArtistView's actual comparator too, including Default after an
// arbitrary sort and after an appended Load more page.
const artistQml = fs.readFileSync(new URL('../crates/qbz-qt/qml/views/ArtistView.qml', import.meta.url), 'utf8');
const begin = artistQml.indexOf('function sortReleaseCards(');
let end = artistQml.indexOf('{', begin) + 1, depth = 1;
while (depth && end < artistQml.length) {
  if (artistQml[end] === '{') ++depth;
  if (artistQml[end] === '}') --depth;
  ++end;
}
assert.equal(depth, 0);
sort.ReleaseSort = sort;
vm.runInContext(artistQml.slice(begin, end), sort);
const ranked = rows.map((x, index) => ({...x, defaultOrder: index}));
assert.deepEqual(ids(sort.sortReleaseCards(ranked, 'oldest')), oldest);
const shuffled = sort.sortReleaseCards(ranked, 'newest');
assert.deepEqual(ids(sort.sortReleaseCards(shuffled, 'default')), ids(rows));
shuffled.push({id:'page2', defaultOrder:10, releaseSortKey:19700101});
assert.deepEqual(ids(sort.sortReleaseCards(sort.sortReleaseCards(shuffled, 'oldest'), 'default')),
  [...ids(rows), 'page2']);
const libraryQml = fs.readFileSync(new URL('../crates/qbz-qt/qml/views/LibraryView.qml', import.meta.url), 'utf8');
const dateGrouping = libraryQml.match(/readonly property bool dateGrouping:([\s\S]*?)readonly property bool alphaActive:/)[1].trim();
for (const activeTab of ['tracks', 'albums', 'artists']) {
  for (const albumsGroup of ['off', 'artist', 'alpha', 'year', 'decade']) {
    assert.equal(vm.runInNewContext(dateGrouping, {activeTab, albumsGroup}),
      activeTab === 'albums' && ['year', 'decade'].includes(albumsGroup));
  }
}
console.log('Artist Default restores server order across sorts/pages; date grouping does not affect track A-Z: passed');

// Run the Library feed parser and visible-items derivation, not just the sort
// helper: duplicate favorite/purchase rows must select the purchased row and
// retain its release date, while source switches remain independent per tab.
function qmlFunction(name) {
  const start = libraryQml.indexOf(`function ${name}(`);
  assert.ok(start >= 0, name);
  let stop = libraryQml.indexOf('{', start) + 1, level = 1;
  while (level && stop < libraryQml.length) {
    if (libraryQml[stop] === '{') ++level;
    if (libraryQml[stop] === '}') --level;
    ++stop;
  }
  assert.equal(level, 0, name);
  return libraryQml.slice(start, stop);
}
const bridge = {
  sessionShowPurchases: true, sessionShowFavorites: true, sessionShowFollowing: true,
  sessionAlbumsShowPurchases: true, sessionAlbumsShowFavorites: true,
  sessionTracksShowPurchases: true, sessionTracksShowFavorites: true,
};
const lib = vm.createContext({
  QbzLibrary: bridge, ReleaseSort: sort, Qt: {callLater() {}},
  activeTab: 'all', search: '', tabSearch: '', showLocal: true,
  sortBy: 'date', sortAsc: false, albumsSort: 'oldest', albumsGroup: 'off',
  tracksGroup: 'off', artistsGroup: 'off', genreNames: [],
});
lib.root = lib;
for (const name of ['showPurchases', 'showFavorites', 'showFollowing', 'genreContext']) {
  const expr = libraryQml.match(new RegExp(`readonly property (?:bool|string) ${name}:([^\\n]*(?:\\n        [^\\n]*)*)`))[1];
  Object.defineProperty(lib, name, {get() { return vm.runInContext(expr, lib); }});
}
for (const name of ['parseFeed', 'visibleItems', 'matchesSources', 'setShowPurchases', 'setShowFavorites', 'setShowFollowing']) {
  vm.runInContext(qmlFunction(name), lib);
}
const libraryRows = [];
for (const kind of ['album', 'track']) {
  for (const [id, groups, date] of [
    ['both', ['favorites', 'purchases'], 20150904],
    ['purchased', ['purchases'], 20210903],
    ['favorite', ['favorites'], 19800401],
  ]) {
    for (const group of groups) libraryRows.push({id, kind, group, title:id,
      artist:'Iron Maiden', source:'qobuz', year:String(date), releaseSortKey:date});
  }
}
libraryRows.push({id:'followed', kind:'playlist', group:'following', source:'qobuz', title:'Followed'});
lib.feed = lib.parseFeed(JSON.stringify(libraryRows));
for (const tab of ['all', 'albums', 'tracks']) {
  lib.activeTab = tab;
  if (tab === 'all') lib.setShowFollowing(false);
  for (const [purchases, favorites, expected] of [
    [true, true, ['both','favorite','purchased']],
    [true, false, ['both','purchased']],
    [false, true, ['both','favorite']],
    [false, false, ['both','favorite','purchased']],
  ]) {
    lib.setShowPurchases(purchases); lib.setShowFavorites(favorites);
    const shown = lib.visibleItems().filter(x => x.kind === (tab === 'tracks' ? 'track' : 'album'));
    assert.deepEqual([...new Set(ids(shown))].sort(), expected, `${tab}: ${purchases}/${favorites}`);
    if (tab !== 'all') {
      assert.equal(shown.length, expected.length, 'No duplicated entities');
      assert.equal(shown.find(x => x.id === 'both').group, 'purchases');
    }
  }
}
lib.activeTab = 'albums'; lib.setShowPurchases(true); lib.setShowFavorites(true);
assert.deepEqual(ids(lib.visibleItems()), ['favorite', 'both', 'purchased']);
lib.albumsSort = 'newest';
assert.deepEqual(ids(lib.visibleItems()), ['purchased', 'both', 'favorite']);
// Navigation away and component recreation must read the singleton's own tab
// state, rather than copy whichever tab was last visible.
lib.activeTab = 'all'; lib.setShowPurchases(false); lib.setShowFavorites(true); lib.setShowFollowing(false);
lib.activeTab = 'albums'; lib.setShowPurchases(true); lib.setShowFavorites(false);
lib.activeTab = 'tracks'; lib.setShowPurchases(false); lib.setShowFavorites(false);
for (const [tab, expected] of [['all',[false,true]], ['albums',[true,false]], ['tracks',[false,false]]]) {
  lib.activeTab = tab;
  assert.deepEqual([lib.showPurchases, lib.showFavorites], expected);
  assert.equal(lib.genreContext, `library-${tab}`);
}
lib.activeTab = 'all'; lib.setShowPurchases(false); lib.setShowFavorites(false); lib.setShowFollowing(true);
assert.deepEqual(ids(lib.visibleItems()), ['followed']);
lib.activeTab = 'albums';
assert.deepEqual(ids(lib.visibleItems()), ['purchased', 'both'], 'Following cannot gate Albums');
console.log('Library source union, per-tab state, purchase-row dates, deduplication and chronology: passed');

// Popularity leaves server order intact, and changing its paging order must
// discard the old overlay/cursor while a local date/title sort keeps them.
assert.deepEqual(ids(sort.sortReleaseCards(ranked, 'relevant')), ids(rows));
const changeStart = artistQml.indexOf('function releaseSortChanged(');
let changeEnd = artistQml.indexOf('{', changeStart) + 1, changeDepth = 1;
while (changeDepth && changeEnd < artistQml.length) {
  if (artistQml[changeEnd] === '{') ++changeDepth;
  if (artistQml[changeEnd] === '}') --changeDepth;
  ++changeEnd;
}
const artistState = vm.createContext({QbzArtist:{setSectionSort() {}}});
artistState.root = artistState;
vm.runInContext(artistQml.slice(changeStart, changeEnd), artistState);
for (const [from,to,reset] of [['default','relevant',true],['relevant','oldest',true],['oldest','newest',false]]) {
  artistState.artist = {releaseSections:[{releaseType:'album',sortBy:from}]};
  artistState.releaseFade = {};
  artistState.releaseOverlay = {album:{cards:[{id:'old-page'}],hasMore:false}};
  artistState.releaseCursor = {album:{next:40}};
  artistState.releasePending = {album:true};
  artistState.releaseSortChanged('album',to);
  assert.equal(artistState.releaseCursor.album === undefined, reset);
  assert.equal(artistState.releaseOverlay.album === undefined, reset);
  if (reset) assert.equal(artistState.releasePending.album, undefined);
}
console.log('Artist popularity preserves server rank and resets pagination across server-order changes: passed');
