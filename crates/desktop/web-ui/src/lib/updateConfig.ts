/**
 * Zagens OTA update endpoints (pre-wired; site not live yet).
 * Keep in sync with `crates/desktop/tauri.conf.json` → `plugins.updater.endpoints`.
 */
export const UPDATE_DOWNLOAD_BASE = 'https://zagens.com/download/';

/** Tauri updater static manifest (see https://v2.tauri.app/plugin/updater/). */
export const UPDATE_MANIFEST_URL = `${UPDATE_DOWNLOAD_BASE}latest.json`;
