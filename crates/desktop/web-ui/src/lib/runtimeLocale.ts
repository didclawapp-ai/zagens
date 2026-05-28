import type { Locale } from '../i18n/keys';
import { SUPPORTED_LOCALES } from '../i18n/keys';
import { detectLocaleFromSystem, persistLocale } from '../i18n/utils';

const LOCALE_STORAGE_KEY = 'zagens-locale';
const LEGACY_LOCALE_STORAGE_KEY = 'ds-pick-locale';

function isLocale(value: string | null | undefined): value is Locale {
  return value != null && (SUPPORTED_LOCALES as readonly string[]).includes(value);
}

function readStoredUiLocale(): Locale | null {
  try {
    const stored =
      localStorage.getItem(LOCALE_STORAGE_KEY) ??
      localStorage.getItem(LEGACY_LOCALE_STORAGE_KEY);
    return isLocale(stored) ? stored : null;
  } catch {
    return null;
  }
}

/** Persist UI locale to `settings.toml` so the runtime system prompt uses the same language. */
export async function syncLocaleToRuntime(locale: Locale): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_app_locale', { locale });
  } catch {
    // Browser dev mode or Tauri unavailable — UI-only locale.
  }
}

/** On desktop startup, align UI locale with runtime settings and vice versa. */
export async function reconcileRuntimeLocale(
  applyLocale: (locale: Locale) => void,
): Promise<void> {
  let runtimeLocale: string | null = null;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    runtimeLocale = await invoke<string>('get_locale');
  } catch {
    return;
  }

  const storedUi = readStoredUiLocale();
  const normalizedRuntime = runtimeLocale?.trim().toLowerCase();
  const runtimeExplicit =
    normalizedRuntime &&
    normalizedRuntime !== 'auto' &&
    normalizedRuntime !== 'system' &&
    isLocale(runtimeLocale!.trim())
      ? (runtimeLocale!.trim() as Locale)
      : null;

  if (storedUi) {
    applyLocale(storedUi);
    await syncLocaleToRuntime(storedUi);
    return;
  }

  if (runtimeExplicit) {
    applyLocale(runtimeExplicit);
    persistLocale(runtimeExplicit);
    return;
  }

  const detected = detectLocaleFromSystem(
    typeof navigator !== 'undefined'
      ? navigator.languages?.length
        ? [...navigator.languages]
        : navigator.language
          ? [navigator.language]
          : []
      : [],
  );
  applyLocale(detected);
  persistLocale(detected);
  await syncLocaleToRuntime(detected);
}
