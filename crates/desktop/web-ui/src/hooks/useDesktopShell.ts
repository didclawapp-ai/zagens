import { useCallback, useEffect, useRef, useState, type Dispatch, type MutableRefObject, type SetStateAction } from 'react';
import type { TurnChatMessage } from './useTurnSend';
import { ensureDefaultComposerWorkspace } from '../lib/appPreferences';
import { toast } from '../lib/toast';
import { subscribeCurrentWebviewEvent } from '../lib/tauriListen';
import { fetchAppUpdateStatus } from '../lib/appUpdate';
import { dispatchSidecarReadyForPanels } from '../lib/sidecarPanelRecovery';
import { getWindowLabel, workspaceStorageKey } from '../lib/windowBridge';
import type { StreamSessionControl } from './useTurnStream';

type StreamMessage = TurnChatMessage;

export type UseDesktopShellParams = {
  t: (key: string, params?: Record<string, string>) => string;
  selectedWorkspace: string;
  setSelectedWorkspace: Dispatch<SetStateAction<string>>;
  streamingRef: MutableRefObject<boolean>;
  streamControllersRef: MutableRefObject<Map<string, AbortController>>;
  streamSessionRef: MutableRefObject<StreamSessionControl | null>;
  setStreamingThreadIds: Dispatch<SetStateAction<Set<string>>>;
  setPendingComposerStream: Dispatch<SetStateAction<boolean>>;
  setMessages: Dispatch<SetStateAction<StreamMessage[]>>;
  notifyRuntimeTransient: (message: string) => void;
};

export type UseDesktopShellResult = {
  desktopHost: boolean;
  desktopApiKeyConfigured: boolean | null;
  platform: string;
  refreshApiKeyStatus: () => void;
  abortActiveStreamForSidecarRestart: () => void;
};

export function useDesktopShell({
  t,
  selectedWorkspace,
  setSelectedWorkspace,
  streamingRef,
  streamControllersRef,
  streamSessionRef,
  setStreamingThreadIds,
  setPendingComposerStream,
  setMessages,
  notifyRuntimeTransient,
}: UseDesktopShellParams): UseDesktopShellResult {
  const [desktopHost, setDesktopHost] = useState(false);
  const [desktopApiKeyConfigured, setDesktopApiKeyConfigured] = useState<boolean | null>(null);
  const [platform, setPlatform] = useState('unknown');
  const selectedWorkspaceRef = useRef(selectedWorkspace);
  selectedWorkspaceRef.current = selectedWorkspace;

  const runRefreshApiKeyStatus = useCallback(async () => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      const s = await invoke<{ configured: boolean }>('get_api_key_status');
      setDesktopHost(true);
      setDesktopApiKeyConfigured(s.configured);
      const info = await invoke<{ os: string; arch: string; version: string }>('get_platform_info');
      setPlatform(info.os);
      await ensureDefaultComposerWorkspace(
        localStorage.getItem(workspaceStorageKey(getWindowLabel()))?.trim() ??
          selectedWorkspaceRef.current,
        setSelectedWorkspace,
      );
    } catch {
      setDesktopHost(false);
      setDesktopApiKeyConfigured(null);
    }
  }, [setSelectedWorkspace]);

  const refreshApiKeyStatus = useCallback(() => {
    void runRefreshApiKeyStatus();
  }, [runRefreshApiKeyStatus]);

  const abortActiveStreamForSidecarRestartRef = useRef<() => void>(() => {});

  const abortActiveStreamForSidecarRestart = useCallback(() => {
    if (!streamingRef.current) return;
    for (const c of streamControllersRef.current.values()) {
      c.abort();
    }
    streamControllersRef.current.clear();
    const label = t('composer.runtimeSidecarRestart');
    setMessages((prev) =>
      prev.map((m) => {
        if (!m.isStreaming) return m;
        const tools = (m.tools ?? []).map((tool) =>
          tool.status === 'running' ? { ...tool, status: 'error' as const } : tool,
        );
        const trimmed = m.content.trim();
        let content = m.content;
        if (!trimmed) {
          content = label;
        } else if (!trimmed.includes(label)) {
          content = `[${label}] ${m.content}`;
        }
        return { ...m, tools, content, isStreaming: false };
      }),
    );
    const session = streamSessionRef.current;
    if (session) {
      session.finishOnce();
    } else {
      setStreamingThreadIds(new Set());
      setPendingComposerStream(false);
    }
    notifyRuntimeTransient(t('banner.runtimeRestartDuringStream'));
  }, [
    notifyRuntimeTransient,
    setMessages,
    setPendingComposerStream,
    setStreamingThreadIds,
    streamControllersRef,
    streamSessionRef,
    streamingRef,
    t,
  ]);

  abortActiveStreamForSidecarRestartRef.current = abortActiveStreamForSidecarRestart;

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

  useEffect(() => {
    if (!desktopHost) return;
    const unlistenRestart = subscribeCurrentWebviewEvent('sidecar://restarting', () => {
      abortActiveStreamForSidecarRestartRef.current();
    });
    const unlistenReady = subscribeCurrentWebviewEvent('sidecar://ready', () => {
      dispatchSidecarReadyForPanels();
    });
    return () => {
      unlistenRestart();
      unlistenReady();
    };
  }, [desktopHost]);

  return {
    desktopHost,
    desktopApiKeyConfigured,
    platform,
    refreshApiKeyStatus,
    abortActiveStreamForSidecarRestart,
  };
}
