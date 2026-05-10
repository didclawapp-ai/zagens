import { useState } from 'react';
import type { AgentState } from '../types/agent';

interface Props {
  agents: AgentState[];
}

export default function AgentPanel({ agents }: Props) {
  const running = agents.filter((a) => a.status === 'spawned' || a.status === 'running');
  const completed = agents.filter((a) => a.status === 'completed');
  const interrupted = agents.filter((a) => a.status === 'interrupted');

  return (
    <div className="overflow-y-auto px-3 py-3 space-y-3">
      {/* Summary */}
      <div className="grid grid-cols-3 gap-2">
        <div className="rounded-lg border border-card-border bg-canvas-alt p-2 text-center">
          <div className="text-sm font-bold text-amber-text">{running.length}</div>
          <div className="text-[9px] text-t-text-muted mt-0.5">运行中</div>
        </div>
        <div className="rounded-lg border border-card-border bg-canvas-alt p-2 text-center">
          <div className="text-sm font-bold text-success">{completed.length}</div>
          <div className="text-[9px] text-t-text-muted mt-0.5">已完成</div>
        </div>
        <div className="rounded-lg border border-card-border bg-canvas-alt p-2 text-center">
          <div className="text-sm font-bold text-t-text-muted">{interrupted.length}</div>
          <div className="text-[9px] text-t-text-muted mt-0.5">已中断</div>
        </div>
      </div>

      {agents.length === 0 && (
        <p className="text-xs text-t-text-muted text-center py-6">暂无子代理活动。</p>
      )}

      {agents.map((a) => (
        <AgentCard key={a.agentId} agent={a} />
      ))}
    </div>
  );
}

function AgentCard({ agent }: { agent: AgentState }) {
  const [expanded, setExpanded] = useState(false);

  const dotColor =
    agent.status === 'completed'
      ? 'bg-success'
      : agent.status === 'interrupted'
        ? 'bg-t-error'
        : 'bg-amber animate-pulse';

  const label =
    agent.status === 'completed'
      ? '已完成'
      : agent.status === 'interrupted'
        ? '已中断'
        : '运行中';

  const duration =
    agent.completedAt != null
      ? ((agent.completedAt - agent.spawnedAt) / 1000).toFixed(1) + 's'
      : agent.status === 'running' || agent.status === 'spawned'
        ? ((Date.now() - agent.spawnedAt) / 1000).toFixed(0) + 's'
        : '—';

  return (
    <div
      className="rounded-lg border border-card-border bg-canvas-alt overflow-hidden cursor-pointer"
      onClick={() => setExpanded(!expanded)}
    >
      <div className="px-3 py-2 flex items-center gap-2">
        <span className={`inline-block w-2 h-2 rounded-full ${dotColor}`} />
        <span className="font-mono text-[10px] text-t-text-muted">{agent.agentId.slice(0, 12)}</span>
        <span className="ml-auto text-[10px] text-t-text-muted">
          {agent.toolCalls.length} 工具 · {agent.tokens > 0 ? `${(agent.tokens / 1000).toFixed(1)}k` : '—'} · {duration}
        </span>
        <span
          className={`text-[10px] font-medium ${
            agent.status === 'completed'
              ? 'text-success'
              : agent.status === 'interrupted'
                ? 'text-t-error-text'
                : 'text-amber-text'
          }`}
        >
          {label}
        </span>
      </div>
      {expanded && (
        <div className="border-t border-divider px-3 py-2 space-y-1.5">
          {agent.toolCalls.map((tc, i) => (
            <div key={i} className="text-[10px] text-t-text-muted">
              <span className="font-mono text-accent">{tc.name}</span>
              {tc.output && (
                <span className="ml-1 text-t-text-secondary">
                  → {tc.output.slice(0, 80)}
                </span>
              )}
            </div>
          ))}
          {agent.resultSummary && (
            <div className="mt-1 pt-1 border-t border-divider text-[10px] text-t-text-secondary leading-relaxed">
              {agent.resultSummary}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
