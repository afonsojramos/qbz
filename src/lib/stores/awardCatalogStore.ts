/**
 * Award catalog — resolves award ids by name for the cases where
 * /album/get returns an award entry without an id. Backed by
 * /award/explore (paginated), fetched once per session and cached
 * in-memory. Names are compared normalized (NFD + diacritics
 * stripped + lowercase + whitespace collapsed) so locale changes
 * affecting accent/case don't break the match.
 *
 * No hardcoded name→id mappings — the catalog is entirely what the
 * Qobuz API returns in the user's current session locale, so the
 * match is agnostic and always in sync with whatever Qobuz is
 * currently labelling awards.
 */

import { invoke } from '@tauri-apps/api/core';

interface ExploreResponse {
  has_more?: boolean;
  items?: Array<{ id?: string | number; name?: string }>;
}

const PAGE_SIZE = 100;
const MAX_PAGES = 40; // 4000 items ceiling — safety net, real catalog is ~hundreds

// normalized name → id
let byName = new Map<string, string>();
let catalogLoaded = false;
let inflight: Promise<void> | null = null;

function normalize(name: string): string {
  return name
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .trim()
    .toLowerCase()
    .replace(/\s+/g, ' ');
}

async function loadCatalog(): Promise<void> {
  if (catalogLoaded) return;
  if (inflight) return inflight;

  inflight = (async () => {
    let offset = 0;
    let pages = 0;
    let totalSeen = 0;
    while (pages < MAX_PAGES) {
      try {
        const result = await invoke<ExploreResponse>('v2_get_award_explore', {
          limit: PAGE_SIZE,
          offset,
        });
        const items = result.items ?? [];
        for (const item of items) {
          if (item?.id == null || !item?.name) continue;
          byName.set(normalize(item.name), String(item.id));
        }
        totalSeen += items.length;
        pages += 1;
        if (items.length < PAGE_SIZE) break;
        if (result.has_more === false) break;
        offset += items.length;
      } catch (err) {
        console.error('[AwardCatalog] /award/explore failed at offset', offset, err);
        break;
      }
    }
    catalogLoaded = true;
    inflight = null;
    console.log(`[AwardCatalog] cached ${byName.size} distinct name keys from ${totalSeen} items across ${pages} page(s)`);
  })();

  return inflight;
}

/**
 * Return the award id for a given name. Fetches the /award/explore
 * catalog on first call (cache-first afterwards). Resolves to null
 * if the name isn't in the catalog.
 */
export async function resolveAwardIdByName(name: string): Promise<string | null> {
  if (!name) return null;
  const key = normalize(name);

  // Cache-first: if we already resolved this name (either from a prior
  // lookup or from the catalog), return immediately.
  const cached = byName.get(key);
  if (cached) return cached;

  await loadCatalog();
  const hit = byName.get(key) ?? null;
  if (!hit) {
    console.warn(`[AwardCatalog] no match for award name "${name}" (normalized: "${key}") among ${byName.size} cached entries`);
  }
  return hit;
}

/** Synchronous lookup — hits only the already-loaded catalog. */
export function lookupAwardIdByName(name: string): string | null {
  if (!name) return null;
  return byName.get(normalize(name)) ?? null;
}

export function isCatalogLoaded(): boolean {
  return catalogLoaded;
}
