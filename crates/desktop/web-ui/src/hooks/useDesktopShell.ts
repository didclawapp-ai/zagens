import { useCallback, useEffect, useRef, useState, type Dispatch, type MutableRefObject, type SetStateAction } from 'react';
import { waitForRuntimeBootReady } from '../api/client';
import type { TurnChatMessage } from './useTurnSend';
import { ensureDefaultComposerWorkspace } from '../lib/appPreferences';
import { toast } from '../lib/toast';
import { subscribeCurrentWebviewEvent } from '../lib/tauriListen';
import { getWindowLabel, workspaceStorageKey } from '../lib/windowBridge';
import type { StreamSessionControl } from './useTurnStream';

type StreamMessage = TurnChatMessage;

export type UseDesktopShellParams = {
  t: (key: string, params?: Record<string, string>) => string;
  selectedWorkspace: string;
  setSelectedWorkspace: Dispatch<SetStateAction<string>>;
  refreshSessions: () => Promise<void>;
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
  refreshSessions,
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
  const refreshSessionsRef = useRef(refreshSessions);
  refreshSessionsRef.current = refreshSessions;
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
    let cancelled = false;
    let timedOut = false;
    let bootHandled = false;

    const showWindow = () => {
      void import('@tauri-apps/api/window')
        .then(({ getCurrentWindow }) => getCurrentWindow().show())
        .catch(() => {});
    };

    const onReady = () => {
      if (cancelled || bootHandled) return;
      bootHandled = true;
      void refreshSessionsRef.current();
      showWindow();
    };

    const fallback = setTimeout(() => {
      timedOut = true;
      showWindow();
    }, 5000);

    void waitForRuntimeBootReady({ timeoutMs: 2_000, intervalMs: 100 }).then((ready) => {
      if (cancelled || !ready) return;
      clearTimeout(fallback);
      if (!timedOut) {
        onReady();
      }
    });

    const unlistenReady = subscribeCurrentWebviewEvent<Record<string, unknown>>(
      'sidecar://ready',
      () => {
        clearTimeout(fallback);
        if (!timedOut) {
          onReady();
        }
      },
    );
    return () => {
      cancelled = true;
      clearTimeout(fallback);
      unlistenReady();
    };
  }, [desktopHost]);

  useEffect(() => {
    if (!desktopHost) return;
    return subscribeCurrentWebviewEvent('sidecar://restarting', () => {
      abortActiveStreamForSidecarRestartRef.current();
    });
  }, [desktopHost]);

  return {
    desktopHost,
    desktopApiKeyConfigured,
    platform,
    refreshApiKeyStatus,
    abortActiveStreamForSidecarRestart,
  };
}
