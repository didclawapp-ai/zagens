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

/** Product gate modes each preset writes into config.toml. */
export const GATES_FOR_PRESET: Record<
  LhtPresetId,
  { auto_verify_replay: LhtGateMode; toolchain_gate: LhtGateMode; stub_gate: LhtGateMode } | null
> = {
  'code-default': {
    auto_verify_replay: 'observe',
    toolchain_gate: 'observe',
    stub_gate: 'observe',
  },
  'long-fix': {
    auto_verify_replay: 'enforce',
    toolchain_gate: 'enforce',
    stub_gate: 'enforce',
  },
  'long-refactor': {
    auto_verify_replay: 'enforce',
    toolchain_gate: 'enforce',
    stub_gate: 'enforce',
  },
  // LHT off — gate modes are soft defaults; not part of the match signal.
  'craft-audit': null,
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

function gatesMatchPreset(settings: LhtSettings, presetId: LhtPresetId): boolean {
  const expected = GATES_FOR_PRESET[presetId];
  if (!expected) return true;
  return (
    settings.auto_verify_replay === expected.auto_verify_replay &&
    settings.toolchain_gate === expected.toolchain_gate &&
    settings.stub_gate === expected.stub_gate
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
    // Prefer long-fix when product gates are already hard-enforce.
    if (
      settings.auto_verify_replay === 'enforce' &&
      settings.toolchain_gate === 'enforce' &&
      settings.stub_gate === 'enforce'
    ) {
      return 'long-fix';
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
  return matchPresetFromSettings(settings) === presetId && gatesMatchPreset(settings, presetId);
}
