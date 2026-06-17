import type { LhtComposerMode } from '../api/client';
import type { LhtGateMode, LhtPresetId, LhtSettings } from '../api/client';

export type LhtPresetMatch = LhtPresetId | 'custom';

const LAST_PRESET_STORAGE_KEY = 'zagens-lht-last-preset';

/** Composer override that pairs with each harness preset. */
export const COMPOSER_MODE_FOR_PRESET: Record<LhtPresetId, LhtComposerMode> = {
  'code-default': 'auto',
  'long-fix': 'auto',
  'long-refactor': 'strict',
  'craft-audit': 'off',
};

export function rememberLastPreset(presetId: LhtPresetId): void {
  try {
    localStorage.setItem(LAST_PRESET_STORAGE_KEY, presetId);
  } catch {
    /* ignore */
  }
}

export function readLastPreset(): LhtPresetId | null {
  try {
    const raw = localStorage.getItem(LAST_PRESET_STORAGE_KEY);
    if (
      raw === 'code-default' ||
      raw === 'long-fix' ||
      raw === 'long-refactor' ||
      raw === 'craft-audit'
    ) {
      return raw;
    }
  } catch {
    /* ignore */
  }
  return null;
}

function gatesAreObserve(settings: LhtSettings): boolean {
  return (
    settings.auto_verify_replay === 'observe' &&
    settings.toolchain_gate === 'observe' &&
    settings.stub_gate === 'observe'
  );
}

/** Best-effort match of saved config to a known harness preset. */
export function matchPresetFromSettings(settings: LhtSettings): LhtPresetMatch {
  if (!settings.enabled && !settings.macro_loop_enabled) {
    return 'craft-audit';
  }

  if (
    settings.enabled &&
    settings.mode === 'strict' &&
    settings.macro_loop_enabled &&
    settings.auto_continue &&
    settings.macro_loop_auto_enter_craft === 'on_graph_complete'
  ) {
    return 'long-refactor';
  }

  if (
    settings.enabled &&
    settings.mode === 'auto' &&
    !settings.macro_loop_enabled &&
    !settings.auto_continue
  ) {
    const last = readLastPreset();
    if (last === 'long-fix' || last === 'code-default') {
      return last;
    }
    return 'code-default';
  }

  return 'custom';
}

export function effectiveLhtEnabled(
  settings: LhtSettings,
  composerMode: LhtComposerMode,
): boolean {
  if (composerMode === 'off') return false;
  if (composerMode === 'strict') return true;
  return settings.enabled;
}

export function effectiveLhtMode(
  settings: LhtSettings,
  composerMode: LhtComposerMode,
): 'auto' | 'strict' {
  if (composerMode === 'strict') return 'strict';
  return settings.mode;
}

export function summarizeGateModes(settings: LhtSettings): 'off' | 'observe' | 'enforce' | 'mixed' {
  const modes = new Set<LhtGateMode>([
    settings.auto_verify_replay,
    settings.toolchain_gate,
    settings.stub_gate,
  ]);
  if (modes.size === 1) {
    const only = [...modes][0];
    return only;
  }
  if (modes.has('enforce')) return 'enforce';
  if (modes.has('observe')) return 'observe';
  return 'mixed';
}

export function settingsMatchPreset(settings: LhtSettings, presetId: LhtPresetId): boolean {
  return matchPresetFromSettings(settings) === presetId && gatesAreObserve(settings);
}
