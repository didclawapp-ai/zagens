import type { Locale, TranslationMap } from './keys';

/**
 * Interpolate `{{placeholder}}` tokens in a translation string.
 */
export function interpolate(template: string, params?: Record<string, string>): string {
  if (!params) return template;
  return template.replace(/\{\{(\w+)\}\}/g, (_, key: string) =>
    Object.prototype.hasOwnProperty.call(params, key) ? params[key] : `{{${key}}}`,
  );
}

/**
 * Detect the initial locale:
 *   localStorage > navigator.language > default (zh-Hans)
 */
const LOCALE_STORAGE_KEY = 'zagens-locale';
const LEGACY_LOCALE_STORAGE_KEY = 'ds-pick-locale';

export function detectLocale(defaultLocale: Locale = 'zh-Hans'): Locale {
  try {
    const stored =
      localStorage.getItem(LOCALE_STORAGE_KEY) ??
      localStorage.getItem(LEGACY_LOCALE_STORAGE_KEY);
    if (stored === 'zh-Hans' || stored === 'en') return stored;
  } catch {
    // localStorage unavailable (e.g. SSR / privacy mode) — ignore
  }

  try {
    const nav = navigator.language;
    if (nav.startsWith('zh')) return 'zh-Hans';
    if (nav.startsWith('en')) return 'en';
  } catch {
    // navigator unavailable
  }

  return defaultLocale;
}

/**
 * Persist the chosen locale.
 */
export function persistLocale(locale: Locale): void {
  try {
    localStorage.setItem(LOCALE_STORAGE_KEY, locale);
  } catch {
    // ignore
  }
}

/**
 * Look up a nested key like "sidebar.newSession" in a translation map.
 */
export function lookup(
  map: TranslationMap,
  key: string,
): string {
  const parts = key.split('.');
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let node: any = map;
  for (const part of parts) {
    if (node == null || typeof node !== 'object') return key;
    node = node[part];
  }
  return typeof node === 'string' ? node : key;
}
