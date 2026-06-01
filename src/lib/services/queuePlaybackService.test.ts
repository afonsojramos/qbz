import { describe, expect, it } from 'vitest';

import {
  assessQconnectQueueSync,
  shouldAutoSkipOnPlaybackError,
  type QconnectPlaybackErrorPayload
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

  it('refuses (not subset) when queue is mixed — on-connect contract', () => {
    const r = assessQconnectQueueSync([
      buildTrack(1, { source: 'qobuz' }),
      buildTrack(2, { source: 'local' })
    ]);
    expect(r.syncable).toBe(false);
    expect(r.reason).toBe('queue_contains_non_qobuz_tracks');
    expect(r.trackIds).toEqual([]); // subset NOT returned
  });
});

describe('assessQconnectQueueSync — offline-cache', () => {
  it('treats qobuz_download as syncable (offline copy carries a Qobuz id)', () => {
    const r = assessQconnectQueueSync([buildTrack(5, { source: 'qobuz_download' })]);
    expect(r.syncable).toBe(true);
    expect(r.trackIds).toEqual([5]);
    expect(r.blockedTrackIds).toEqual([]);
  });
  it('still blocks local and plex', () => {
    const r = assessQconnectQueueSync([
      buildTrack(1, { source: 'qobuz' }), buildTrack(2, { source: 'local' }),
    ]);
    expect(r.syncable).toBe(false);
    expect(r.blockedTrackIds).toContain(2);
  });
});

describe('shouldAutoSkipOnPlaybackError', () => {
  it('auto-skips on TrackNotStreamable for the current queue item', () => {
    const payload: QconnectPlaybackErrorPayload = { queue_item_id: 7, error_type: 'TrackNotStreamable' };
    expect(shouldAutoSkipOnPlaybackError(payload, 7)).toBe(true);
  });
  it('does NOT skip when the failed item is not the one currently playing', () => {
    expect(shouldAutoSkipOnPlaybackError({ queue_item_id: 9, error_type: 'NetworkError' }, 7)).toBe(false);
  });
  it('does NOT skip on Unknown error type (avoid skip storms)', () => {
    expect(shouldAutoSkipOnPlaybackError({ queue_item_id: 7, error_type: 'Unknown' }, 7)).toBe(false);
  });
  it('does NOT skip when current queue item id is null', () => {
    expect(shouldAutoSkipOnPlaybackError({ queue_item_id: 7, error_type: 'TrackNotStreamable' }, null)).toBe(false);
  });
});
