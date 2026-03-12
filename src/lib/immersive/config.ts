/**
 * Immersive Rendering Configuration
 *
 * Handles build-time and runtime feature flags for the immersive system.
 * This module is the single source of truth for "should immersive be active?"
 */

import { DEFAULT_IMMERSIVE_CONFIG, type ImmersiveConfig } from './types';

/** Storage key for runtime enable/disable preference */
const STORAGE_KEY_ENABLED = 'qbz.immersive.enabled';
const STORAGE_KEY_CONFIG = 'qbz.immersive.config';

/**
 * Build-time flag: Is immersive compiled into this build?
 *
 * Set via environment variable at build time:
 *   QBZ_IMMERSIVE=false pnpm build
 *
 * When false, all immersive code should be tree-shaken out.
 */
export const BUILD_IMMERSIVE_ENABLED: boolean =
  import.meta.env.VITE_IMMERSIVE_ENABLED !== 'false';

/**
 * Check if WebGL2 is available in the current environment.
 * This is a capability check, not a preference check.
 */
export function isWebGL2Available(): boolean {
  if (typeof document === 'undefined') {
    // SSR or headless - no WebGL
    return false;
  }

  try {
    const canvas = document.createElement('canvas');
    const gl = canvas.getContext('webgl2');
    return gl !== null;
  } catch {
    return false;
  }
}

/**
 * Get detailed WebGL2 information for debugging.
 * Returns null if WebGL2 is unavailable.
 */
export function getWebGL2Info(): { renderer: string; vendor: string; version: string } | null {
  if (typeof document === 'undefined') return null;

  try {
    const canvas = document.createElement('canvas');
    const gl = canvas.getContext('webgl2');
    if (!gl) return null;

    const debugInfo = gl.getExtension('WEBGL_debug_renderer_info');
    return {
      renderer: debugInfo
        ? gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL)
        : gl.getParameter(gl.RENDERER),
      vendor: debugInfo
        ? gl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL)
        : gl.getParameter(gl.VENDOR),
      version: gl.getParameter(gl.VERSION),
    };
  } catch {
    return null;
  }
}

/**
 * Check if immersive is enabled at runtime (user preference).
 * Only meaningful if BUILD_IMMERSIVE_ENABLED is true.
 */
export function isRuntimeEnabled(): boolean {
  if (!BUILD_IMMERSIVE_ENABLED) return false;
  if (typeof localStorage === 'undefined') return true; // Default to enabled

  const stored = localStorage.getItem(STORAGE_KEY_ENABLED);
  return stored !== 'false'; // Default to enabled if not set
}

/**
 * Set runtime enabled state (user preference).
 */
export function setRuntimeEnabled(enabled: boolean): void {
  if (typeof localStorage === 'undefined') return;
  localStorage.setItem(STORAGE_KEY_ENABLED, enabled ? 'true' : 'false');
}

/**
 * Get the current immersive configuration.
 * Includes migration from legacy disableBlurBackground to backgroundMode.
 */
export function getConfig(): ImmersiveConfig {
  if (typeof localStorage === 'undefined') return { ...DEFAULT_IMMERSIVE_CONFIG };

  try {
    const stored = localStorage.getItem(STORAGE_KEY_CONFIG);
    if (!stored) return { ...DEFAULT_IMMERSIVE_CONFIG };

    const parsed = JSON.parse(stored);
    const config = { ...DEFAULT_IMMERSIVE_CONFIG, ...parsed };

    // Migration: disableBlurBackground → backgroundMode
    if (!parsed.backgroundMode) {
      config.backgroundMode = parsed.disableBlurBackground ? 'off' : 'full';
    }

    return config;
  } catch {
    return { ...DEFAULT_IMMERSIVE_CONFIG };
  }
}

/**
 * Update immersive configuration.
 */
export function setConfig(config: Partial<ImmersiveConfig>): void {
  if (typeof localStorage === 'undefined') return;

  const current = getConfig();
  const updated = { ...current, ...config };
  localStorage.setItem(STORAGE_KEY_CONFIG, JSON.stringify(updated));
}

/**
 * Check if immersive should be available (all conditions met).
 * This is the main entry point for "can we use immersive?"
 */
export function shouldImmersiveBeAvailable(): {
  available: boolean;
  reason?: 'build-disabled' | 'runtime-disabled' | 'no-webgl2';
} {
  if (!BUILD_IMMERSIVE_ENABLED) {
    return { available: false, reason: 'build-disabled' };
  }

  if (!isRuntimeEnabled()) {
    return { available: false, reason: 'runtime-disabled' };
  }

  if (!isWebGL2Available()) {
    return { available: false, reason: 'no-webgl2' };
  }

  return { available: true };
}
