import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from 'react';
import {
  fetchSystemSettings,
  postResolveApproval,
  saveSystemSettings,
  type SystemSettings,
} from '../api/client';
import { autoApproveFromPolicy, composerAutoApproveToggleEnabled } from '../lib/approvalPolicy';
import { persistNotifyMethod } from '../lib/appPreferences';
import { confirmDialog } from '../lib/confirmDialog';
import { threadOwnedByWindow } from '../lib/windowBridge';
import { toast } from '../lib/toast';
import { resolveActiveThreadTurn } from '../lib/chat/streamContextAccess';
import type { StreamContextRegistry } from './useStreamContextRegistry';
import type { DesktopRunModeId } from '../types/desktop';

export type ApprovalState = {
  toolCallId: string;
  toolName: string;
  description: string;
};

export type UseTurnApprovalParams = {
  t: (key: string, params?: Record<string, string>) => string;
  streamRegistry: StreamContextRegistry;
  resumedThreadIdRef: MutableRefObject<string | null>;
  desktopHost: boolean;
  runModeRef: MutableRefObject<DesktopRunModeId>;
  streamingRef: MutableRefObject<boolean>;
};

export type UseTurnApprovalResult = {
  approval: ApprovalState | null;
  setApproval: Dispatch<SetStateAction<ApprovalState | null>>;
  approvalBusy: boolean;
  approvalPolicy: string;
  approvalPolicyBusy: boolean;
  autoApprove: boolean;
  setAutoApprove: Dispatch<SetStateAction<boolean>>;
  handleAutoApproveChange: (value: boolean) => void;
  handleApprovalPolicyChange: (policy: string) => Promise<boolean>;
  syncAutoApproveFromPolicy: (policy: string) => void;
  syncAutoApproveFromRunMode: (mode: DesktopRunModeId) => void;
  handleSystemSettingsSaved: (settings: SystemSettings) => void;
  handleApproveDecision: (decision: 'approve' | 'deny', rememberForSession?: boolean) => Promise<void>;
  showApprovalIfOwned: (desktopHost: boolean, payload: ApprovalState) => void;
  clearApproval: () => void;
};

export function useTurnApproval({
  t,
  streamRegistry,
  resumedThreadIdRef,
  desktopHost,
  runModeRef,
  streamingRef,
}: UseTurnApprovalParams): UseTurnApprovalResult {
  const [approval, setApproval] = useState<ApprovalState | null>(null);
  const [approvalBusy, setApprovalBusy] = useState(false);
  const [approvalPolicy, setApprovalPolicy] = useState('on-request');
  const [approvalPolicyBusy, setApprovalPolicyBusy] = useState(false);
  const [autoApprove, setAutoApprove] = useState(false);
  const approvalPolicyRef = useRef('on-request');

  const syncAutoApproveFromPolicy = useCallback((policy: string) => {
    approvalPolicyRef.current = policy;
    setApprovalPolicy(policy);
    const mode = runModeRef.current;
    if (mode === 'yolo') {
      setAutoApprove(true);
    } else if (mode === 'plan') {
      setAutoApprove(false);
    } else {
      setAutoApprove(autoApproveFromPolicy(policy));
    }
  }, [runModeRef]);

  const syncAutoApproveFromRunMode = useCallback(
    (mode: DesktopRunModeId) => {
      if (mode === 'yolo') {
        setAutoApprove(true);
      } else if (mode === 'plan') {
        setAutoApprove(false);
      } else {
        setAutoApprove(autoApproveFromPolicy(approvalPolicyRef.current));
      }
    },
    [],
  );

  const handleAutoApproveChange = useCallback((value: boolean) => {
    if (!composerAutoApproveToggleEnabled(approvalPolicyRef.current) && value) {
      return;
    }
    setAutoApprove(value);
  }, []);

  const handleApprovalPolicyChange = useCallback(
    async (policy: string): Promise<boolean> => {
      const next = policy.trim().toLowerCase();
      if (!desktopHost || !next || next === approvalPolicyRef.current.trim().toLowerCase()) {
        return false;
      }
      if (streamingRef.current && !(await confirmDialog(t('settings.saveRestartsSidecar')))) {
        return false;
      }
      setApprovalPolicyBusy(true);
      try {
        const settings = await fetchSystemSettings();
        const updated = { ...settings, approval_policy: next };
        await saveSystemSettings(updated);
        syncAutoApproveFromPolicy(next);
        persistNotifyMethod(updated.notify_method);
        return true;
      } catch (e) {
        toast.error(t('composer.approvalPolicySaveFailed', { message: (e as Error).message }));
        return false;
      } finally {
        setApprovalPolicyBusy(false);
      }
    },
    [desktopHost, streamingRef, syncAutoApproveFromPolicy, t],
  );

  const handleSystemSettingsSaved = useCallback(
    (settings: SystemSettings) => {
      syncAutoApproveFromPolicy(settings.approval_policy);
      persistNotifyMethod(settings.notify_method);
    },
    [syncAutoApproveFromPolicy],
  );

  useEffect(() => {
    if (!desktopHost) return;
    let cancelled = false;
    fetchSystemSettings()
      .then((s) => {
        if (!cancelled) {
          syncAutoApproveFromPolicy(s.approval_policy);
          persistNotifyMethod(s.notify_method);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [desktopHost, syncAutoApproveFromPolicy]);

  const clearApproval = useCallback(() => {
    setApproval(null);
  }, []);

  const showApprovalIfOwned = useCallback(
    (host: boolean, payload: ApprovalState) => {
      const tid = resolveActiveThreadTurn(streamRegistry, resumedThreadIdRef.current).threadId;
      void (async () => {
        const show = !tid || !host || (await threadOwnedByWindow(tid));
        if (show) {
          setApproval(payload);
        }
      })();
    },
    [resumedThreadIdRef, streamRegistry],
  );

  const handleApproveDecision = useCallback(
    async (decision: 'approve' | 'deny', rememberForSession = false) => {
      if (!approval) return;
      const { threadId, turnId } = resolveActiveThreadTurn(
        streamRegistry,
        resumedThreadIdRef.current,
      );
      if (!threadId || !turnId) {
        toast.warning(t('banner.approvalMissingThread'));
        setApproval(null);
        return;
      }
      setApprovalBusy(true);
      try {
        await postResolveApproval(
          threadId,
          turnId,
          approval.toolCallId,
          decision,
          decision === 'approve' ? rememberForSession : false,
        );
      } catch (e) {
        const err = e as Error & { status?: number };
        if (err.status === 409) {
          toast.warning(t('banner.approvalExpired'));
        } else {
          toast.error(t('banner.approvalSubmitFailed', { message: err.message }));
        }
      } finally {
        setApprovalBusy(false);
        setApproval(null);
      }
    },
    [approval, t, streamRegistry, resumedThreadIdRef],
  );

  return {
    approval,
    setApproval,
    approvalBusy,
    approvalPolicy,
    approvalPolicyBusy,
    autoApprove,
    setAutoApprove,
    handleAutoApproveChange,
    handleApprovalPolicyChange,
    syncAutoApproveFromPolicy,
    syncAutoApproveFromRunMode,
    handleSystemSettingsSaved,
    handleApproveDecision,
    showApprovalIfOwned,
    clearApproval,
  };
}
