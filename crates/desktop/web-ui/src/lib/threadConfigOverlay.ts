import type { LhtComposerMode, LhtSettings, SystemSettings } from '../api/client';

/** Mirrors runtime `ThreadConfigOverlay` JSON (session-scoped). */
export interface ThreadConfigOverlay {
  long_horizon?: LongHorizonOverlay | null;
  lht_composer_mode?: string | null;
  /** Feature flags table (`#[serde(flatten)]` map: web_search / exec_policy / subagents). */
  features?: Record<string, boolean> | null;
  compaction?: { auto_compact?: boolean | null; token_threshold?: number | null } | null;
  memory?: { enabled?: boolean | null } | null;
  topic_memory?: {
    enabled?: boolean | null;
    inject_interval?: number | null;
    graph_path?: string | null;
    attribution?: string | null;
  } | null;
  lsp?: {
    enabled?: boolean | null;
    poll_after_edit_ms?: number | null;
    max_diagnostics_per_file?: number | null;
    include_warnings?: boolean | null;
  } | null;
  snapshots?: {
    enabled?: boolean | null;
    max_age_days?: number | null;
    max_workspace_gb?: number | null;
  } | null;
  approval_policy?: string | null;
}

interface LongHorizonOverlay {
  enabled?: boolean | null;
  mode?: string | null;
  progress_via_git?: boolean | null;
  max_nudges_per_item?: number | null;
  blocked_nudges_without_progress?: number | null;
  auto_continue?: boolean | null;
  max_auto_continue_rounds?: number | null;
  completion_gate?: {
    auto_verify_replay?: string | null;
    toolchain_gate?: string | null;
    stub_gate?: string | null;
    max_manifest_rounds?: number | null;
    max_audit_rounds?: number | null;
    max_infra_strikes?: number | null;
  } | null;
  macro_loop?: {
    enabled?: boolean | null;
    max_macro_cycles?: number | null;
    max_craft_rounds_per_cycle?: number | null;
    auto_enter_craft?: string | null;
    craft_on_small_tasks?: boolean | null;
    min_checklist_items_for_craft?: number | null;
  } | null;
}

export interface ThreadConfigResponse {
  /** Global baseline view (no overlay) — compare against to label inherited vs overridden. */
  base: ThreadConfigOverlay;
  overlay?: ThreadConfigOverlay | null;
  effective: ThreadConfigOverlay;
}

export function overlayHasSessionOverrides(overlay?: ThreadConfigOverlay | null): boolean {
  if (!overlay) return false;
  if (overlay.lht_composer_mode != null) return true;
  return overlay.long_horizon != null;
}

/** True when the overlay carries any of the System-settings-scoped sections. */
export function overlayHasSystemOverrides(overlay?: ThreadConfigOverlay | null): boolean {
  if (!overlay) return false;
  return (
    overlay.approval_policy != null ||
    overlay.features != null ||
    overlay.compaction != null ||
    overlay.memory != null ||
    overlay.topic_memory != null ||
    overlay.lsp != null ||
    overlay.snapshots != null
  );
}

/** Top-level overlay sections written by the System settings panel (for clear). */
export const SYSTEM_OVERLAY_SECTIONS = [
  'features',
  'compaction',
  'memory',
  'topic_memory',
  'lsp',
  'snapshots',
  'approval_policy',
] as const;

/** Map the session-scoped subset of `SystemSettings` → overlay patch. */
export function systemSettingsToOverlay(s: SystemSettings): ThreadConfigOverlay {
  return {
    approval_policy: s.approval_policy,
    features: {
      web_search: s.web_search,
      exec_policy: s.exec_policy,
      subagents: s.subagents_enabled,
    },
    compaction: {
      auto_compact: s.auto_compact,
      token_threshold: s.compaction_threshold_tokens,
    },
    memory: { enabled: s.memory_enabled },
    topic_memory: {
      enabled: s.topic_memory_enabled,
      inject_interval: s.topic_memory_inject_interval,
    },
    lsp: { enabled: s.lsp_enabled },
    snapshots: { enabled: s.snapshots_enabled },
  };
}

/** Overlay effective view onto a global `SystemSettings` baseline (session-scoped fields only). */
export function applyEffectiveOverlayToSystemSettings(
  effective: ThreadConfigOverlay,
  base: SystemSettings,
): SystemSettings {
  const feat = effective.features ?? {};
  return {
    ...base,
    approval_policy: effective.approval_policy ?? base.approval_policy,
    web_search: feat.web_search ?? base.web_search,
    exec_policy: feat.exec_policy ?? base.exec_policy,
    subagents_enabled: feat.subagents ?? base.subagents_enabled,
    auto_compact: effective.compaction?.auto_compact ?? base.auto_compact,
    compaction_threshold_tokens:
      effective.compaction?.token_threshold ?? base.compaction_threshold_tokens,
    memory_enabled: effective.memory?.enabled ?? base.memory_enabled,
    topic_memory_enabled: effective.topic_memory?.enabled ?? base.topic_memory_enabled,
    topic_memory_inject_interval:
      effective.topic_memory?.inject_interval ?? base.topic_memory_inject_interval,
    lsp_enabled: effective.lsp?.enabled ?? base.lsp_enabled,
    snapshots_enabled: effective.snapshots?.enabled ?? base.snapshots_enabled,
  };
}

export function lhtSettingsToOverlay(
  settings: LhtSettings,
  composerMode: LhtComposerMode,
): ThreadConfigOverlay {
  return {
    lht_composer_mode: composerMode,
    long_horizon: {
      enabled: settings.enabled,
      mode: settings.mode,
      progress_via_git: settings.progress_via_git,
      max_nudges_per_item: settings.max_nudges_per_item,
      blocked_nudges_without_progress: settings.blocked_nudges_without_progress,
      auto_continue: settings.auto_continue,
      max_auto_continue_rounds: settings.max_auto_continue_rounds,
      completion_gate: {
        auto_verify_replay: settings.auto_verify_replay,
        toolchain_gate: settings.toolchain_gate,
        stub_gate: settings.stub_gate,
        max_manifest_rounds: settings.max_manifest_rounds,
        max_audit_rounds: settings.max_audit_rounds,
        max_infra_strikes: settings.max_infra_strikes,
      },
      macro_loop: {
        enabled: settings.macro_loop_enabled,
        max_macro_cycles: settings.macro_loop_max_cycles,
        max_craft_rounds_per_cycle: settings.macro_loop_max_craft_rounds,
        auto_enter_craft: settings.macro_loop_auto_enter_craft,
        craft_on_small_tasks: settings.macro_loop_craft_on_small_tasks,
        min_checklist_items_for_craft: settings.macro_loop_min_checklist_items,
      },
    },
  };
}

function num(v: number | null | undefined, fallback: number): number {
  return typeof v === 'number' ? v : fallback;
}

function str(v: string | null | undefined, fallback: string): string {
  return typeof v === 'string' && v.length > 0 ? v : fallback;
}

function bool(v: boolean | null | undefined, fallback: boolean): boolean {
  return typeof v === 'boolean' ? v : fallback;
}

/** Map runtime effective overlay → desktop LHT panel model. */
export function lhtSettingsFromEffectiveOverlay(
  effective: ThreadConfigOverlay,
  base: LhtSettings,
): { settings: LhtSettings; composerMode: LhtComposerMode } {
  const lh = effective.long_horizon ?? {};
  const gate = lh.completion_gate ?? {};
  const macro = lh.macro_loop ?? {};
  const composerRaw = effective.lht_composer_mode;
  const composerMode: LhtComposerMode =
    composerRaw === 'strict' || composerRaw === 'off' || composerRaw === 'auto'
      ? composerRaw
      : 'auto';
  return {
    composerMode,
    settings: {
      ...base,
      enabled: bool(lh.enabled, base.enabled),
      mode: str(lh.mode, base.mode) as LhtSettings['mode'],
      progress_via_git: bool(lh.progress_via_git, base.progress_via_git),
      max_nudges_per_item: num(lh.max_nudges_per_item, base.max_nudges_per_item),
      blocked_nudges_without_progress: num(
        lh.blocked_nudges_without_progress,
        base.blocked_nudges_without_progress,
      ),
      auto_continue: bool(lh.auto_continue, base.auto_continue),
      max_auto_continue_rounds: num(lh.max_auto_continue_rounds, base.max_auto_continue_rounds),
      auto_verify_replay: str(gate.auto_verify_replay, base.auto_verify_replay) as LhtSettings['auto_verify_replay'],
      toolchain_gate: str(gate.toolchain_gate, base.toolchain_gate) as LhtSettings['toolchain_gate'],
      stub_gate: str(gate.stub_gate, base.stub_gate) as LhtSettings['stub_gate'],
      max_manifest_rounds: num(gate.max_manifest_rounds, base.max_manifest_rounds),
      max_audit_rounds: num(gate.max_audit_rounds, base.max_audit_rounds),
      max_infra_strikes: num(gate.max_infra_strikes, base.max_infra_strikes),
      macro_loop_enabled: bool(macro.enabled, base.macro_loop_enabled),
      macro_loop_max_cycles: num(macro.max_macro_cycles, base.macro_loop_max_cycles),
      macro_loop_max_craft_rounds: num(
        macro.max_craft_rounds_per_cycle,
        base.macro_loop_max_craft_rounds,
      ),
      macro_loop_auto_enter_craft: str(
        macro.auto_enter_craft,
        base.macro_loop_auto_enter_craft,
      ) as LhtSettings['macro_loop_auto_enter_craft'],
      macro_loop_craft_on_small_tasks: bool(
        macro.craft_on_small_tasks,
        base.macro_loop_craft_on_small_tasks,
      ),
      macro_loop_min_checklist_items: num(
        macro.min_checklist_items_for_craft,
        base.macro_loop_min_checklist_items,
      ),
    },
  };
}
