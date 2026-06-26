import { useEffect, useState, useCallback } from 'react';
import { fetchThreadConfig, putThreadConfig } from '../api/client';
import { useT } from '../i18n';

export const LHT_COMPOSER_MODE_CHANGED_EVENT = 'zagens-lht-composer-mode-changed';

/**
 * Composer top-bar tri-state LHT override: auto → strict → off → auto.
 * With `threadId`, persisted via per-session overlay (zero sidecar restart).
 * Without `threadId`, falls back to `settings.toml` / Tauri IPC.
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

async function writeGlobalRuntimeMode(mode: LhtComposerMode): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_lht_composer_mode', { mode });
  } catch {
    /* browser dev */
  }
}

function dispatchModeChanged(mode: LhtComposerMode): void {
  window.dispatchEvent(
    new CustomEvent(LHT_COMPOSER_MODE_CHANGED_EVENT, { detail: mode }),
  );
}

interface Props {
  disabled?: boolean;
  /** When set, mode writes go to per-session overlay (no sidecar restart). */
  threadId?: string | null;
}

export default function LhtModeToggle({ disabled = false, threadId = null }: Props) {
  const { t } = useT();
  const [mode, setMode] = useState<LhtComposerMode>(readStored);
  const sessionScoped = Boolean(threadId?.trim());

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      if (sessionScoped && threadId?.trim()) {
        try {
          const cfg = await fetchThreadConfig(threadId.trim());
          const raw = cfg.effective.lht_composer_mode;
          const resolved: LhtComposerMode =
            raw === 'strict' || raw === 'off' || raw === 'auto' ? raw : 'auto';
          if (!cancelled) {
            setMode(resolved);
            persistLocal(resolved);
          }
          return;
        } catch {
          /* fall through to global */
        }
      }
      const runtime = await readRuntimeMode();
      if (!cancelled && runtime != null) {
        setMode(runtime);
        persistLocal(runtime);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [sessionScoped, threadId]);

  useEffect(() => {
    const onComposerModeChanged = (event: Event) => {
      const detail = (event as CustomEvent<LhtComposerMode>).detail;
      if (detail === 'strict' || detail === 'off' || detail === 'auto') {
        setMode(detail);
        persistLocal(detail);
      }
    };
    window.addEventListener(LHT_COMPOSER_MODE_CHANGED_EVENT, onComposerModeChanged);
    return () => window.removeEventListener(LHT_COMPOSER_MODE_CHANGED_EVENT, onComposerModeChanged);
  }, []);

  const cycle = useCallback(() => {
    setMode((prev) => {
      const idx = CYCLE.indexOf(prev);
      const next = CYCLE[(idx + 1) % CYCLE.length] ?? 'auto';
      persistLocal(next);
      void (async () => {
        if (sessionScoped && threadId?.trim()) {
          await putThreadConfig(threadId.trim(), { lht_composer_mode: next });
        } else {
          await writeGlobalRuntimeMode(next);
        }
        dispatchModeChanged(next);
      })();
      return next;
    });
  }, [sessionScoped, threadId]);

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
        : sessionScoped
          ? t('composer.lhtModeAutoHintSession')
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
