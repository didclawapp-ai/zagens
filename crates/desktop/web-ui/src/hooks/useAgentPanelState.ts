import { useCallback, useMemo, useRef, useState } from 'react';
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
import type { AgentState } from '../types/agent';

type MessageLike = {
  role: string;
  tools?: { name: string; status?: string }[];
};

export type UseAgentPanelStateParams = {
  messages: MessageLike[];
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
