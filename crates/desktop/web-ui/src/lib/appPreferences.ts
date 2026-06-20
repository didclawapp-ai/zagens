import type { RightPanelView } from '../components/RightPanel';
import {
  type ComposerModelId,
  type DesktopRouteIntentOption,
  type DesktopRunModeId,
  type DesktopTaskTypePreference,
  parseDesktopRouteIntentOption,
  parseDesktopRunModeId,
  parseDesktopTaskTypePreference,
} from '../types/desktop';
import {
  DEFAULT_COMPOSER_MODEL,
  normalizeComposerModel,
} from './composerModels';
import {
  fetchDefaultComposerWorkspace,
  isUnsafeComposerWorkspace,
  normalizeWorkspaceForApi,
} from './defaultWorkspace';
import { workspaceStorageKey } from './windowBridge';

export type Theme = 'light' | 'dark';

export const ACTIVE_INSPECTOR_STORAGE_KEY = 'zagens-desktop-active-inspector';
export const RIGHT_PANEL_COLLAPSED_STORAGE_KEY = 'zagens-desktop-right-panel-collapsed';
export const ROUTE_INTENT_STORAGE_KEY = 'zagens-desktop-route-intent';
export const TASK_TYPE_STORAGE_KEY = 'zagens-desktop-task-type';

/**
 * One-time migration: copy values from `deepseek-desktop-*` keys (renamed in v0.7.1)
 * to the new `zagens-desktop-*` keys, then remove the old ones.
 * Safe to call repeatedly — each key is only migrated once.
 */
function migrateDeepseekLocalStorageKeys(): void {
  const renames: [string, string][] = [
    ['deepseek-desktop-task-type', TASK_TYPE_STORAGE_KEY],
    ['deepseek-desktop-active-inspector', ACTIVE_INSPECTOR_STORAGE_KEY],
    ['deepseek-desktop-right-panel-collapsed', RIGHT_PANEL_COLLAPSED_STORAGE_KEY],
    ['deepseek-desktop-route-intent', ROUTE_INTENT_STORAGE_KEY],
    ['deepseek-desktop-run-mode', 'zagens-desktop-run-mode'],
    ['deepseek-desktop-model', 'zagens-desktop-model'],
    ['deepseek-desktop-notify-method', 'zagens-desktop-notify-method'],
  ];
  try {
    for (const [oldKey, newKey] of renames) {
      const oldVal = localStorage.getItem(oldKey);
      if (oldVal !== null && localStorage.getItem(newKey) === null) {
        localStorage.setItem(newKey, oldVal);
      }
      if (oldVal !== null) {
        localStorage.removeItem(oldKey);
      }
    }
    // Workspace keys are per-window-label: migrate any deepseek-desktop-workspace:* entries.
    const workspacePrefix = 'deepseek-desktop-workspace:';
    const newWorkspacePrefix = 'zagens-desktop-workspace:';
    const keysToMigrate: string[] = [];
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (k?.startsWith(workspacePrefix)) keysToMigrate.push(k);
    }
    for (const oldKey of keysToMigrate) {
      const label = oldKey.slice(workspacePrefix.length);
      const newKey = `${newWorkspacePrefix}${label}`;
      const oldVal = localStorage.getItem(oldKey);
      if (oldVal !== null && localStorage.getItem(newKey) === null) {
        localStorage.setItem(newKey, oldVal);
      }
      localStorage.removeItem(oldKey);
    }
  } catch {
    /* ignore — localStorage may be unavailable in some contexts */
  }
}

let cachedOnboardingComplete = false;
let shellPrefsHydrated = false;

/** Whether shell prefs were loaded from disk (Tauri) this session. */
export function isShellPrefsHydrated(): boolean {
  return shellPrefsHydrated;
}

/** Whether onboarding wizard was completed (disk + localStorage). */
export function isOnboardingComplete(): boolean {
  return cachedOnboardingComplete || hasTaskTypePreferenceStored();
}

/** Whether the user has explicitly chosen a default task type (onboarding step 3). */
export function hasTaskTypePreferenceStored(): boolean {
  try {
    return localStorage.getItem(TASK_TYPE_STORAGE_KEY) != null;
  } catch {
    return true;
  }
}

/** Load onboarding prefs from `~/.zagens/settings.toml` (desktop) with localStorage fallback. */
export async function hydrateDesktopShellPrefs(): Promise<{
  onboardingComplete: boolean;
  taskType: DesktopTaskTypePreference;
}> {
  migrateDeepseekLocalStorageKeys();
  const localTaskType = loadTaskTypePreference();
  const localComplete = hasTaskTypePreferenceStored();
  try {
    if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
      const { invoke } = await import('@tauri-apps/api/core');
      const prefs = await invoke<{
        onboarding_complete: boolean;
        task_type_preference: string;
      }>('get_desktop_shell_prefs');
      const diskTaskType = parseDesktopTaskTypePreference(prefs.task_type_preference);
      const taskType = localComplete ? localTaskType : (diskTaskType ?? 'auto');
      cachedOnboardingComplete = prefs.onboarding_complete || localComplete;
      persistTaskTypePreference(taskType);
      if (localComplete && diskTaskType != null && diskTaskType !== localTaskType) {
        try {
          await invoke('save_desktop_shell_prefs', {
            onboarding_complete: cachedOnboardingComplete,
            task_type_preference: localTaskType,
          });
        } catch {
          /* localStorage fallback */
        }
      }
      if (localComplete && !prefs.onboarding_complete) {
        try {
          await invoke('save_desktop_shell_prefs', {
            onboarding_complete: true,
            task_type_preference: taskType,
          });
          cachedOnboardingComplete = true;
        } catch {
          /* keep local fallback */
        }
      }
      shellPrefsHydrated = true;
      return { onboardingComplete: cachedOnboardingComplete, taskType };
    }
  } catch {
    /* fall through */
  }
  cachedOnboardingComplete = localComplete;
  shellPrefsHydrated = true;
  return { onboardingComplete: cachedOnboardingComplete, taskType: localTaskType };
}

export function loadRunModePreference(): DesktopRunModeId {
  try {
    return parseDesktopRunModeId(localStorage.getItem('zagens-desktop-run-mode')) ?? 'agent';
  } catch {
    return 'agent';
  }
}

export function loadComposerPrefs(windowLabel: string): {
  model: ComposerModelId;
  workspace: string;
} {
  try {
    const wm = normalizeComposerModel(localStorage.getItem('zagens-desktop-model'));
    const ws = normalizeWorkspaceForApi(
      localStorage.getItem(workspaceStorageKey(windowLabel))?.trim() ?? '',
    );
    const workspace = ws.length > 0 && !isUnsafeComposerWorkspace(ws) ? ws : '';
    return {
      model: wm ?? DEFAULT_COMPOSER_MODEL,
      workspace,
    };
  } catch {
    return { model: DEFAULT_COMPOSER_MODEL, workspace: '' };
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

/** Persist task type to localStorage and mirror to `settings.toml` on desktop. */
export async function syncTaskTypePreferencePersist(
  value: DesktopTaskTypePreference,
): Promise<void> {
  persistTaskTypePreference(value);
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('save_desktop_shell_prefs', {
        onboarding_complete: cachedOnboardingComplete || hasTaskTypePreferenceStored(),
        task_type_preference: value,
      });
    } catch {
      /* localStorage fallback */
    }
  }
}

/** Mark onboarding complete and mirror prefs to `settings.toml` on desktop.
 *  The localStorage write is the primary store; the Tauri disk write is best-effort
 *  so that a transient IPC failure never blocks the onboarding from completing. */
export async function persistOnboardingComplete(
  taskType: DesktopTaskTypePreference,
): Promise<void> {
  persistTaskTypePreference(taskType);
  cachedOnboardingComplete = true;
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('save_desktop_shell_prefs', {
        onboarding_complete: true,
        task_type_preference: taskType,
      });
    } catch {
      /* localStorage fallback is sufficient; disk sync retried on next launch via migration */
    }
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
      s === 'system' ||
      s === 'sandbox' ||
      s === 'mcp' ||
      s === 'usage' ||
      s === 'tasks' ||
      s === 'skills' ||
      s === 'agents' ||
      s === 'routing' ||
      s === 'lht-settings' ||
      s === 'hooks' ||
      s === 'schedule' ||
      s === 'index' ||
      s === 'checklist' ||
      s === 'audit' ||
      s === 'long-horizon' ||
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

const NOTIFY_METHOD_KEY = 'zagens-desktop-notify-method';

/**
 * Read the cached notify_method from localStorage.
 * Returns 'auto' if not yet set (matches sidecar default).
 */
export function loadNotifyMethod(): string {
  try {
    return localStorage.getItem(NOTIFY_METHOD_KEY) ?? 'auto';
  } catch {
    return 'auto';
  }
}

/** Persist the notify_method chosen in SettingsPanel so the frontend can read it. */
export function persistNotifyMethod(method: string): void {
  try {
    localStorage.setItem(NOTIFY_METHOD_KEY, method);
  } catch {
    /* ignore */
  }
}

export function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === 'dark') {
    root.classList.add('dark');
  } else {
    root.classList.remove('dark');
  }
}
