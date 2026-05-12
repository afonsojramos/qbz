/**
 * Discovery V2 — section preferences.
 *
 * Persists the user's choice of which sections to render and their order.
 * Stored in localStorage; the schema is small enough that we don't need a
 * dedicated SQLite table for V1. If the user clears site data we fall back
 * to the defaults below.
 *
 * Defaults: seven sections enabled, the other five available to switch on.
 * The list of enabled-by-default reflects a "useful for a freshly-installed
 * user" balance — discovery-led (new releases, press, top albums, ideal
 * discography, playlists) plus personalized (recently played albums and
 * tracks). Other sections (release watch, qobuzissimes, editor picks, top
 * artists, favorites) require either a long play history or deep catalog
 * familiarity and are off by default to keep the initial DOM cost low.
 */

import { writable, get } from 'svelte/store';

export type DiscoverySectionId =
  | 'newReleases'
  | 'pressAwards'
  | 'qobuzPlaylists'
  | 'recentlyPlayedAlbums'
  | 'continueListening'
  | 'idealDiscography'
  | 'mostStreamed'
  | 'releaseWatch'
  | 'editorPicks'
  | 'qobuzissimes'
  | 'topArtists'
  | 'favoriteAlbums';

export interface DiscoverySectionPref {
  id: DiscoverySectionId;
  enabled: boolean;
}

/**
 * The order of this array IS the render order. enabled=true sections render;
 * enabled=false don't, but their position survives toggling.
 */
const DEFAULT_PREFS: DiscoverySectionPref[] = [
  { id: 'newReleases', enabled: true },
  { id: 'pressAwards', enabled: true },
  { id: 'qobuzPlaylists', enabled: true },
  { id: 'recentlyPlayedAlbums', enabled: true },
  { id: 'continueListening', enabled: true },
  { id: 'idealDiscography', enabled: true },
  { id: 'mostStreamed', enabled: true },
  { id: 'releaseWatch', enabled: false },
  { id: 'editorPicks', enabled: false },
  { id: 'qobuzissimes', enabled: false },
  { id: 'topArtists', enabled: false },
  { id: 'favoriteAlbums', enabled: false },
];

const STORAGE_KEY = 'qbz.discovery-v2.section-prefs';

function loadPersisted(): DiscoverySectionPref[] {
  if (typeof localStorage === 'undefined') return DEFAULT_PREFS;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_PREFS;
    const parsed = JSON.parse(raw) as DiscoverySectionPref[];
    if (!Array.isArray(parsed)) return DEFAULT_PREFS;
    return migrate(parsed);
  } catch {
    return DEFAULT_PREFS;
  }
}

/**
 * Reconcile a persisted list against the canonical set of section IDs.
 * If a new section is introduced after the user has saved prefs, append it
 * to the tail with its default enabled flag (rather than dropping it).
 * If a saved ID is unknown (renamed/removed), drop it.
 */
function migrate(persisted: DiscoverySectionPref[]): DiscoverySectionPref[] {
  const valid = new Map(DEFAULT_PREFS.map((p) => [p.id, p]));
  const seen = new Set<DiscoverySectionId>();
  const kept: DiscoverySectionPref[] = [];
  for (const item of persisted) {
    if (!item || typeof item.id !== 'string') continue;
    const def = valid.get(item.id);
    if (!def) continue;
    if (seen.has(item.id)) continue;
    seen.add(item.id);
    kept.push({ id: item.id, enabled: !!item.enabled });
  }
  for (const def of DEFAULT_PREFS) {
    if (!seen.has(def.id)) kept.push(def);
  }
  return kept;
}

function persist(prefs: DiscoverySectionPref[]) {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // quota or storage disabled — ignore
  }
}

export const sectionPrefs = writable<DiscoverySectionPref[]>(loadPersisted());

sectionPrefs.subscribe((value) => persist(value));

export function toggleSection(id: DiscoverySectionId) {
  sectionPrefs.update((prefs) =>
    prefs.map((p) => (p.id === id ? { ...p, enabled: !p.enabled } : p))
  );
}

export function moveSection(id: DiscoverySectionId, direction: -1 | 1) {
  sectionPrefs.update((prefs) => {
    const idx = prefs.findIndex((p) => p.id === id);
    if (idx < 0) return prefs;
    const target = idx + direction;
    if (target < 0 || target >= prefs.length) return prefs;
    const next = prefs.slice();
    [next[idx], next[target]] = [next[target], next[idx]];
    return next;
  });
}

export function resetToDefaults() {
  sectionPrefs.set(DEFAULT_PREFS);
}

export function isEnabled(id: DiscoverySectionId): boolean {
  return get(sectionPrefs).find((p) => p.id === id)?.enabled ?? false;
}

export function enabledCount(): number {
  return get(sectionPrefs).filter((p) => p.enabled).length;
}
