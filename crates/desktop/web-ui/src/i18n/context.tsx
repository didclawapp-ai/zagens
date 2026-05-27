import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import type { Locale, TranslationMap } from './keys';
import { DEFAULT_LOCALE, LOCALE_LABELS } from './keys';
import { detectLocale, interpolate, lookup, persistLocale } from './utils';

// ── lazy-load locale packs (Vite tree-shakes unused) ────────────────

const localeLoaders: Record<Locale, () => Promise<{ default: TranslationMap }>> = {
  'zh-Hans': () => import('./locales/zh-Hans'),
  en: () => import('./locales/en'),
  ja: () => import('./locales/ja'),
  'pt-BR': () => import('./locales/pt-BR'),
};

// ── context shape ───────────────────────────────────────────────────

interface I18nContextValue {
  locale: Locale;
  setLocale: (loc: Locale) => void;
  t: (key: string, params?: Record<string, string>) => string;
  /** true while a locale pack is being loaded */
  pending: boolean;
}

const I18nContext = createContext<I18nContextValue | null>(null);

// ── provider ────────────────────────────────────────────────────────

export function I18nProvider({
  children,
  defaultLocale,
}: {
  children: React.ReactNode;
  defaultLocale?: Locale;
}) {
  const [locale, setLocaleState] = useState<Locale>(() => detectLocale(defaultLocale ?? DEFAULT_LOCALE));
  const [messages, setMessages] = useState<TranslationMap | null>(null);
  const [pending, setPending] = useState(true);

  // load locale pack
  useEffect(() => {
    let cancelled = false;
    setPending(true);
    localeLoaders[locale]()
      .then((mod) => {
        if (!cancelled) {
          setMessages(mod.default);
          setPending(false);
        }
      })
      .catch(() => {
        if (!cancelled) setPending(false);
      });
    return () => {
      cancelled = true;
    };
  }, [locale]);

  const setLocale = useCallback((loc: Locale) => {
    persistLocale(loc);
    setLocaleState(loc);
  }, []);

  const t = useCallback(
    (key: string, params?: Record<string, string>) => {
      if (!messages) return key; // fallback while loading
      const raw = lookup(messages, key);
      return interpolate(raw, params);
    },
    [messages],
  );

  const value = useMemo<I18nContextValue>(
    () => ({ locale, setLocale, t, pending }),
    [locale, setLocale, t, pending],
  );

  return React.createElement(I18nContext.Provider, { value }, children);
}

// ── consumer hook ───────────────────────────────────────────────────

export function useT(): I18nContextValue {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error('useT() must be used inside <I18nProvider>');
  return ctx;
}

/** For UI that only needs locale labels without translations (e.g. language selector). */
export { LOCALE_LABELS };
