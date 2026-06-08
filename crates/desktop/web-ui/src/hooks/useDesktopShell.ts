import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react';
import { ensureDefaultComposerWorkspace, hydrateDesktopShellPrefs } from '../lib/appPreferences';
import type { DesktopTaskTypePreference } from '../types/desktop';
import { toast } from '../lib/toast';
import { fetchAppUpdateStatus } from '../lib/appUpdate';
import { getWindowLabel, workspaceStorageKey } from '../lib/windowBridge';

export type UseDesktopShellParams = {
  t: (key: string, params?: Record<string, string>) => string;
  selectedWorkspace: string;
  setSelectedWorkspace: Dispatch<SetStateAction<string>>;
  setTaskTypePreference?: Dispatch<SetStateAction<DesktopTaskTypePreference>>;
};

export type UseDesktopShellResult = {
  desktopHost: boolean;
  /** Tauri present but shell IPC failed (e.g. disk full on user-data volume). */
  shellInitFailed: boolean;
  /** Disk-backed onboarding prefs loaded (avoid flashing wizard before read). */
  shellPrefsReady: boolean;
  onboardingComplete: boolean;
  desktopApiKeyConfigured: boolean | null;
  platform: string;
  refreshApiKeyStatus: () => void;
  markOnboardingComplete: (taskType?: DesktopTaskTypePreference) => void;
};

export function useDesktopShell({
  t,
  selectedWorkspace,
  setSelectedWorkspace,
  setTaskTypePreference,
}: UseDesktopShellParams): UseDesktopShellResult {
  const [desktopHost, setDesktopHost] = useState(false);
  const [shellInitFailed, setShellInitFailed] = useState(false);
  const [shellPrefsReady, setShellPrefsReady] = useState(false);
  const [onboardingComplete, setOnboardingComplete] = useState(false);
  const [desktopApiKeyConfigured, setDesktopApiKeyConfigured] = useState<boolean | null>(null);
  const [platform, setPlatform] = useState('unknown');
  const selectedWorkspaceRef = useRef(selectedWorkspace);
  selectedWorkspaceRef.current = selectedWorkspace;

  const runRefreshApiKeyStatus = useCallback(async () => {
    const inTauri =
      typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const s = await invoke<{ configured: boolean }>('get_api_key_status');
      setShellInitFailed(false);
      setDesktopHost(true);
      setDesktopApiKeyConfigured(s.configured);
      const info = await invoke<{ os: string; arch: string; version: string }>('get_platform_info');
      setPlatform(info.os);
      const prefs = await hydrateDesktopShellPrefs();
      setOnboardingComplete(prefs.onboardingComplete);
      setTaskTypePreference?.(prefs.taskType);
      setShellPrefsReady(true);
      await ensureDefaultComposerWorkspace(
        localStorage.getItem(workspaceStorageKey(getWindowLabel()))?.trim() ??
          selectedWorkspaceRef.current,
        setSelectedWorkspace,
      );
    } catch {
      setDesktopHost(false);
      setDesktopApiKeyConfigured(null);
      setShellPrefsReady(false);
      setOnboardingComplete(false);
      setShellInitFailed(inTauri);
    }
  }, [setSelectedWorkspace, setTaskTypePreference]);

  const refreshApiKeyStatus = useCallback(() => {
    void runRefreshApiKeyStatus();
  }, [runRefreshApiKeyStatus]);

  const markOnboardingComplete = useCallback(
    (taskType?: DesktopTaskTypePreference) => {
      setOnboardingComplete(true);
      if (taskType !== undefined) {
        setTaskTypePreference?.(taskType);
      }
    },
    [setTaskTypePreference],
  );

  useEffect(() => {
    void runRefreshApiKeyStatus();
  }, [runRefreshApiKeyStatus]);

  useEffect(() => {
    if (!desktopHost) return;
    void import('@tauri-apps/api/window')
      .then(({ getCurrentWindow }) => getCurrentWindow().show())
      .catch(() => {});
  }, [desktopHost]);

  useEffect(() => {
    if (!desktopHost) return;
    let cancelled = false;
    void fetchAppUpdateStatus()
      .then((status) => {
        if (cancelled || status.status !== 'available' || !status.availableVersion) return;
        toast.info(t('about.updateToastAvailable', { version: status.availableVersion }));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [desktopHost, t]);

  return {
    desktopHost,
    shellInitFailed,
    shellPrefsReady,
    onboardingComplete,
    desktopApiKeyConfigured,
    platform,
    refreshApiKeyStatus,
    markOnboardingComplete,
  };
}
