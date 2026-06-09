/** Desktop Composer: model IDs accepted by DeepSeek Chat Completions-compatible API. */

export type DesktopRunModeId = 'plan' | 'agent' | 'yolo';

/** Session task type: office docs/chat vs full code agent. */
export type DesktopTaskTypePreference = 'auto' | 'office' | 'code';

export type DesktopTaskTypeResolved = 'office' | 'code';

/** Office sessions only use Agent run mode (Plan/Yolo are code-workflow oriented). */

export function parseDesktopTaskTypePreference(raw: unknown): DesktopTaskTypePreference | undefined {
  if (raw === 'auto' || raw === 'office' || raw === 'code') return raw;
  return undefined;
}

export function parseDesktopTaskTypeResolved(raw: unknown): DesktopTaskTypeResolved | undefined {
  if (raw === 'office' || raw === 'code') return raw;
  return undefined;
}

/** UI labels aligned with crates/tui SandboxPolicy elevation in engine `build_tool_context` */
export const DESKTOP_RUN_MODE_LABELS: Record<DesktopRunModeId, string> = {
  plan: 'Plan',
  agent: 'Agent',
  yolo: 'YOLO',
};

export function parseDesktopRunModeId(raw: unknown): DesktopRunModeId | undefined {
  if (raw === 'plan' || raw === 'agent' || raw === 'yolo') return raw;
  return undefined;
}

/**
 * Optional intent passed as `route_intent` on thread/stream turns.
 * Runtime matches against `~/.zagens/routing_rules.json` (RoutingPanel).
 */
export type DesktopRouteIntentOption = 'off' | 'follow_runmode' | 'code' | 'chat' | 'research';

export function parseDesktopRouteIntentOption(raw: unknown): DesktopRouteIntentOption | undefined {
  if (raw === 'off' || raw === 'follow_runmode' || raw === 'code' || raw === 'chat' || raw === 'research') {
    return raw;
  }
  return undefined;
}

/** Order for routing strategy UI (RoutingPanel). */
export const ROUTE_INTENT_OPTIONS: DesktopRouteIntentOption[] = [
  'off',
  'follow_runmode',
  'code',
  'chat',
  'research',
];

/** Resolved value for API; omit field when `undefined`. */
export function resolveRouteIntentForApi(
  opt: DesktopRouteIntentOption,
  runMode: DesktopRunModeId,
): string | undefined {
  if (opt === 'off') return undefined;
  if (opt === 'follow_runmode') return runMode;
  return opt;
}

/** Runtime model id sent on thread/stream turns (any provider-specific string). */
export type ComposerModelId = string;

/** @deprecated Use `ComposerModelId`; kept for gradual migration. */
export type DesktopModelId = ComposerModelId;

export {
  composerModelLabel as desktopModelLabel,
  composerModelShortLabel as desktopModelShortLabel,
  DESKTOP_MODEL_PRESET_IDS,
} from '../lib/composerModels';

/** @deprecated Use `composerModelLabel` from `lib/composerModels`. */
export const DESKTOP_MODEL_LABELS: Record<string, string> = {
  'deepseek-v4-pro': 'DeepSeek V4 Pro',
  'deepseek-v4-flash': 'DeepSeek V4 Flash',
};

/** @deprecated Use `composerModelShortLabel` from `lib/composerModels`. */
export const DESKTOP_MODEL_SHORT_LABELS: Record<string, string> = {
  'deepseek-v4-pro': 'V4 Pro',
  'deepseek-v4-flash': 'V4 Flash',
};

/** Compact label for Composer routing status chip. */
export function composerRoutingStatusLabel(
  t: (key: string, params?: Record<string, string>) => string,
  opt: DesktopRouteIntentOption,
  runMode: DesktopRunModeId,
): string | null {
  if (opt === 'off') return null;
  if (opt === 'follow_runmode') {
    return t('routing.statusFollowRunmode', { mode: DESKTOP_RUN_MODE_LABELS[runMode] });
  }
  return t('routing.statusFixed', { intent: opt });
}

export function parseDesktopModelId(raw: unknown): ComposerModelId | undefined {
  if (typeof raw !== 'string') return undefined;
  const trimmed = raw.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}
