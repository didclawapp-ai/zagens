/** Sub-agent state tracked from SSE agent.* events */
export type AgentStatus = 'spawned' | 'running' | 'completed' | 'interrupted';

export interface AgentState {
  agentId: string;
  status: AgentStatus;
  /** Task prompt / objective from `agent_spawn` or `agent.list`. */
  objective?: string;
  /** Sub-agent type: explore, auditor, general, … */
  agentType?: string;
  role?: string;
  /** CRAFT / scratchpad work-package id when set at spawn. */
  taskId?: string;
  nickname?: string;
  /** Latest `agent.progress` status line. */
  progressStatus?: string;
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
