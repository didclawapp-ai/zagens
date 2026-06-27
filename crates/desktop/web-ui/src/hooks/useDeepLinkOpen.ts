import { useCallback, useEffect, type Dispatch, type SetStateAction } from 'react';
import { subscribeCurrentWebviewEvent } from '../lib/tauriListen';
import {
  parseDesktopTaskTypePreference,
  type DesktopTaskTypePreference,
} from '../types/desktop';

export type DeepLinkOpenPayload = {
  workspace?: string;
  prompt?: string | null;
  taskType?: string | null;
  useWorktree?: boolean | null;
};

type UseDeepLinkOpenParams = {
  desktopHost: boolean;
  shellPrefsReady: boolean;
  setSelectedWorkspace: Dispatch<SetStateAction<string>>;
  setTaskTypePreference?: Dispatch<SetStateAction<DesktopTaskTypePreference>>;
  setUseWorktree?: Dispatch<SetStateAction<boolean>>;
  setComposerPrefill: Dispatch<
    SetStateAction<{ text: string; nonce: number } | undefined>
  >;
};

function applyDeepLinkOpenPayload(
  payload: DeepLinkOpenPayload,
  {
    setSelectedWorkspace,
    setTaskTypePreference,
    setUseWorktree,
    setComposerPrefill,
  }: Pick<
    UseDeepLinkOpenParams,
    | 'setSelectedWorkspace'
    | 'setTaskTypePreference'
    | 'setUseWorktree'
    | 'setComposerPrefill'
  >,
) {
  const workspace = payload.workspace?.trim();
  if (workspace) {
    setSelectedWorkspace(workspace);
  }
  const taskType = parseDesktopTaskTypePreference(payload.taskType);
  if (taskType && setTaskTypePreference) {
    setTaskTypePreference(taskType);
  }
  if (payload.useWorktree === true && setUseWorktree) {
    setUseWorktree(true);
  }
  const prompt = payload.prompt?.trim();
  if (prompt) {
    setComposerPrefill({ text: prompt, nonce: Date.now() });
  }
}

export function useDeepLinkOpen({
  desktopHost,
  shellPrefsReady,
  setSelectedWorkspace,
  setTaskTypePreference,
  setUseWorktree,
  setComposerPrefill,
}: UseDeepLinkOpenParams) {
  const applyPayload = useCallback(
    (payload: DeepLinkOpenPayload) => {
      applyDeepLinkOpenPayload(payload, {
        setSelectedWorkspace,
        setTaskTypePreference,
        setUseWorktree,
        setComposerPrefill,
      });
    },
    [setComposerPrefill, setSelectedWorkspace, setTaskTypePreference, setUseWorktree],
  );

  useEffect(() => {
    if (!desktopHost) return;
    return subscribeCurrentWebviewEvent<DeepLinkOpenPayload>(
      'zagens://open-request',
      applyPayload,
    );
  }, [applyPayload, desktopHost]);

  useEffect(() => {
    if (!desktopHost || !shellPrefsReady) return;
    void (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const pending = await invoke<DeepLinkOpenPayload | null>('take_pending_deep_link');
        if (pending) applyPayload(pending);
      } catch {
        /* non-Tauri dev */
      }
    })();
  }, [applyPayload, desktopHost, shellPrefsReady]);
}
