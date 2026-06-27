import { useCallback, useState } from 'react';
import { patchThread, type RuntimeConnectionState } from '../api/client';
import type { RightPanelView } from '../components/RightPanel';
import type { PreviewState } from '../components/preview/types';
import { loadWorkspaceFileIntoPreview, normalizeWorkspaceRelPath } from '../lib/openWorkspaceFile';
import {
  isOfficePreviewExternal,
  openWorkspaceFileWithSystemApp,
} from '../lib/openWorkspaceSystem';
import { formatWorkspaceFileError, WorkspaceFileOpenError } from '../lib/workspaceFileOpenError';
import { toast } from '../lib/toast';
import { isRuntimeApiAvailable } from '../lib/runtimeReachable';
export type UseWorkspacePanelParams = {
  t: (key: string, params?: Record<string, string>) => string;
  runtimeConn: RuntimeConnectionState;
  runtimeReachability: { streaming: boolean; sessionEstablished: boolean };
  selectedWorkspace: string;
  resumedThreadId: string | null;
  desktopHost: boolean;
  setSelectedWorkspace: React.Dispatch<React.SetStateAction<string>>;
  setActiveInspector: React.Dispatch<React.SetStateAction<RightPanelView>>;
  setRightPanelCollapsed: React.Dispatch<React.SetStateAction<boolean>>;
  setAuditGridDismissed: React.Dispatch<React.SetStateAction<boolean>>;
};

export function useWorkspacePanel({
  t,
  runtimeConn,
  runtimeReachability,
  selectedWorkspace,
  resumedThreadId,
  desktopHost,
  setSelectedWorkspace,
  setActiveInspector,
  setRightPanelCollapsed,
  setAuditGridDismissed,
}: UseWorkspacePanelParams) {
  const [panelPreview, setPanelPreview] = useState<PreviewState | null>(null);
  const [focusWorkspaceFilesNonce, setFocusWorkspaceFilesNonce] = useState(0);
  const [focusWorkspaceFilesRelPath, setFocusWorkspaceFilesRelPath] = useState<string | null>(
    null,
  );
  const [focusWorkspaceDiffNonce, setFocusWorkspaceDiffNonce] = useState(0);
  const [composerMentionNonce, setComposerMentionNonce] = useState(0);
  const [composerMentionRel, setComposerMentionRel] = useState<string | null>(null);
  const [composerMentionIsDir, setComposerMentionIsDir] = useState(false);
  const [composerPrefill, setComposerPrefill] = useState<
    { text: string; nonce: number } | undefined
  >();
  const [filesRefreshNonce, setFilesRefreshNonce] = useState(0);

  const bumpFilesRefresh = useCallback(() => {
    setFilesRefreshNonce((n) => n + 1);
  }, []);

  const closePanelPreview = useCallback(() => {
    setPanelPreview(null);
  }, []);

  const addWorkspaceFileToChat = useCallback((relPath: string, isDirectory = false) => {
    const rel = normalizeWorkspaceRelPath(relPath);
    if (!rel) return;
    setComposerMentionRel(rel);
    setComposerMentionIsDir(isDirectory);
    setComposerMentionNonce((n) => n + 1);
  }, []);

  const revealWorkspaceFileInDirectory = useCallback(
    (relPath: string) => {
      const rel = normalizeWorkspaceRelPath(relPath);
      if (!rel) return;
      setActiveInspector('workspace');
      setAuditGridDismissed(true);
      setRightPanelCollapsed(false);
      setFocusWorkspaceFilesRelPath(rel);
      setFocusWorkspaceFilesNonce((n) => n + 1);
    },
    [setActiveInspector, setAuditGridDismissed, setRightPanelCollapsed],
  );

  const openWorkspaceFileForPreview = useCallback(
    async (relPath: string, title?: string) => {
      if (!isRuntimeApiAvailable(runtimeConn, runtimeReachability)) {
        throw new Error(t('banner.runtimeNotConnected'));
      }
      revealWorkspaceFileInDirectory(relPath);
      const fileName = (title?.trim() || relPath).split('/').pop() ?? relPath;
      if (isOfficePreviewExternal(fileName)) {
        if (!desktopHost) {
          throw new WorkspaceFileOpenError('binaryNeedsDesktop');
        }
        await openWorkspaceFileWithSystemApp(selectedWorkspace, relPath);
        setPanelPreview(null);
        toast.info(t('workspaceFiles.openedWithSystemApp'));
        return;
      }
      const state = await loadWorkspaceFileIntoPreview({
        relPath,
        title,
        workspaceRoot: selectedWorkspace,
        resumedThreadId,
        desktopHost,
      });
      setPanelPreview(state);
    },
    [
      runtimeConn,
      runtimeReachability,
      selectedWorkspace,
      resumedThreadId,
      desktopHost,
      t,
      revealWorkspaceFileInDirectory,
    ],
  );

  const handleOfficeDeliverableReady = useCallback(
    async (relPath: string) => {
      setFilesRefreshNonce((n) => n + 1);
      try {
        await openWorkspaceFileForPreview(relPath);
      } catch {
        // files tab still reveals path; office formats open via system app
      }
    },
    [openWorkspaceFileForPreview],
  );

  const handleChatOpenWorkspacePath = useCallback(
    async (relPath: string) => {
      try {
        await openWorkspaceFileForPreview(relPath);
      } catch (e) {
        toast.error(t('banner.openFileFailed', { err: formatWorkspaceFileError(e, t) }));
      }
    },
    [openWorkspaceFileForPreview, t],
  );

  const openDiffInPanel = useCallback(() => {
    setActiveInspector('workspace');
    setAuditGridDismissed(true);
    setRightPanelCollapsed(false);
    setFocusWorkspaceDiffNonce((n) => n + 1);
  }, [setActiveInspector, setAuditGridDismissed, setRightPanelCollapsed]);

  const handleRequestDiffPanel = useCallback(() => {
    openDiffInPanel();
  }, [openDiffInPanel]);

  const handleComposerWorkspaceChange = useCallback(
    async (next: string) => {
      const trimmed = next.trim();
      if (!trimmed) {
        throw new Error(t('banner.workspaceEmpty'));
      }
      if (!resumedThreadId) {
        setSelectedWorkspace(trimmed);
        return;
      }
      try {
        const updated = await patchThread(resumedThreadId, { workspace: trimmed });
        setSelectedWorkspace(typeof updated.workspace === 'string' ? updated.workspace : trimmed);
      } catch (e) {
        const err = e as Error & { status?: number };
        let msg = err.message ?? String(e);
        if (/active turn|finish or interrupt/i.test(msg)) {
          toast.warning(t('banner.activeTurnBlocking'));
        } else {
          toast.error(t('banner.updateThreadWorkspace', { msg }));
        }
        throw err;
      }
    },
    [resumedThreadId, setSelectedWorkspace, t],
  );

  return {
    panelPreview,
    setPanelPreview,
    focusWorkspaceFilesNonce,
    focusWorkspaceFilesRelPath,
    focusWorkspaceDiffNonce,
    composerMentionNonce,
    composerMentionRel,
    composerMentionIsDir,
    composerPrefill,
    setComposerPrefill,
    closePanelPreview,
    addWorkspaceFileToChat,
    revealWorkspaceFileInDirectory,
    openWorkspaceFileForPreview,
    handleChatOpenWorkspacePath,
    openDiffInPanel,
    handleRequestDiffPanel,
    handleComposerWorkspaceChange,
    filesRefreshNonce,
    bumpFilesRefresh,
    handleOfficeDeliverableReady,
  };
}
