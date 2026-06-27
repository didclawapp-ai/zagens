import { useCallback, useState } from 'react';

import { compactThread } from '../api/client';
import type { RightPanelView, WorkspaceTabId } from '../components/RightPanel';
import { contextCategoryNavTarget } from '../lib/contextExplorerNav';
import { confirmDialog } from '../lib/confirmDialog';
import { toast } from '../lib/toast';

export type UseContextExplorerActionsParams = {
  t: (key: string, params?: Record<string, string>) => string;
  resumedThreadId: string | null;
  streaming: boolean;
  setActiveInspector: React.Dispatch<React.SetStateAction<RightPanelView>>;
  setRightPanelCollapsed: React.Dispatch<React.SetStateAction<boolean>>;
  setAuditGridDismissed: React.Dispatch<React.SetStateAction<boolean>>;
  setFocusWorkspaceTab: React.Dispatch<React.SetStateAction<WorkspaceTabId | null>>;
  bumpFocusWorkspaceTab: () => void;
  onArchiveComplete?: () => void;
};

export function useContextExplorerActions({
  t,
  resumedThreadId,
  streaming,
  setActiveInspector,
  setRightPanelCollapsed,
  setAuditGridDismissed,
  setFocusWorkspaceTab,
  bumpFocusWorkspaceTab,
  onArchiveComplete,
}: UseContextExplorerActionsParams) {
  const [archivePending, setArchivePending] = useState(false);

  const navigateContextCategory = useCallback(
    (categoryId: string) => {
      const target = contextCategoryNavTarget(categoryId);
      if (!target) {
        return;
      }
      setAuditGridDismissed(true);
      setRightPanelCollapsed(false);
      if (target.view === 'workspace') {
        setActiveInspector('workspace');
        setFocusWorkspaceTab(target.workspaceTab);
        bumpFocusWorkspaceTab();
      } else {
        setActiveInspector(target.view);
      }
    },
    [
      bumpFocusWorkspaceTab,
      setActiveInspector,
      setAuditGridDismissed,
      setFocusWorkspaceTab,
      setRightPanelCollapsed,
    ],
  );

  const archiveContext = useCallback(async () => {
    const threadId = resumedThreadId?.trim();
    if (!threadId || streaming || archivePending) {
      return;
    }
    if (!(await confirmDialog(t('contextExplorer.archiveConfirm')))) {
      return;
    }
    setArchivePending(true);
    try {
      await compactThread(threadId, { reason: 'Archive context (explorer)' });
      toast.success(t('contextExplorer.archiveStarted'));
      onArchiveComplete?.();
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      toast.error(t('contextExplorer.archiveFailed', { message: msg }));
    } finally {
      setArchivePending(false);
    }
  }, [archivePending, onArchiveComplete, resumedThreadId, streaming, t]);

  return {
    navigateContextCategory,
    archiveContext,
    archivePending,
    canArchiveContext: Boolean(resumedThreadId?.trim()) && !streaming,
  };
}
