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
import { fetchSubagentStateFromDisk } from '../lib/subagentStatePoll';
import type { AgentState } from '../types/agent';

type MessageLike = {
  role: string;
  tools?: { name: string; status?: string }[];
};

export type UseAgentPanelStateParams = {
  messages: MessageLike[];
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
  applyAgentStreamEvent: (norm: NormalizedStreamEvent) => boolean;
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

export function useAgentPanelState({
  messages,
  workspaceRoot = '',
  streaming = false,
  runtimeConn = 'offline',
  runtimeSessionEstablished = false,
}: UseAgentPanelStateParams): UseAgentPanelStateResult {
  const [agentStates, setAgentStates] = useState<AgentState[]>([]);
  const pendingSpawnMetaRef = useRef<Map<string, AgentSpawnMeta>>(new Map());

  const resetAgentPanel = useCallback(() => {
    setAgentStates([]);
    pendingSpawnMetaRef.current.clear();
  }, []);

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
      queueMicrotask(() => {
        setAgentStates((prev) =>
          upsertAgentInList(prev, agentId, {
            status: 'spawned',
            ...metaFromSpawn(spawnMeta),
          }),
        );
      });
    },
    [],
  );

  const applyAgentStreamEvent = useCallback((norm: NormalizedStreamEvent): boolean => {
    switch (norm.kind) {
      case 'agent_spawned':
        setAgentStates((prev) =>
          upsertAgentInList(prev, norm.agentId, {
            status: 'spawned',
            ...(norm.prompt ? { objective: norm.prompt } : {}),
          }),
        );
        return true;
      case 'agent_progress':
        setAgentStates((prev) =>
          upsertAgentInList(prev, norm.agentId, {
            status: 'running',
            ...(norm.status ? { progressStatus: norm.status } : {}),
          }),
        );
        return true;
      case 'agent_completed':
        setAgentStates((prev) =>
          upsertAgentInList(prev, norm.agentId, {
            status: 'completed',
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
            const uiStatus = mapSubAgentUiStatus(row.status);
            next = upsertAgentInList(next, row.id, {
              status: uiStatus,
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
  }, []);

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
      const rows = await fetchSubagentStateFromDisk(ws);
      if (cancelled || rows.length === 0) {
        return;
      }
      setAgentStates((prev) => {
        let next = prev;
        for (const row of rows) {
          if (!row.id) {
            continue;
          }
          const uiStatus = mapSubAgentUiStatus(row.status);
          next = upsertAgentInList(next, row.id, {
            status: uiStatus,
            ...metaFromListRow(row),
            ...(row.progressStatus ? { progressStatus: row.progressStatus } : {}),
            stepsTaken: row.stepsTaken,
            maxSteps: row.maxSteps,
            stepTimeoutMs: row.stepTimeoutMs,
            stuckSuspected: row.stuckSuspected,
            idleMs: row.idleMs,
            completedAt:
              uiStatus === 'completed' || uiStatus === 'interrupted' ? Date.now() : null,
          });
        }
        return next;
      });
    };

    void poll();
    const id = window.setInterval(() => {
      void poll();
    }, SUBAGENT_STATE_POLL_STREAMING_MS);

    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, [runtimeReady, streaming, workspaceRoot]);

  const subagentActiveCount = useMemo(
    () => countActiveSubagents(agentStates),
    [agentStates],
  );

  const narrativeSpawnSuspected = useMemo(
    () => detectNarrativeSpawnWithoutAgents(messages, agentStates),
    [messages, agentStates],
  );

  return {
    agentStates,
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
