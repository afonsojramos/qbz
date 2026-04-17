/**
 * Unlocking Store
 *
 * Tracks which tracks are currently being decrypted (offline CMAF bundle
 * unlock). The backend emits `offline:unlock_start` / `offline:unlock_end`
 * around `load_cmaf_bundle` calls. TrackRow subscribes and swaps the
 * play/equalizer glyph for an animated padlock while the id is in the
 * active set.
 *
 * IDs here are "display ids" — whatever the UI keys tracks by. For Qobuz
 * flow that's the Qobuz track id; for Local Library that's the library
 * row id. The backend helper decides which one to emit based on context,
 * so the same store handles both flows transparently.
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

const unlockingIds = new Set<number>();
// Brief "just unlocked" window so the UI can show an opened-padlock glyph
// between the end of decrypt and the first audio frame. Cleared on a
// per-id timeout — NOT reused across tracks.
const recentlyUnlockedIds = new Set<number>();
const recentlyUnlockedTimers = new Map<number, ReturnType<typeof setTimeout>>();
const UNLOCKED_HOLD_MS = 600;

const listeners = new Set<() => void>();
let backendUnlisteners: UnlistenFn[] = [];
let started = false;

function notify(): void {
  for (const listener of listeners) {
    try {
      listener();
    } catch {
      // never let a single subscriber break the fanout
    }
  }
}

function markRecentlyUnlocked(id: number): void {
  const existing = recentlyUnlockedTimers.get(id);
  if (existing) clearTimeout(existing);
  recentlyUnlockedIds.add(id);
  const timer = setTimeout(() => {
    recentlyUnlockedIds.delete(id);
    recentlyUnlockedTimers.delete(id);
    notify();
  }, UNLOCKED_HOLD_MS);
  recentlyUnlockedTimers.set(id, timer);
}

export function isUnlocking(trackId: number | null | undefined): boolean {
  if (trackId == null) return false;
  return unlockingIds.has(trackId);
}

export function isRecentlyUnlocked(trackId: number | null | undefined): boolean {
  if (trackId == null) return false;
  return recentlyUnlockedIds.has(trackId);
}

/**
 * True if ANY track is currently being decrypted. Used by the global
 * buffering toast to swap its label from "Buffering" to "Unlocking"
 * without needing to know which track triggered it.
 */
export function isAnyUnlocking(): boolean {
  return unlockingIds.size > 0;
}

/**
 * Subscribe to unlocking-state changes. Svelte 5 runes-friendly: call
 * from a $derived or $effect; the callback is invoked on every add /
 * remove event so the caller can re-evaluate `isUnlocking(id)`.
 */
export function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * Start listening for backend unlock events. Idempotent; subsequent
 * calls are no-ops. Called once during app boot.
 */
export async function startPolling(): Promise<void> {
  if (started) return;
  started = true;

  try {
    const stopStart = await listen<{ trackId: number }>(
      'offline:unlock_start',
      (event) => {
        const id = event.payload?.trackId;
        if (typeof id !== 'number') return;
        if (!unlockingIds.has(id)) {
          unlockingIds.add(id);
          notify();
        }
      }
    );
    const stopEnd = await listen<{ trackId: number; success?: boolean }>(
      'offline:unlock_end',
      (event) => {
        const id = event.payload?.trackId;
        if (typeof id !== 'number') return;
        const wasUnlocking = unlockingIds.delete(id);
        // Only show the "unlocked" flash on successful decrypt — on
        // failure the row should fall back to its normal glyph
        // immediately, no celebratory padlock.
        const success = event.payload?.success !== false;
        if (success) {
          markRecentlyUnlocked(id);
        }
        if (wasUnlocking || success) {
          notify();
        }
      }
    );
    backendUnlisteners = [stopStart, stopEnd];
  } catch (err) {
    console.error('[UnlockingStore] Failed to register listeners:', err);
    started = false;
  }
}

/**
 * Stop listening for backend events and clear state. Called on session
 * teardown so the next user doesn't see stale animations.
 */
export function stopPolling(): void {
  for (const unlisten of backendUnlisteners) {
    try {
      unlisten();
    } catch {
      // ignore; best-effort cleanup
    }
  }
  backendUnlisteners = [];
  unlockingIds.clear();
  for (const timer of recentlyUnlockedTimers.values()) clearTimeout(timer);
  recentlyUnlockedTimers.clear();
  recentlyUnlockedIds.clear();
  started = false;
  notify();
}
