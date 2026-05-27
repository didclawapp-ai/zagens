/** Type system for Zagens i18n — derived from the zh-Hans translation source of truth. */

import type zhHans from './locales/zh-Hans';

/** Widens literal string types to plain `string` so locale files can hold different text. */
type Widen<T> = {
  [K in keyof T]: T[K] extends string
    ? string
    : T[K] extends object
      ? Widen<T[K]>
      : T[K];
};

/** Shape all locale files must conform to (same key structure as zh-Hans, values are `string`). */
export type TranslationMap = Widen<typeof zhHans>;

export type Locale = 'zh-Hans' | 'en' | 'ja' | 'pt-BR';

/** Locales with a shipped desktop Web UI translation pack. */
export const SUPPORTED_LOCALES: readonly Locale[] = ['zh-Hans', 'en', 'ja', 'pt-BR'];

/** Fallback when no stored preference and no system locale matches a pack. */
export const DEFAULT_LOCALE: Locale = 'en';

/** Locale display names for UI selectors. */
export const LOCALE_LABELS: Record<Locale, string> = {
  'zh-Hans': '中文',
  en: 'English',
  ja: '日本語',
  'pt-BR': 'Português (Brasil)',
};

// ── deep dot-path key type (simplified, depth-limited to 3) ────────

type PathsToDepth<T, D extends number = 3> = D extends 0
  ? never
  : {
      [K in keyof T & string]: T[K] extends string
        ? K
        : T[K] extends object
          ? `${K}.${PathsToDepth<T[K], PrevDepth<D>>}`
          : K;
    }[keyof T & string];

type PrevDepthTable = [never, 0, 1, 2, 3];
type PrevDepth<D extends number> = PrevDepthTable[D];

/** All valid translation keys, e.g. `"sidebar.newSession"` or `"banner.loadSessionsError"`. */
export type TranslationKey = PathsToDepth<TranslationMap>;

// ── interpolation params (for reference) ────────────────────────────

export type InterpolateParams = Record<string, string>;
