import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...a: unknown[]) => invokeMock(...a),
  convertFileSrc: (p: string) => p
}));
vi.mock('@tauri-apps/api/event', () => ({ emit: vi.fn() }));
vi.mock('@tauri-apps/plugin-clipboard-manager', () => ({ writeText: vi.fn() }));
vi.mock('$lib/stores/toastStore', () => ({ showToast: vi.fn() }));
vi.mock('$lib/i18n', () => ({
  t: {
    subscribe: (fn: (v: (k: string) => string) => void) => {
      fn((k) => k);
      return () => {};
    }
  }
}));
vi.mock('$lib/stores/queueStore', () => ({ addToQueue: vi.fn(), addToQueueNext: vi.fn() }));
vi.mock('$lib/stores/playerStore', () => ({ getPlayerState: () => ({ currentTrack: null }) }));
vi.mock('$lib/services/playbackService', () => ({ addTrackToFavorites: vi.fn() }));
vi.mock('$lib/stores/uiStore', () => ({ openPlaylistModal: vi.fn() }));

import {
  reorderQconnectQueueIfRemote,
  removeQconnectQueueItemsIfRemote,
  clearQconnectQueueIfRemote
} from './trackActions';

function setConnected(connected: boolean) {
  invokeMock.mockImplementation((cmd: string) =>
    cmd === 'v2_qconnect_status'
      ? Promise.resolve({ transport_connected: connected })
      : Promise.resolve(undefined)
  );
}

describe('QConnect remote queue mutation routing', () => {
  beforeEach(() => invokeMock.mockReset());
  afterEach(() => vi.clearAllMocks());

  it('routes reorder to queue_reorder_tracks when connected', async () => {
    setConnected(true);
    expect(await reorderQconnectQueueIfRemote([10, 30, 20])).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith('v2_qconnect_send_command_with_admission', {
      request: {
        command_type: 'queue_reorder_tracks',
        origin: 'qobuz_online',
        track_origins: [],
        payload: { queue_item_ids: [10, 30, 20] }
      }
    });
  });

  it('routes remove to queue_remove_tracks when connected', async () => {
    setConnected(true);
    expect(await removeQconnectQueueItemsIfRemote([42])).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith('v2_qconnect_send_command_with_admission', {
      request: {
        command_type: 'queue_remove_tracks',
        origin: 'qobuz_online',
        track_origins: [],
        payload: { queue_item_ids: [42] }
      }
    });
  });

  it('routes clear to clear_queue with empty payload when connected', async () => {
    setConnected(true);
    expect(await clearQconnectQueueIfRemote()).toBe(true);
    expect(invokeMock).toHaveBeenCalledWith('v2_qconnect_send_command_with_admission', {
      request: {
        command_type: 'clear_queue',
        origin: 'qobuz_online',
        track_origins: [],
        payload: {}
      }
    });
  });

  it('returns false (local fallback) when NOT connected and never dispatches', async () => {
    setConnected(false);
    expect(await reorderQconnectQueueIfRemote([1, 2])).toBe(false);
    expect(await removeQconnectQueueItemsIfRemote([1])).toBe(false);
    expect(await clearQconnectQueueIfRemote()).toBe(false);
    expect(
      invokeMock.mock.calls.filter(([c]) => c === 'v2_qconnect_send_command_with_admission')
    ).toHaveLength(0);
  });
});
