/** Desktop Composer: model IDs accepted by DeepSeek Chat Completions-compatible API. */

export type DesktopRunModeId = 'plan' | 'agent' | 'yolo';

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

export type DesktopModelId = 'deepseek-v4-pro' | 'deepseek-v4-flash';

export const DESKTOP_MODEL_LABELS: Record<DesktopModelId, string> = {
  'deepseek-v4-pro': 'DeepSeek V4 Pro',
  'deepseek-v4-flash': 'DeepSeek V4 Flash',
};

export function parseDesktopModelId(raw: unknown): DesktopModelId | undefined {
  if (raw === 'deepseek-v4-pro' || raw === 'deepseek-v4-flash') return raw;
  return undefined;
}
