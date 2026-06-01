import { useEffect, useState, useCallback } from 'react';
import { useT } from '../i18n';

/**
 * Composer top-bar toggle for LHT (long-horizon) strict mode. Global (not
 * per-session): persisted to `settings.toml` via the `set_lht_strict` Tauri
 * command and read live by the sidecar engine spawn, so it takes effect on the
 * next turn without a restart. `localStorage` mirrors the value for instant UI
 * paint and browser dev mode (where Tauri is unavailable).
 */
const LHT_STRICT_STORAGE_KEY = 'zagens-lht-strict';

function readStored(): boolean {
  try {
    return localStorage.getItem(LHT_STRICT_STORAGE_KEY) === '1';
  } catch {
    return false;
  }
}

function persistLocal(enabled: boolean): void {
  try {
    localStorage.setItem(LHT_STRICT_STORAGE_KEY, enabled ? '1' : '0');
  } catch {
    /* ignore quota / disabled storage */
  }
}

async function readRuntimeLhtStrict(): Promise<boolean | null> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke<boolean>('get_lht_strict');
  } catch {
    return null; // browser dev / Tauri unavailable
  }
}

async function writeRuntimeLhtStrict(enabled: boolean): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_lht_strict', { enabled });
  } catch {
    /* browser dev / Tauri unavailable — UI-only */
  }
}

interface Props {
  disabled?: boolean;
}

export default function LhtModeToggle({ disabled = false }: Props) {
  const { t } = useT();
  const [strict, setStrict] = useState<boolean>(readStored);

  // Reconcile with the persisted runtime value on mount (settings.toml wins).
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const runtime = await readRuntimeLhtStrict();
      if (!cancelled && runtime != null) {
        setStrict(runtime);
        persistLocal(runtime);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const toggle = useCallback(() => {
    setStrict((prev) => {
      const next = !prev;
      persistLocal(next);
      void writeRuntimeLhtStrict(next);
      return next;
    });
  }, []);

  return (
    <button
      type="button"
      disabled={disabled}
      onClick={toggle}
      aria-pressed={strict}
      title={strict ? t('composer.lhtModeOnHint') : t('composer.lhtModeOffHint')}
      className={`composer-chip ${strict ? 'active text-accent' : 'text-t-text-muted'}`}
    >
      {strict ? t('composer.lhtModeOnLabel') : t('composer.lhtModeLabel')}
    </button>
  );
}
