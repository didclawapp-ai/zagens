import type { Locale, TranslationMap } from './keys';
import { DEFAULT_LOCALE, SUPPORTED_LOCALES } from './keys';

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
 *   localStorage (explicit user choice) > system languages > English
 */
const LOCALE_STORAGE_KEY = 'zagens-locale';
const LEGACY_LOCALE_STORAGE_KEY = 'ds-pick-locale';

function isLocale(value: string | null): value is Locale {
  return value != null && (SUPPORTED_LOCALES as readonly string[]).includes(value);
}

/** Normalize a BCP-47 tag for prefix / region matching. */
function normalizeLanguageTag(tag: string): string {
  return tag.trim().toLowerCase().replace(/_/g, '-');
}

/**
 * Map one system language tag to a supported locale, or null when no pack exists.
 * Only exact supported variants match — e.g. zh-TW has no pack and returns null.
 */
export function matchLocaleFromTag(tag: string): Locale | null {
  const norm = normalizeLanguageTag(tag);
  if (!norm) return null;

  if (norm === 'zh-hans' || norm === 'zh-cn' || norm === 'zh-sg') return 'zh-Hans';
  if (norm.startsWith('zh')) return null;

  if (norm.startsWith('ja')) return 'ja';

  if (norm === 'pt-br' || norm.startsWith('pt-br') || norm.startsWith('pt')) return 'pt-BR';

  if (norm.startsWith('en')) return 'en';

  return null;
}

/**
 * Resolve locale from an ordered list of system language tags (e.g. navigator.languages).
 */
export function detectLocaleFromSystem(
  languageTags: readonly string[],
  fallback: Locale = DEFAULT_LOCALE,
): Locale {
  for (const tag of languageTags) {
    const matched = matchLocaleFromTag(tag);
    if (matched) return matched;
  }
  return fallback;
}

function readSystemLanguageTags(): string[] {
  try {
    if (typeof navigator !== 'undefined') {
      if (navigator.languages?.length) return [...navigator.languages];
      if (navigator.language) return [navigator.language];
    }
  } catch {
    // navigator unavailable
  }
  return [];
}

export function detectLocale(fallback: Locale = DEFAULT_LOCALE): Locale {
  try {
    const stored =
      localStorage.getItem(LOCALE_STORAGE_KEY) ??
      localStorage.getItem(LEGACY_LOCALE_STORAGE_KEY);
    if (isLocale(stored)) return stored;
  } catch {
    // localStorage unavailable (e.g. SSR / privacy mode) — ignore
  }

  return detectLocaleFromSystem(readSystemLanguageTags(), fallback);
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
