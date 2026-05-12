/**
 * Discovery V2 — data layer.
 *
 * Pure functions wrapping the V2 invoke surface. Each function returns the
 * minimum shape the corresponding `*Lite` card component needs to render.
 * Mapping happens here so individual sections don't carry the full Qobuz
 * payload around (most fields are unused by Discovery).
 */

import { invoke } from '@tauri-apps/api/core';
import type { QobuzAlbum, DiscoverResponse, DiscoverAlbum } from '$lib/types';
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

function qobuzAlbumToCard(album: QobuzAlbum): DiscoveryAlbumCard {
  return {
    albumId: album.id,
    title: album.title,
    artist: album.artist.name,
    artistId: album.artist.id,
    artwork: getQobuzImageForSize(album.image, 'small'),
  };
}

function discoverAlbumToCard(album: DiscoverAlbum): DiscoveryAlbumCard {
  return {
    albumId: album.id,
    title: album.title,
    artist: album.artists?.[0]?.name ?? 'Unknown Artist',
    artistId: album.artists?.[0]?.id,
    artwork: album.image?.small || album.image?.large || album.image?.thumbnail,
  };
}

/**
 * Fetch the "Release Watch" feed — releases from followed artists. The V2
 * command returns full Album objects in a single round-trip; the original
 * HomeView called `v2_get_album` per id on top of release-watch (N+1),
 * which Discovery sidesteps.
 */
export async function fetchReleaseWatch(limit = 8): Promise<DiscoveryAlbumCard[]> {
  try {
    const page = await invoke<SearchResultsPage<QobuzAlbum>>('v2_get_release_watch', {
      releaseType: 'artists',
      limit,
      offset: 0,
    });
    return page.items.map(qobuzAlbumToCard);
  } catch (err) {
    console.error('[discovery-v2] fetchReleaseWatch failed', err);
    return [];
  }
}

/**
 * Editorial album sections — one round-trip returns five containers
 * (new releases, press accolades, most streamed, qobuzissimes, album of
 * the week) plus playlists. Discovery splits the result into the shape
 * each section needs.
 */
export interface DiscoverIndexSections {
  newReleases: DiscoveryAlbumCard[];
  pressAwards: DiscoveryAlbumCard[];
  mostStreamed: DiscoveryAlbumCard[];
  qobuzissimes: DiscoveryAlbumCard[];
  editorPicks: DiscoveryAlbumCard[];
}

export async function fetchDiscoverIndex(
  perSection = 8,
  genreIds: number[] = []
): Promise<DiscoverIndexSections> {
  const empty: DiscoverIndexSections = {
    newReleases: [],
    pressAwards: [],
    mostStreamed: [],
    qobuzissimes: [],
    editorPicks: [],
  };
  try {
    const apiGenreIds = genreIds.length > 0 ? genreIds : null;
    const response = await invoke<DiscoverResponse>('v2_get_discover_index', {
      genreIds: apiGenreIds,
    });
    const c = response.containers;
    const take = (items: DiscoverAlbum[] | undefined): DiscoveryAlbumCard[] =>
      (items ?? []).slice(0, perSection).map(discoverAlbumToCard);

    return {
      newReleases: take(c.new_releases?.data?.items),
      pressAwards: take(c.press_awards?.data?.items),
      mostStreamed: take(c.most_streamed?.data?.items),
      qobuzissimes: take(c.qobuzissims?.data?.items),
      editorPicks: take(c.album_of_the_week?.data?.items),
    };
  } catch (err) {
    console.error('[discovery-v2] fetchDiscoverIndex failed', err);
    return empty;
  }
}
