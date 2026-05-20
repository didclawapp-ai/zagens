import type { AgentState } from '../types/agent';

type ToolLike = { name: string; status?: string };

type MessageLike = {
  role: string;
  tools?: ToolLike[];
};

/** L7c-style: transcript shows agent_spawn but Sub-agent panel has no rows. */
export function detectNarrativeSpawnWithoutAgents(
  messages: MessageLike[],
  agentStates: AgentState[],
): boolean {
  if (agentStates.length > 0) {
    return false;
  }
  for (const m of messages) {
    if (m.role !== 'assistant') {
      continue;
    }
    for (const tool of m.tools ?? []) {
      const name = tool.name?.trim() ?? '';
      if (name === 'agent_spawn' || name === 'spawn_agent') {
        return true;
      }
    }
  }
  return false;
}

export function countActiveSubagents(agentStates: AgentState[]): number {
  return agentStates.filter((a) => a.status === 'spawned' || a.status === 'running').length;
}
