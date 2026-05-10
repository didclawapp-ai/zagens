/** Sub-agent state tracked from SSE agent.* events */
export type AgentStatus = 'spawned' | 'running' | 'completed' | 'interrupted';

export interface AgentState {
  agentId: string;
  status: AgentStatus;
  toolCalls: AgentToolCall[];
  resultSummary: string | null;
  tokens: number;
  spawnedAt: number;
  completedAt: number | null;
}

export interface AgentToolCall {
  name: string;
  input?: string;
  output?: string;
  status: 'running' | 'done' | 'error';
}
