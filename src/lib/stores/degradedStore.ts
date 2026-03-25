/**
 * Tracks Qobuz service degradation (repeated 504/server errors).
 * Shows a visual indicator when the service is having issues.
 * Auto-clears after 10 minutes without new errors.
 */

let degraded = false;
let lastErrorTime = 0;
let clearTimer: ReturnType<typeof setTimeout> | null = null;
const CLEAR_TIMEOUT_MS = 10 * 60 * 1000; // 10 minutes
const listeners = new Set<() => void>();

function notifyListeners() {
  listeners.forEach(fn => fn());
}

/**
 * Report a server error (504, 502, 503) from Qobuz.
 * Call this when QualityExhausted or server errors are detected.
 */
export function reportServerError(): void {
  lastErrorTime = Date.now();

  if (!degraded) {
    degraded = true;
    notifyListeners();
  }

  // Reset the clear timer
  if (clearTimer) clearTimeout(clearTimer);
  clearTimer = setTimeout(() => {
    degraded = false;
    clearTimer = null;
    notifyListeners();
  }, CLEAR_TIMEOUT_MS);
}

/**
 * Check if Qobuz service is currently degraded.
 */
export function isDegraded(): boolean {
  return degraded;
}

/**
 * Subscribe to degraded state changes.
 */
export function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * Clear degraded state (e.g., when going fully offline).
 */
export function clearDegraded(): void {
  if (clearTimer) clearTimeout(clearTimer);
  clearTimer = null;
  degraded = false;
  notifyListeners();
}
