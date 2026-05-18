/** Desktop Composer: model IDs accepted by DeepSeek Chat Completions-compatible API. */

export type DesktopRunModeId = 'plan' | 'agent' | 'yolo';

/** Session task type: office docs/chat vs full code agent. */
export type DesktopTaskTypePreference = 'auto' | 'office' | 'code';

export type DesktopTaskTypeResolved = 'office' | 'code';

export const DESKTOP_TASK_TYPE_LABELS: Record<DesktopTaskTypePreference, string> = {
  auto: '自动',
  office: '办公',
  code: '代码',
};

export const DESKTOP_TASK_TYPE_HINTS: Record<DesktopTaskTypePreference, string> = {
  auto: '按工作区与首条消息推断；新建会话时生效',
  office: '聊天与办公文档；精简工具与 prompt',
  code: '完整编程 Agent 工具面',
};

/** Office sessions only use Agent run mode (Plan/Yolo are code-workflow oriented). */
export const OFFICE_COMPOSER_RUN_MODE_HINT =
  '办公模式仅使用 Agent（无 Plan / YOLO）';

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

export const DESKTOP_RUN_MODE_HINTS: Record<DesktopRunModeId, string> = {
  plan: 'Strict：无 Shell，不向 Shell 升格 WorkspaceWrite／网络（与 CLI Plan 一致）',
  agent: 'WorkspaceWrite + 网络（引擎 #273 Shell 升格）',
  yolo: 'DangerFullAccess：SandboxPolicy 完全不限制（慎用）',
};

export function parseDesktopRunModeId(raw: unknown): DesktopRunModeId | undefined {
  if (raw === 'plan' || raw === 'agent' || raw === 'yolo') return raw;
  return undefined;
}

/**
 * Optional intent passed as `route_intent` on thread/stream turns.
 * Runtime matches against `~/.deepseek/routing_rules.json` (RoutingPanel).
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

export const DESKTOP_ROUTE_INTENT_LABELS: Record<DesktopRouteIntentOption, string> = {
  off: '关闭路由',
  follow_runmode: '跟随运行模式',
  code: '固定意图 · code',
  chat: '固定意图 · chat',
  research: '固定意图 · research',
};

export const DESKTOP_ROUTE_INTENT_HINTS: Record<DesktopRouteIntentOption, string> = {
  off: '不向运行时发送 route_intent（仅用 Composer 所选模型）',
  follow_runmode: '将当前 Plan / Agent / YOLO 作为意图传给 routing_rules.json',
  code: '固定意图 code（用于路由规则匹配）',
  chat: '固定意图 chat',
  research: '固定意图 research',
};

/** Resolved value for API; omit field when `undefined`. */
export function resolveRouteIntentForApi(
  opt: DesktopRouteIntentOption,
  runMode: DesktopRunModeId,
): string | undefined {
  if (opt === 'off') return undefined;
  if (opt === 'follow_runmode') return runMode;
  return opt;
}

export type DesktopModelId = 'deepseek-v4-pro' | 'deepseek-v4-flash';

export const DESKTOP_MODEL_LABELS: Record<DesktopModelId, string> = {
  'deepseek-v4-pro': 'DeepSeek V4 Pro',
  'deepseek-v4-flash': 'DeepSeek V4 Flash',
};

export const DESKTOP_MODEL_SHORT_LABELS: Record<DesktopModelId, string> = {
  'deepseek-v4-pro': 'V4 Pro',
  'deepseek-v4-flash': 'V4 Flash',
};

/** Compact label for Composer routing status chip. */
export function composerRoutingStatusLabel(
  opt: DesktopRouteIntentOption,
  runMode: DesktopRunModeId,
): string | null {
  if (opt === 'off') return null;
  if (opt === 'follow_runmode') return `路由 · ${DESKTOP_RUN_MODE_LABELS[runMode]}`;
  return `路由 · ${opt}`;
}

export function parseDesktopModelId(raw: unknown): DesktopModelId | undefined {
  if (raw === 'deepseek-v4-pro' || raw === 'deepseek-v4-flash') return raw;
  return undefined;
}
