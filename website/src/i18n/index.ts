export type Locale = 'en' | 'zh-Hans';

export const locales: Locale[] = ['en', 'zh-Hans'];

export const localeLabels: Record<Locale, string> = {
  en: 'English',
  'zh-Hans': '简体中文',
};

export function isLocale(value: string): value is Locale {
  return locales.includes(value as Locale);
}

export function getLocaleFromPath(pathname: string): Locale {
  if (pathname.startsWith('/zh-Hans')) return 'zh-Hans';
  return 'en';
}

export function localePath(locale: Locale, path: string): string {
  const normalized = path.startsWith('/') ? path : `/${path}`;
  const suffix = normalized === '/' ? '' : normalized;
  if (locale === 'en') return normalized === '/' ? '/' : normalized;
  return `/zh-Hans${suffix}`;
}

export { en } from './en';
export { zhHans } from './zh-Hans';

import { en } from './en';
import { zhHans } from './zh-Hans';

export type SiteCopy = typeof en;

const copyByLocale: Record<Locale, SiteCopy> = {
  en,
  'zh-Hans': zhHans,
};

export function getCopy(locale: Locale): SiteCopy {
  return copyByLocale[locale];
}
