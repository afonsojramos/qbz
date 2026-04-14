/**
 * Award catalog — resolves award ids by name for the cases where
 * /album/get returns an award with only a name and no id. Fetched
 * lazily via /award/explore (paginated), cached in memory per session.
 */

import { invoke } from '@tauri-apps/api/core';

interface AwardCatalogEntry {
  id: string;
  name: string;
}

interface ExploreResponse {
  has_more?: boolean;
  items?: Array<{ id?: string | number; name?: string }>;
}

const PAGE_SIZE = 100;

// name (lowercased, trimmed) → id
let byName = new Map<string, string>();
let loaded = false;
let inflight: Promise<void> | null = null;

function normalize(name: string): string {
  return name.trim().toLowerCase();
}

async function loadCatalog(): Promise<void> {
  if (loaded) return;
  if (inflight) return inflight;

  inflight = (async () => {
    const next = new Map<string, string>();
    let offset = 0;
    while (true) {
      try {
        const result = await invoke<ExploreResponse>('v2_get_award_explore', {
          limit: PAGE_SIZE,
          offset,
        });
        const items = result.items ?? [];
        for (const item of items) {
          if (item?.id == null || !item?.name) continue;
          next.set(normalize(item.name), String(item.id));
        }
        if (items.length < PAGE_SIZE) break;
        if (result.has_more === false) break;
        offset += items.length;
        // Reasonable upper bound to avoid infinite loops on a broken API.
        if (offset >= 2000) break;
      } catch (err) {
        console.error('[AwardCatalog] explore failed:', err);
        break;
      }
    }
    byName = next;
    loaded = true;
    inflight = null;
  })();

  return inflight;
}

/**
 * Return the award id for a given name, fetching the catalog on
 * first call. Resolves to null if not found.
 */
export async function resolveAwardIdByName(name: string): Promise<string | null> {
  if (!name) return null;
  await loadCatalog();
  return byName.get(normalize(name)) ?? null;
}

/** Synchronous lookup — returns null until loadCatalog has run. */
export function lookupAwardIdByName(name: string): string | null {
  if (!name) return null;
  return byName.get(normalize(name)) ?? null;
}

export function isCatalogLoaded(): boolean {
  return loaded;
}
