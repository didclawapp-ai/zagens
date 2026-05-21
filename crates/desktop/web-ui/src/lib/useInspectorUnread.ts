import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { fetchTasks, fetchThreadChecklist } from '../api/client';
import type { RightPanelView } from '../components/RightPanel';
import type { AgentState } from '../types/agent';
import type { TaskSummary } from '../types/automation';
import {
  agentNavActivity,
  checklistNavActivity,
  markAgentsSeen,
  markChecklistSeen,
  markTasksSeen,
  taskNavActivity,
} from './inspectorUnread';
import {
  normalizeChecklistPayload,
  PANEL_CHECKLIST_EVENT,
  type ChecklistPanelPayload,
} from './panelChannel';

const TASK_POLL_MS = 15_000;
const CHECKLIST_POLL_MS = 20_000;

export function useInspectorUnread({
  agentStates,
  resumedThreadId,
  activeInspector,
  runtimeSessionEstablished,
  streaming = false,
}: {
  agentStates: AgentState[];
  resumedThreadId: string | null;
  activeInspector: RightPanelView;
  runtimeSessionEstablished: boolean;
  streaming?: boolean;
}) {
  const tasksRef = useRef<TaskSummary[]>([]);
  const [tasksForUnread, setTasksForUnread] = useState<TaskSummary[]>([]);
  const checklistRef = useRef<ChecklistPanelPayload | null>(null);
  const [checklistSnapshot, setChecklistSnapshot] = useState<ChecklistPanelPayload | null>(null);

  useEffect(() => {
    if (!runtimeSessionEstablished) {
      tasksRef.current = [];
      setTasksForUnread([]);
      return;
    }
    let cancelled = false;
    const poll = async () => {
      try {
        const tasks = await fetchTasks();
        if (cancelled) {
          return;
        }
        tasksRef.current = tasks;
        setTasksForUnread(tasks);
      } catch {
        /* keep last snapshot */
      }
    };
    void poll();
    const id = window.setInterval(() => void poll(), TASK_POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [runtimeSessionEstablished]);

  useEffect(() => {
    if (!runtimeSessionEstablished || !resumedThreadId) {
      checklistRef.current = null;
      setChecklistSnapshot(null);
      return;
    }
    let cancelled = false;
    const apply = (raw: unknown) => {
      const normalized = normalizeChecklistPayload(raw);
      checklistRef.current = normalized;
      setChecklistSnapshot(normalized);
    };
    const onPanelPush = (ev: Event) => {
      apply((ev as CustomEvent<unknown>).detail);
    };
    window.addEventListener(PANEL_CHECKLIST_EVENT, onPanelPush);
    const poll = async () => {
      try {
        const data = await fetchThreadChecklist(resumedThreadId);
        if (cancelled) {
          return;
        }
        apply(data);
      } catch {
        /* keep last snapshot */
      }
    };
    void poll();
    const ms = streaming ? 8_000 : CHECKLIST_POLL_MS;
    const id = window.setInterval(() => void poll(), ms);
    return () => {
      cancelled = true;
      window.clearInterval(id);
      window.removeEventListener(PANEL_CHECKLIST_EVENT, onPanelPush);
    };
  }, [resumedThreadId, runtimeSessionEstablished, streaming]);

  const taskActivity = useMemo(
    () => taskNavActivity(tasksForUnread, activeInspector === 'tasks'),
    [activeInspector, tasksForUnread],
  );

  const agentActivity = useMemo(
    () => agentNavActivity(agentStates, resumedThreadId, activeInspector === 'agents'),
    [activeInspector, agentStates, resumedThreadId],
  );

  const checklistActivity = useMemo(
    () =>
      checklistNavActivity(checklistSnapshot, resumedThreadId, activeInspector === 'checklist'),
    [activeInspector, checklistSnapshot, resumedThreadId],
  );

  const acknowledgeInspectorView = useCallback(
    (view: RightPanelView) => {
      if (view === 'tasks') {
        markTasksSeen(tasksRef.current);
      }
      if (view === 'agents') {
        markAgentsSeen(agentStates, resumedThreadId);
      }
      if (view === 'checklist') {
        markChecklistSeen(checklistRef.current, resumedThreadId);
      }
    },
    [agentStates, resumedThreadId],
  );

  return { taskActivity, agentActivity, checklistActivity, acknowledgeInspectorView };
}
