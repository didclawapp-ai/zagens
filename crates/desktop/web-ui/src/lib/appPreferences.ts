import type { RightPanelView } from '../components/RightPanel';
import {
  type DesktopModelId,
  type DesktopRouteIntentOption,
  type DesktopRunModeId,
  type DesktopTaskTypePreference,
  parseDesktopModelId,
  parseDesktopRouteIntentOption,
  parseDesktopRunModeId,
  parseDesktopTaskTypePreference,
} from '../types/desktop';
import {
  fetchDefaultComposerWorkspace,
  isUnsafeComposerWorkspace,
  normalizeWorkspaceForApi,
} from './defaultWorkspace';
import { workspaceStorageKey } from './windowBridge';

export type Theme = 'light' | 'dark';

export const ACTIVE_INSPECTOR_STORAGE_KEY = 'deepseek-desktop-active-inspector';
export const RIGHT_PANEL_COLLAPSED_STORAGE_KEY = 'deepseek-desktop-right-panel-collapsed';
export const ROUTE_INTENT_STORAGE_KEY = 'deepseek-desktop-route-intent';
export const TASK_TYPE_STORAGE_KEY = 'deepseek-desktop-task-type';
/** Whether the user has explicitly chosen a default task type (onboarding step 3). */
export function hasTaskTypePreferenceStored(): boolean {
  try {
    return localStorage.getItem(TASK_TYPE_STORAGE_KEY) != null;
  } catch {
    return true;
  }
}

export function loadRunModePreference(): DesktopRunModeId {
  try {
    return parseDesktopRunModeId(localStorage.getItem('deepseek-desktop-run-mode')) ?? 'agent';
  } catch {
    return 'agent';
  }
}

export function loadComposerPrefs(windowLabel: string): {
  model: DesktopModelId;
  workspace: string;
} {
  try {
    const wm = parseDesktopModelId(localStorage.getItem('deepseek-desktop-model'));
    const ws = normalizeWorkspaceForApi(
      localStorage.getItem(workspaceStorageKey(windowLabel))?.trim() ?? '',
    );
    const workspace = ws.length > 0 && !isUnsafeComposerWorkspace(ws) ? ws : '';
    return {
      model: wm ?? 'deepseek-v4-pro',
      workspace,
    };
  } catch {
    return { model: 'deepseek-v4-pro', workspace: '' };
  }
}

/** First-run or legacy `.` / System32 paths → `<Documents>/Zagens` (or legacy Zagens folder). */
export async function ensureDefaultComposerWorkspace(
  current: string,
  setWorkspace: (path: string) => void,
): Promise<void> {
  if (current.trim().length > 0 && !isUnsafeComposerWorkspace(current)) {
    return;
  }
  const path = await fetchDefaultComposerWorkspace();
  if (path.trim().length > 0 && !isUnsafeComposerWorkspace(path)) {
    setWorkspace(path);
  }
}

export function loadTheme(): Theme {
  try {
    const stored = localStorage.getItem('deepseek-theme');
    if (stored === 'dark' || stored === 'light') return stored;
  } catch {
    /* ignore */
  }
  return 'light';
}

export function loadTaskTypePreference(): DesktopTaskTypePreference {
  try {
    return parseDesktopTaskTypePreference(localStorage.getItem(TASK_TYPE_STORAGE_KEY)) ?? 'auto';
  } catch {
    return 'auto';
  }
}

/** Persist an explicit task-type choice (onboarding or composer). */
export function persistTaskTypePreference(value: DesktopTaskTypePreference): void {
  try {
    localStorage.setItem(TASK_TYPE_STORAGE_KEY, value);
  } catch {
    /* ignore */
  }
}

export function loadRouteIntentPreference(): DesktopRouteIntentOption {
  try {
    return parseDesktopRouteIntentOption(localStorage.getItem(ROUTE_INTENT_STORAGE_KEY)) ?? 'off';
  } catch {
    return 'off';
  }
}

export function loadStoredInspector(): RightPanelView {
  try {
    let s = localStorage.getItem(ACTIVE_INSPECTOR_STORAGE_KEY);
    if (s === 'automation' || s === 'tasks-skills') {
      s = 'tasks';
      try {
        localStorage.setItem(ACTIVE_INSPECTOR_STORAGE_KEY, 'tasks');
      } catch {
        /* ignore */
      }
    }
    if (
      s === 'workspace' ||
      s === 'api-key' ||
      s === 'settings' ||
      s === 'mcp' ||
      s === 'usage' ||
      s === 'tasks' ||
      s === 'skills' ||
      s === 'agents' ||
      s === 'routing' ||
      s === 'lht-settings' ||
      s === 'index' ||
      s === 'checklist' ||
      s === 'audit' ||
      s === 'mermaid' ||
      s === 'about'
    ) {
      return s;
    }
  } catch {
    /* ignore */
  }
  return 'workspace';
}

/** First launch (no key): collapsed; later launches restore last collapsed/expanded state. */
export function loadStoredRightPanelCollapsed(): boolean {
  try {
    const s = localStorage.getItem(RIGHT_PANEL_COLLAPSED_STORAGE_KEY);
    if (s === null) return true;
    if (s === 'false' || s === '0') return false;
    if (s === 'true' || s === '1') return true;
  } catch {
    /* ignore */
  }
  return true;
}

export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === 'dark') {
    root.classList.add('dark');
  } else {
    root.classList.remove('dark');
  }
}
