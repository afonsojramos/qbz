// Test mock for SvelteKit's `$app/environment`.
// The vitest config aliases `$app` to this directory, but the module was never
// created, so any test whose import graph reaches `$app/environment` (e.g. via
// the i18n module) failed to load. This provides the minimal surface the app uses.
export const browser = false;
export const dev = false;
export const building = false;
export const version = 'test';
