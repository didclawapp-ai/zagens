import { useEffect, useState, useCallback } from 'react';
import { useT } from '../i18n';

/**
 * Composer top-bar tri-state LHT override: auto → strict → off → auto.
 * Persisted to `settings.toml` as `lht_composer_mode`; read live by engine
 * spawn (next turn, no restart). `localStorage` mirrors for instant paint.
 */
export type LhtComposerMode = 'auto' | 'strict' | 'off';

const LHT_MODE_STORAGE_KEY = 'zagens-lht-composer-mode';

const CYCLE: LhtComposerMode[] = ['auto', 'strict', 'off'];

function readStored(): LhtComposerMode {
  try {
    const v = localStorage.getItem(LHT_MODE_STORAGE_KEY);
    if (v === 'strict' || v === 'off' || v === 'auto') return v;
  } catch {
    /* ignore */
  }
  return 'auto';
}

function persistLocal(mode: LhtComposerMode): void {
  try {
    localStorage.setItem(LHT_MODE_STORAGE_KEY, mode);
  } catch {
    /* ignore */
  }
}

async function readRuntimeMode(): Promise<LhtComposerMode | null> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const raw = await invoke<string>('get_lht_composer_mode');
    if (raw === 'strict' || raw === 'off' || raw === 'auto') return raw;
    return 'auto';
  } catch {
    return null;
  }
}

async function writeRuntimeMode(mode: LhtComposerMode): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_lht_composer_mode', { mode });
  } catch {
    /* browser dev */
  }
}

interface Props {
  disabled?: boolean;
}

export default function LhtModeToggle({ disabled = false }: Props) {
  const { t } = useT();
  const [mode, setMode] = useState<LhtComposerMode>(readStored);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const runtime = await readRuntimeMode();
      if (!cancelled && runtime != null) {
        setMode(runtime);
        persistLocal(runtime);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const cycle = useCallback(() => {
    setMode((prev) => {
      const idx = CYCLE.indexOf(prev);
      const next = CYCLE[(idx + 1) % CYCLE.length] ?? 'auto';
      persistLocal(next);
      void writeRuntimeMode(next);
      return next;
    });
  }, []);

  const label =
    mode === 'strict'
      ? t('composer.lhtModeStrictLabel')
      : mode === 'off'
        ? t('composer.lhtModeDisabledLabel')
        : t('composer.lhtModeLabel');

  const title =
    mode === 'strict'
      ? t('composer.lhtModeStrictHint')
      : mode === 'off'
        ? t('composer.lhtModeDisabledHint')
        : t('composer.lhtModeAutoHint');

  const chipClass =
    mode === 'strict'
      ? 'composer-chip active text-accent'
      : mode === 'off'
        ? 'composer-chip text-t-text-muted opacity-60 line-through'
        : 'composer-chip text-t-text-muted';

  return (
    <button
      type="button"
      disabled={disabled}
      onClick={cycle}
      aria-pressed={mode !== 'off'}
      aria-label={label}
      title={title}
      className={chipClass}
    >
      {label}
    </button>
  );
}
