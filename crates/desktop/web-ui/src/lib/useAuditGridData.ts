import { useEffect, useMemo, useState } from 'react';
import { fetchThreadChecklist, fetchThreadScratchpadStatus } from '../api/client';
import {
  CHECKLIST_POLL_IDLE_MS,
  CHECKLIST_POLL_STREAMING_MS,
  SCRATCHPAD_STATUS_POLL_IDLE_MS,
  SCRATCHPAD_STATUS_POLL_STREAMING_MS,
} from './runtimePoll';
import {
  normalizeChecklistPayload,
  PANEL_CHECKLIST_EVENT,
  PANEL_SCRATCHPAD_EVENT,
  type ChecklistPanelPayload,
} from './panelChannel';
import type { ScratchpadStatus } from '../api/client';
import type { AgentState } from '../types/agent';

export interface AuditGridDataSnapshot {
  hasAudit: boolean;
  hasChecklist: boolean;
  hasAgents: boolean;
  hasAnyData: boolean;
}

export function useAuditGridData({
  threadId,
  streaming,
  runtimeSessionEstablished,
  agentStates,
}: {
  threadId: string | null;
  streaming: boolean;
  runtimeSessionEstablished: boolean;
  agentStates: AgentState[];
}): AuditGridDataSnapshot {
  const [scratchpadStatus, setScratchpadStatus] = useState<ScratchpadStatus | null>(null);
  const [checklistCount, setChecklistCount] = useState(0);

  useEffect(() => {
    setScratchpadStatus(null);
    setChecklistCount(0);
  }, [threadId]);

  useEffect(() => {
    if (!runtimeSessionEstablished || !threadId) {
      setScratchpadStatus(null);
      return;
    }
    let cancelled = false;
    const applyScratchpad = (data: ScratchpadStatus | null) => {
      if (!cancelled) {
        setScratchpadStatus(data);
      }
    };
    const loadScratchpad = async () => {
      try {
        applyScratchpad(await fetchThreadScratchpadStatus(threadId));
      } catch {
        /* keep snapshot */
      }
    };
    const onScratchpadPush = (ev: Event) => {
      const detail = (ev as CustomEvent<ScratchpadStatus | null>).detail;
      if (detail && typeof detail === 'object') {
        applyScratchpad(detail);
      }
    };
    void loadScratchpad();
    window.addEventListener(PANEL_SCRATCHPAD_EVENT, onScratchpadPush);
    const ms = streaming ? SCRATCHPAD_STATUS_POLL_STREAMING_MS : SCRATCHPAD_STATUS_POLL_IDLE_MS;
    const id = window.setInterval(() => void loadScratchpad(), ms);
    return () => {
      cancelled = true;
      window.removeEventListener(PANEL_SCRATCHPAD_EVENT, onScratchpadPush);
      window.clearInterval(id);
    };
  }, [threadId, runtimeSessionEstablished, streaming]);

  useEffect(() => {
    if (!runtimeSessionEstablished || !threadId) {
      setChecklistCount(0);
      return;
    }
    let cancelled = false;
    const applyChecklistCount = (count: number) => {
      if (!cancelled) {
        setChecklistCount(count);
      }
    };
    const loadChecklist = async () => {
      try {
        const data = await fetchThreadChecklist(threadId);
        if (data && Array.isArray(data.items) && data.items.length > 0) {
          applyChecklistCount(data.items.length);
        } else {
          applyChecklistCount(0);
        }
      } catch (e) {
        const status = (e as Error & { status?: number }).status;
        if (status === 404) {
          applyChecklistCount(0);
        }
      }
    };
    const onChecklistPush = (ev: Event) => {
      const normalized = normalizeChecklistPayload(
        (ev as CustomEvent<ChecklistPanelPayload | unknown>).detail,
      );
      applyChecklistCount(normalized?.items.length ?? 0);
    };
    void loadChecklist();
    window.addEventListener(PANEL_CHECKLIST_EVENT, onChecklistPush);
    const ms = streaming ? CHECKLIST_POLL_STREAMING_MS : CHECKLIST_POLL_IDLE_MS;
    const id = window.setInterval(() => void loadChecklist(), ms);
    return () => {
      cancelled = true;
      window.removeEventListener(PANEL_CHECKLIST_EVENT, onChecklistPush);
      window.clearInterval(id);
    };
  }, [threadId, runtimeSessionEstablished, streaming]);

  return useMemo((): AuditGridDataSnapshot => {
    const hasAudit = Boolean(scratchpadStatus?.run_id);
    const hasChecklist = checklistCount > 0;
    const hasAgents = agentStates.length > 0;
    return {
      hasAudit,
      hasChecklist,
      hasAgents,
      hasAnyData: hasAudit || hasChecklist || hasAgents,
    };
  }, [agentStates.length, checklistCount, scratchpadStatus?.run_id]);
}
