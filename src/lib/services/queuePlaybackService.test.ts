import { describe, expect, it } from 'vitest';

import {
  assessQconnectQueueSync
} from './queuePlaybackService';
import type { BackendQueueTrack } from '$lib/stores/queueStore';

function buildTrack(id: number, overrides: Partial<BackendQueueTrack> = {}): BackendQueueTrack {
  return {
    id,
    title: `Track ${id}`,
    artist: 'Artist',
    album: 'Album',
    duration_secs: 180,
    artwork_url: null,
    hires: false,
    bit_depth: null,
    sample_rate: null,
    ...overrides
  };
}

describe('assessQconnectQueueSync', () => {
  it('accepts a pure Qobuz queue', () => {
    expect(assessQconnectQueueSync([
      buildTrack(101),
      buildTrack(102),
      buildTrack(103)
    ])).toEqual({
      syncable: true,
      reason: 'ok',
      trackIds: [101, 102, 103],
      blockedTrackIds: []
    });
  });

  it('rejects local tracks so remote queue is not desynced', () => {
    expect(assessQconnectQueueSync([
      buildTrack(101),
      buildTrack(9001, { is_local: true, source: 'local' })
    ])).toEqual({
      syncable: false,
      reason: 'queue_contains_non_qobuz_tracks',
      trackIds: [],
      blockedTrackIds: [9001]
    });
  });

  it('rejects Plex tracks', () => {
    expect(assessQconnectQueueSync([
      buildTrack(7001, { source: 'plex', is_local: false })
    ])).toEqual({
      syncable: false,
      reason: 'queue_contains_non_qobuz_tracks',
      trackIds: [],
      blockedTrackIds: [7001]
    });
  });

  it('rejects an empty queue', () => {
    expect(assessQconnectQueueSync([])).toEqual({
      syncable: false,
      reason: 'empty_queue',
      trackIds: [],
      blockedTrackIds: []
    });
  });
});
