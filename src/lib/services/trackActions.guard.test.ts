import { describe, it, expect } from 'vitest';
import { partitionQconnectLoadableTracks } from './trackActions';
import type { BackendQueueTrack } from '$lib/stores/queueStore';

const t = (over: Partial<BackendQueueTrack>): BackendQueueTrack =>
  ({ id: 1, title: 't', artist: 'a', album: 'al', duration_secs: 0, artwork_url: null,
     hires: false, bit_depth: null, sample_rate: null, is_local: false, album_id: null,
     artist_id: null, streamable: true, source: 'qobuz', parental_warning: false, ...over }) as BackendQueueTrack;

describe('partitionQconnectLoadableTracks', () => {
  it('separates Qobuz/offline from local/plex', () => {
    const r = partitionQconnectLoadableTracks([
      t({ id: 1, source: 'qobuz' }), t({ id: 2, source: 'qobuz_download' }),
      t({ id: 3, source: 'local' }), t({ id: 4, source: 'plex' }),
    ]);
    expect(r.loadableIds).toEqual([1, 2]);
    expect(r.blockedIds).toEqual([3, 4]);
    expect(r.hasBlocked).toBe(true);
  });
});
