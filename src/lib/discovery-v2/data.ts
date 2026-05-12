/**
 * Discovery V2 — data layer.
 *
 * Pure functions wrapping the V2 invoke surface. Each function returns the
 * minimum shape the corresponding `*Lite` card component needs to render.
 * Mapping happens here so individual sections don't carry the full Qobuz
 * payload around (most fields are unused by Discovery).
 */

import { invoke } from '@tauri-apps/api/core';
import type { QobuzAlbum } from '$lib/types';
import { getQobuzImageForSize } from '$lib/adapters/qobuzAdapters';

// Wire format from src-tauri/src/api/models.rs `SearchResultsPage<T>`.
interface SearchResultsPage<T> {
  items: T[];
  total: number;
  offset: number;
  limit: number;
}

/**
 * Minimum shape an AlbumCardLite needs. Intentionally narrower than
 * `AlbumCardData` in HomeView.svelte (which carries quality, samplingRate,
 * bitDepth, awards, parental_warning, etc. that V1 of Discovery doesn't
 * render).
 */
export interface DiscoveryAlbumCard {
  albumId: string;
  title: string;
  artist: string;
  artistId?: number;
  artwork?: string;
}

function albumToCard(album: QobuzAlbum): DiscoveryAlbumCard {
  return {
    albumId: album.id,
    title: album.title,
    artist: album.artist.name,
    artistId: album.artist.id,
    artwork: getQobuzImageForSize(album.image, 'small'),
  };
}

/**
 * Fetch the "New Releases" feed (release-watch on followed artists). The V2
 * command returns full Album objects in a single round-trip; the current
 * HomeView still calls `v2_get_album` per id on top of release-watch, which
 * is wasteful — Discovery V2 uses the album payload that already comes back.
 */
export async function fetchNewReleases(limit = 8): Promise<DiscoveryAlbumCard[]> {
  try {
    const page = await invoke<SearchResultsPage<QobuzAlbum>>('v2_get_release_watch', {
      releaseType: 'artists',
      limit,
      offset: 0,
    });
    return page.items.map(albumToCard);
  } catch (err) {
    console.error('[discovery-v2] fetchNewReleases failed', err);
    return [];
  }
}
