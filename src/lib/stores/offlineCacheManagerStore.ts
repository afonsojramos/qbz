/**
 * Offline Cache Manager Store
 *
 * Phase 4 (P4.1): Types and `buildRollup` pure function only.
 * Reactive store API (subscribe / refresh / event listeners) lands in P5.1.
 */

export type OfflineStatus = 'queued' | 'downloading' | 'ready' | 'failed';

export type CachedTrackInfo = {
  trackId: number;
  title: string;
  artist: string;
  album: string | null;
  albumId: string | null;
  durationSecs: number;
  fileSizeBytes: number;
  quality: string;
  bitDepth: number | null;
  sampleRate: number | null;
  status: OfflineStatus;
  progressPercent: number;
  errorMessage: string | null;
  createdAt: string;
  lastAccessedAt: string;
};

export type AlbumGroup = {
  albumId: string | null;          // null => "Singles & loose tracks"
  title: string;
  artistLabel: string;
  year: number | null;
  coverUrl: string | null;
  cachedTracks: CachedTrackInfo[];
  totalSizeBytes: number;
  worstStatus: OfflineStatus;
  failedCount: number;
  isFullyCached: boolean | null;
  dominantQuality: string;
  mostRecentCachedAt: string;
};

export type ArtistGroup = {
  artistKey: string;
  artistName: string;
  albumGroups: AlbumGroup[];
  totalSizeBytes: number;
  totalTracks: number;
};

const STATUS_RANK: Record<OfflineStatus, number> = {
  ready: 0,
  queued: 1,
  downloading: 2,
  failed: 3,
};

function worseStatus(a: OfflineStatus, b: OfflineStatus): OfflineStatus {
  return STATUS_RANK[a] >= STATUS_RANK[b] ? a : b;
}

function dominantString(values: string[]): string {
  if (values.length === 0) return '';
  const counts = new Map<string, number>();
  for (const v of values) counts.set(v, (counts.get(v) ?? 0) + 1);
  let best = values[0];
  let bestCount = 0;
  for (const [v, c] of counts) {
    if (c > bestCount) { best = v; bestCount = c; }
  }
  // Heterogeneous if no single value > 50% of total
  return bestCount * 2 > values.length ? best : 'Mixed';
}

export function buildRollup(
  tracks: CachedTrackInfo[],
  fullyCachedFlags: Map<string, boolean>,
  singlesPseudoAlbumLabel: string,
): ArtistGroup[] {
  const byArtist = new Map<string, CachedTrackInfo[]>();
  for (const track of tracks) {
    const key = track.artist.trim().toLowerCase();
    if (!byArtist.has(key)) byArtist.set(key, []);
    byArtist.get(key)!.push(track);
  }

  const artists: ArtistGroup[] = [];
  for (const [artistKey, artistTracks] of byArtist) {
    const displayNames = artistTracks.map(track => track.artist);
    const artistName = dominantString(displayNames);
    // dominantString returns 'Mixed' for ties; for artist name fall back to first.
    const finalArtistName = artistName === 'Mixed' ? displayNames[0] : artistName;

    const byAlbum = new Map<string, CachedTrackInfo[]>();
    for (const track of artistTracks) {
      const albumKey = track.albumId ?? '__singles__';
      if (!byAlbum.has(albumKey)) byAlbum.set(albumKey, []);
      byAlbum.get(albumKey)!.push(track);
    }

    const albumGroups: AlbumGroup[] = [];
    for (const [albumKey, albumTracks] of byAlbum) {
      const albumId = albumKey === '__singles__' ? null : albumKey;
      const title = albumId
        ? (albumTracks[0].album ?? singlesPseudoAlbumLabel)
        : singlesPseudoAlbumLabel;
      const artistLabel = albumId
        ? dominantString(albumTracks.map(track => track.artist)) || finalArtistName
        : finalArtistName;
      const totalSizeBytes = albumTracks.reduce((s, track) => s + track.fileSizeBytes, 0);
      const failedCount = albumTracks.filter(track => track.status === 'failed').length;
      const worstStatus = albumTracks.reduce<OfflineStatus>(
        (acc, track) => worseStatus(acc, track.status),
        'ready',
      );
      const dominantQuality = dominantString(albumTracks.map(track => track.quality));
      const mostRecentCachedAt = albumTracks.reduce(
        (acc, track) => (track.createdAt > acc ? track.createdAt : acc),
        '',
      );
      const isFullyCached = albumId ? (fullyCachedFlags.get(albumId) ?? null) : null;

      albumGroups.push({
        albumId,
        title,
        artistLabel,
        year: null,           // Not in cache DB; UI may enrich later.
        coverUrl: null,       // Resolved by frontend from artwork helpers.
        cachedTracks: albumTracks.sort((a, b) => a.title.localeCompare(b.title)),
        totalSizeBytes,
        worstStatus,
        failedCount,
        isFullyCached,
        dominantQuality,
        mostRecentCachedAt,
      });
    }

    albumGroups.sort((a, b) => a.title.localeCompare(b.title));

    artists.push({
      artistKey,
      artistName: finalArtistName,
      albumGroups,
      totalSizeBytes: albumGroups.reduce((s, g) => s + g.totalSizeBytes, 0),
      totalTracks: artistTracks.length,
    });
  }

  artists.sort((a, b) => a.artistName.localeCompare(b.artistName));
  return artists;
}
