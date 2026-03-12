import {
  setLocalTrackIds,
  setQueue,
  type BackendQueueTrack
} from '$lib/stores/queueStore';
import { loadQconnectQueue } from '$lib/services/trackActions';

export type QconnectQueueSyncAssessment = {
  syncable: boolean;
  reason: 'ok' | 'empty_queue' | 'queue_contains_non_qobuz_tracks';
  trackIds: number[];
  blockedTrackIds: number[];
};

export type ReplacePlaybackQueueOptions = {
  clearLocal?: boolean;
  localTrackIds?: number[];
  syncQconnect?: boolean;
  debugLabel?: string;
};

function isQconnectSyncEligibleTrack(track: BackendQueueTrack): boolean {
  if (track.is_local) return false;

  const source = (track.source ?? '').toLowerCase();
  if (source === 'local' || source === 'plex' || source === 'qobuz_download') {
    return false;
  }

  return Number.isFinite(track.id) && track.id > 0;
}

export function assessQconnectQueueSync(tracks: BackendQueueTrack[]): QconnectQueueSyncAssessment {
  if (tracks.length === 0) {
    return {
      syncable: false,
      reason: 'empty_queue',
      trackIds: [],
      blockedTrackIds: []
    };
  }

  const trackIds: number[] = [];
  const blockedTrackIds: number[] = [];

  for (const track of tracks) {
    if (isQconnectSyncEligibleTrack(track)) {
      trackIds.push(track.id);
    } else {
      blockedTrackIds.push(track.id);
    }
  }

  if (blockedTrackIds.length > 0) {
    return {
      syncable: false,
      reason: 'queue_contains_non_qobuz_tracks',
      trackIds: [],
      blockedTrackIds
    };
  }

  return {
    syncable: true,
    reason: 'ok',
    trackIds,
    blockedTrackIds: []
  };
}

export async function syncQconnectQueueFromTracks(
  tracks: BackendQueueTrack[],
  startIndex: number,
  debugLabel: string = 'queue-replace'
): Promise<boolean> {
  const assessment = assessQconnectQueueSync(tracks);

  if (!assessment.syncable) {
    console.log('[QConnect/QueueSync] skipped %s: reason=%s blockedTrackIds=%o', debugLabel, assessment.reason, assessment.blockedTrackIds);
    return false;
  }

  console.log('[QConnect/QueueSync] syncing %s: trackCount=%d startIndex=%d', debugLabel, assessment.trackIds.length, startIndex);
  return loadQconnectQueue(assessment.trackIds, startIndex);
}

export async function replacePlaybackQueue(
  tracks: BackendQueueTrack[],
  startIndex: number,
  options: ReplacePlaybackQueueOptions = {}
): Promise<boolean> {
  const {
    clearLocal = true,
    localTrackIds = [],
    syncQconnect = true,
    debugLabel = 'queue-replace'
  } = options;

  const success = await setQueue(tracks, startIndex, clearLocal);
  if (!success) {
    return false;
  }

  if (localTrackIds.length > 0) {
    setLocalTrackIds(localTrackIds);
  }

  if (syncQconnect) {
    await syncQconnectQueueFromTracks(tracks, startIndex, debugLabel);
  }

  return true;
}
