import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { RuntimeConnectionState } from '../api/client';
import type { NormalizedStreamEvent } from '../api/streamNormalize';
import {
  countActiveSubagents,
  detectNarrativeSpawnWithoutAgents,
} from '../lib/auditSpawnDetect';
import {
  isAgentSpawnToolName,
  parseAgentSpawnInput,
  type AgentSpawnMeta,
} from '../lib/agentSpawnMeta';
import {
  metaFromListRow,
  metaFromSpawn,
  upsertAgentInList,
} from '../lib/agentStateUpsert';
import { parseAgentIdFromSpawnOutput } from '../lib/chat/toolOutput';
import { isRuntimeApiAvailable } from '../lib/runtimeReachable';
import { SUBAGENT_STATE_POLL_STREAMING_MS } from '../lib/runtimePoll';
import { parseSubagentProgressStatus } from '../lib/subagentProgress';
import {
  fetchSubagentStateFromDisk,
  filterSubagentRowsForThread,
  type SubagentPollRow,
} from '../lib/subagentStatePoll';
import { shouldDispatchPanelForThread } from '../lib/panelChannel';
import type { AgentState, AgentToolCall } from '../types/agent';

type MessageLike = {
  role: string;
  tools?: { name: string; status?: string }[];
};

export type UseAgentPanelStateParams = {
  messages: MessageLike[];
  /** Current runtime thread — sub-agent rows are scoped to this id (grid parity with checklist/LHT). */
  resumedThreadId?: string | null;
  workspaceRoot?: string;
  streaming?: boolean;
  runtimeConn?: RuntimeConnectionState;
  runtimeSessionEstablished?: boolean;
};

export type UseAgentPanelStateResult = {
  agentStates: AgentState[];
  setAgentStates: React.Dispatch<React.SetStateAction<AgentState[]>>;
  pendingSpawnMetaRef: React.MutableRefObject<Map<string, AgentSpawnMeta>>;
  resetAgentPanel: () => void;
  onAgentSpawnToolStarted: (toolCallId: string, name: string, input: unknown) => void;
  onAgentSpawnToolCompleted: (toolCallId: string, toolName: string, mergedOutput: string) => void;
  applyAgentStreamEvent: (norm: NormalizedStreamEvent, originThreadId?: string) => boolean;
  subagentActiveCount: number;
  narrativeSpawnSuspected: boolean;
};

function mapSubAgentUiStatus(status: string): AgentState['status'] {
  if (status === 'Completed') return 'completed';
  if (status === 'Interrupted' || status === 'Failed' || status === 'Cancelled') {
    return 'interrupted';
  }
  return 'running';
}

function agentsForThread(all: AgentState[], threadId: string | null | undefined): AgentState[] {
  const tid = threadId?.trim();
  if (!tid) {
    return [];
  }
  return all.filter((a) => a.ownerThreadId === tid);
}

function patchFromPollRow(
  row: SubagentPollRow,
  threadId: string,
  existing?: AgentState,
): Partial<AgentState> & { status: AgentState['status'] } {
  const uiStatus = mapSubAgentUiStatus(row.status);
  const stepsTaken = Math.max(existing?.stepsTaken ?? 0, row.stepsTaken);
  const toolsExecuted = Math.max(existing?.toolsExecuted ?? 0, row.toolsExecuted);
  return {
    status: uiStatus,
    ownerThreadId: threadId,
    ...metaFromListRow(row),
    ...(row.progressStatus ? { progressStatus: row.progressStatus } : {}),
    stepsTaken,
    maxSteps: row.maxSteps,
    stepTimeoutMs: row.stepTimeoutMs,
    stuckSuspected: row.stuckSuspected,
    idleMs: row.idleMs,
    toolsExecuted,
    durationMs: row.durationMs > 0 ? row.durationMs : existing?.durationMs,
    completedAt:
      uiStatus === 'completed' || uiStatus === 'interrupted'
        ? (existing?.completedAt ?? Date.now())
        : null,
  };
}

/** Merge disk snapshots into in-memory rows for one thread (disk wins on overlap). */
function mergeDiskRowsIntoAgentState(
  prev: AgentState[],
  threadId: string,
  rows: SubagentPollRow[],
): AgentState[] {
  const tid = threadId.trim();
  if (!tid) {
    return prev;
  }
  const scoped = filterSubagentRowsForThread(rows, tid);
  if (scoped.length === 0) {
    return prev;
  }
  const otherThreads = prev.filter((a) => a.ownerThreadId !== tid);
  const existingForThread = new Map(
    prev.filter((a) => a.ownerThreadId === tid).map((a) => [a.agentId, a]),
  );
  let threadAgents: AgentState[] = [];
  for (const row of scoped) {
    if (!row.id) {
      continue;
    }
    const existing = existingForThread.get(row.id);
    threadAgents = upsertAgentInList(
      threadAgents,
      row.id,
      patchFromPollRow(row, tid, existing),
    );
  }
  for (const agent of existingForThread.values()) {
    if (!threadAgents.some((a) => a.agentId === agent.agentId)) {
      threadAgents = [...threadAgents, agent];
    }
  }
  return [...otherThreads, ...threadAgents];
}

export function useAgentPanelState({
  messages,
  resumedThreadId = null,
  workspaceRoot = '',
  streaming = false,
  runtimeConn = 'offline',
  runtimeSessionEstablished = false,
}: UseAgentPanelStateParams): UseAgentPanelStateResult {
  const [agentStates, setAgentStates] = useState<AgentState[]>([]);
  const pendingSpawnMetaRef = useRef<Map<string, AgentSpawnMeta>>(new Map());
  const ownerThreadId = resumedThreadId?.trim() ?? '';

  const resetAgentPanel = useCallback(() => {
    setAgentStates([]);
    pendingSpawnMetaRef.current.clear();
  }, []);

  useEffect(() => {
    const tid = resumedThreadId?.trim();
    if (!tid) {
      return;
    }
    const ws = workspaceRoot.trim();
    if (!ws) {
      return;
    }

    let cancelled = false;
    void (async () => {
      const rows = await fetchSubagentStateFromDisk(ws);
      if (cancelled) {
        return;
      }
      setAgentStates((prev) => mergeDiskRowsIntoAgentState(prev, tid, rows));
    })();

    return () => {
      cancelled = true;
    };
  }, [resumedThreadId, workspaceRoot]);

  const onAgentSpawnToolStarted = useCallback(
    (toolCallId: string, name: string, input: unknown) => {
      if (!isAgentSpawnToolName(name)) return;
      const meta = parseAgentSpawnInput(input);
      if (meta) {
        pendingSpawnMetaRef.current.set(toolCallId, meta);
      }
    },
    [],
  );

  const onAgentSpawnToolCompleted = useCallback(
    (toolCallId: string, toolName: string, mergedOutput: string) => {
      if (!isAgentSpawnToolName(toolName)) return;
      const agentId = parseAgentIdFromSpawnOutput(mergedOutput);
      const spawnMeta = pendingSpawnMetaRef.current.get(toolCallId);
      pendingSpawnMetaRef.current.delete(toolCallId);
      if (!agentId) return;
      if (!ownerThreadId) return;
      queueMicrotask(() => {
        setAgentStates((prev) =>
          upsertAgentInList(prev, agentId, {
            status: 'spawned',
            ownerThreadId,
            ...metaFromSpawn(spawnMeta),
          }),
        );
      });
    },
    [ownerThreadId],
  );

  const applyAgentStreamEvent = useCallback(
    (norm: NormalizedStreamEvent, originThreadId?: string): boolean => {
      if (!shouldDispatchPanelForThread(originThreadId)) {
        return false;
      }
      if (!ownerThreadId) {
        return false;
      }
      switch (norm.kind) {
        case 'agent_spawned':
          setAgentStates((prev) =>
            upsertAgentInList(prev, norm.agentId, {
              status: 'spawned',
              ownerThreadId,
              ...(norm.prompt ? { objective: norm.prompt } : {}),
            }),
          );
          return true;
        case 'agent_progress':
          setAgentStates((prev) => {
            const parsed = parseSubagentProgressStatus(norm.status ?? '');
            const existing = prev.find((a) => a.agentId === norm.agentId);
            let toolCalls = existing?.toolCalls ?? [];
            if (parsed.toolName) {
              toolCalls = appendOrUpdateToolCall(toolCalls, parsed.toolName, parsed.toolPhase, parsed.toolOk);
            }
            const toolsExecuted =
              parsed.toolPhase === 'finished'
                ? Math.max(existing?.toolsExecuted ?? 0, toolCalls.filter((t) => t.status !== 'running').length)
                : existing?.toolsExecuted;
            return upsertAgentInList(prev, norm.agentId, {
              status: 'running',
              ownerThreadId,
              ...(norm.status ? { progressStatus: norm.status } : {}),
              ...(parsed.stepsTaken !== undefined ? { stepsTaken: parsed.stepsTaken } : {}),
              ...(parsed.maxSteps !== undefined ? { maxSteps: parsed.maxSteps } : {}),
              ...(toolsExecuted !== undefined ? { toolsExecuted } : {}),
              toolCalls,
            });
          });
          return true;
        case 'agent_completed':
          setAgentStates((prev) =>
            upsertAgentInList(prev, norm.agentId, {
              status: 'completed',
              ownerThreadId,
              resultSummary: norm.result,
              completedAt: Date.now(),
            }),
          );
          return true;
        case 'agent_list': {
          setAgentStates((prev) => {
            const now = Date.now();
            let next = prev;
            for (const row of norm.agents) {
              if (!row.id) continue;
              const rowThreadId = row.ownerThreadId?.trim() || ownerThreadId;
              if (rowThreadId !== ownerThreadId) {
                continue;
              }
              const uiStatus = mapSubAgentUiStatus(row.status);
              next = upsertAgentInList(next, row.id, {
                status: uiStatus,
                ownerThreadId: rowThreadId,
                ...metaFromListRow(row),
                completedAt:
                  uiStatus === 'completed' || uiStatus === 'interrupted' ? now : null,
              });
            }
            return next;
          });
          return true;
        }
        default:
          return false;
      }
    },
    [ownerThreadId],
  );

  const runtimeReady = isRuntimeApiAvailable(runtimeConn, {
    streaming,
    sessionEstablished: runtimeSessionEstablished,
  });

  useEffect(() => {
    if (!streaming || !runtimeReady) {
      return;
    }
    const ws = workspaceRoot.trim();
    if (!ws) {
      return;
    }

    let cancelled = false;

    const poll = async () => {
      if (!ownerThreadId) {
        return;
      }
      const rows = await fetchSubagentStateFromDisk(ws);
      if (cancelled) {
        return;
      }
      const scoped = filterSubagentRowsForThread(rows, ownerThreadId);
      if (scoped.length === 0) {
        return;
      }
      setAgentStates((prev) => mergeDiskRowsIntoAgentState(prev, ownerThreadId, scoped));
    };

    void poll();
    const id = window.setInterval(() => {
      void poll();
    }, SUBAGENT_STATE_POLL_STREAMING_MS);

    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [ownerThreadId, runtimeReady, streaming, workspaceRoot]);

  const visibleAgentStates = useMemo(
    () => agentsForThread(agentStates, resumedThreadId),
    [agentStates, resumedThreadId],
  );

  const subagentActiveCount = useMemo(
    () => countActiveSubagents(visibleAgentStates),
    [visibleAgentStates],
  );

  const narrativeSpawnSuspected = useMemo(
    () => detectNarrativeSpawnWithoutAgents(messages, visibleAgentStates),
    [messages, visibleAgentStates],
  );

  return {
    agentStates: visibleAgentStates,
    setAgentStates,
    pendingSpawnMetaRef,
    resetAgentPanel,
    onAgentSpawnToolStarted,
    onAgentSpawnToolCompleted,
    applyAgentStreamEvent,
    subagentActiveCount,
    narrativeSpawnSuspected,
  };
}

function appendOrUpdateToolCall(
  prev: AgentToolCall[],
  name: string,
  phase: 'running' | 'finished' | undefined,
  ok: boolean | undefined,
): AgentToolCall[] {
  const next = [...prev];
  const last = next[next.length - 1];
  if (phase === 'running') {
    if (last && last.name === name && last.status === 'running') {
      return next;
    }
    next.push({ name, status: 'running' });
    return next;
  }
  if (phase === 'finished') {
    if (last && last.name === name && last.status === 'running') {
      next[next.length - 1] = {
        ...last,
        status: ok === false ? 'error' : 'done',
      };
      return next;
    }
    next.push({ name, status: ok === false ? 'error' : 'done' });
    return next;
  }
  return next;
}
