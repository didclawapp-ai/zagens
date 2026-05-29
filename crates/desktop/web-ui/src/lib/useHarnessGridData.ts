import { useEffect, useMemo, useState } from 'react';
import { fetchThreadHarnessTaskGraph, fetchThreadChecklist, fetchThreadScratchpadStatus } from '../api/client';
import {
  CHECKLIST_POLL_IDLE_MS,
  CHECKLIST_POLL_STREAMING_MS,
  SCRATCHPAD_STATUS_POLL_IDLE_MS,
  SCRATCHPAD_STATUS_POLL_STREAMING_MS,
  TASK_GRAPH_POLL_IDLE_MS,
  TASK_GRAPH_POLL_STREAMING_MS,
} from './runtimePoll';
import {
  normalizeChecklistPayload,
  PANEL_CHECKLIST_EVENT,
  PANEL_SCRATCHPAD_EVENT,
  PANEL_TASK_GRAPH_EVENT,
  type ChecklistPanelPayload,
} from './panelChannel';
import type { HarnessTaskGraph } from './types/longHorizon';
import { SIDECAR_READY_PANEL_EVENT } from './sidecarPanelRecovery';
import type { ScratchpadStatus } from '../api/client';
import type { AgentState } from '../types/agent';

export interface HarnessGridDataSnapshot {
  hasAudit: boolean;
  hasChecklist: boolean;
  hasAgents: boolean;
  hasLongHorizon: boolean;
  hasAnyData: boolean;
}

function isActiveTaskGraph(graph: HarnessTaskGraph | null): boolean {
  if (!graph?.lht_enabled) {
    return false;
  }
  return Boolean(graph.incomplete && (graph.phases.length > 0 || graph.checklist.length > 0));
}

export function useHarnessGridData({
  threadId,
  streaming,
  runtimeSessionEstablished,
  agentStates,
}: {
  threadId: string | null;
  streaming: boolean;
  runtimeSessionEstablished: boolean;
  agentStates: AgentState[];
}): HarnessGridDataSnapshot {
  const [scratchpadStatus, setScratchpadStatus] = useState<ScratchpadStatus | null>(null);
  const [checklistCount, setChecklistCount] = useState(0);
  const [taskGraph, setTaskGraph] = useState<HarnessTaskGraph | null>(null);

  useEffect(() => {
    setScratchpadStatus(null);
    setChecklistCount(0);
    setTaskGraph(null);
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

  useEffect(() => {
    if (!runtimeSessionEstablished || !threadId) {
      setTaskGraph(null);
      return;
    }
    let cancelled = false;
    const applyGraph = (graph: HarnessTaskGraph | null) => {
      if (!cancelled) {
        setTaskGraph(graph);
      }
    };
    const loadGraph = async () => {
      try {
        const data = await fetchThreadHarnessTaskGraph(threadId);
        applyGraph(data as HarnessTaskGraph);
      } catch {
        /* keep snapshot */
      }
    };
    const onGraphPush = (ev: Event) => {
      const detail = (ev as CustomEvent<{ task_graph?: HarnessTaskGraph }>).detail;
      if (detail?.task_graph) {
        applyGraph(detail.task_graph);
      }
    };
    const onSidecarReady = () => {
      void loadGraph();
    };
    void loadGraph();
    window.addEventListener(PANEL_TASK_GRAPH_EVENT, onGraphPush);
    window.addEventListener(SIDECAR_READY_PANEL_EVENT, onSidecarReady);
    const ms = streaming ? TASK_GRAPH_POLL_STREAMING_MS : TASK_GRAPH_POLL_IDLE_MS;
    const id = window.setInterval(() => void loadGraph(), ms);
    return () => {
      cancelled = true;
      window.removeEventListener(PANEL_TASK_GRAPH_EVENT, onGraphPush);
      window.removeEventListener(SIDECAR_READY_PANEL_EVENT, onSidecarReady);
      window.clearInterval(id);
    };
  }, [threadId, runtimeSessionEstablished, streaming]);

  return useMemo((): HarnessGridDataSnapshot => {
    const hasAudit = Boolean(scratchpadStatus?.run_id);
    const hasChecklist = checklistCount > 0;
    const hasAgents = agentStates.length > 0;
    const hasLongHorizon = isActiveTaskGraph(taskGraph);
    return {
      hasAudit,
      hasChecklist,
      hasAgents,
      hasLongHorizon,
      hasAnyData: hasAudit || hasChecklist || hasAgents || hasLongHorizon,
    };
  }, [agentStates.length, checklistCount, scratchpadStatus?.run_id, taskGraph]);
}
