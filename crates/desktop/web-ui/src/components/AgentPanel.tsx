import { useMemo, useState } from 'react';
import type { AgentState } from '../types/agent';
import { agentTypeLabel, isLikelySubAgentId, truncateObjective } from '../lib/agentSpawnMeta';

interface Props {
  agents: AgentState[];
}

export default function AgentPanel({ agents }: Props) {
  const visible = useMemo(
    () => agents.filter((a) => isLikelySubAgentId(a.agentId)),
    [agents],
  );
  const running = visible.filter((a) => a.status === 'spawned' || a.status === 'running');
  const completed = visible.filter((a) => a.status === 'completed');
  const interrupted = visible.filter((a) => a.status === 'interrupted');

  return (
    <div className="overflow-y-auto px-3 py-3 space-y-3">
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

      {visible.length === 0 && (
        <p className="text-xs text-t-text-muted text-center py-6">暂无子代理活动。</p>
      )}

      {visible.map((a) => (
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

  const typeLabel = agentTypeLabel(agent.agentType);
  const title = agent.nickname?.trim() || typeLabel || agent.agentId.slice(0, 12);
  const objective = agent.objective?.trim() ?? '';
  const objectivePreview = objective ? truncateObjective(objective, 200) : null;

  return (
    <div
      className="rounded-lg border border-card-border bg-canvas-alt overflow-hidden cursor-pointer"
      onClick={() => setExpanded(!expanded)}
    >
      <div className="px-3 py-2.5 space-y-1.5">
        <div className="flex items-start gap-2">
          <span className={`mt-1 inline-block w-2 h-2 rounded-full shrink-0 ${dotColor}`} />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5 flex-wrap">
              <span className="text-xs font-medium text-t-text truncate">{title}</span>
              {typeLabel && agent.nickname?.trim() ? (
                <span className="text-[9px] px-1.5 py-0.5 rounded bg-accent-soft text-accent font-medium">
                  {typeLabel}
                </span>
              ) : null}
              {agent.role?.trim() ? (
                <span className="text-[9px] text-t-text-muted">{agent.role}</span>
              ) : null}
            </div>
            <div className="font-mono text-[9px] text-t-text-muted truncate">{agent.agentId}</div>
          </div>
          <span
            className={`shrink-0 text-[10px] font-medium ${
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

        {objectivePreview ? (
          <p className="text-[11px] text-t-text-secondary leading-relaxed line-clamp-3 pl-4">
            {objectivePreview}
          </p>
        ) : (
          <p className="text-[10px] text-t-text-muted pl-4 italic">任务描述加载中…</p>
        )}

        {agent.progressStatus && (agent.status === 'running' || agent.status === 'spawned') ? (
          <p className="text-[10px] text-amber-text/90 pl-4 truncate" title={agent.progressStatus}>
            {agent.progressStatus}
          </p>
        ) : null}

        {agent.taskId?.trim() ? (
          <p className="text-[9px] text-t-text-muted pl-4 font-mono truncate" title={agent.taskId}>
            工作包: {agent.taskId}
          </p>
        ) : null}

        <div className="flex items-center gap-2 pl-4 text-[10px] text-t-text-muted">
          <span>
            {agent.toolCalls.length} 工具 · {agent.tokens > 0 ? `${(agent.tokens / 1000).toFixed(1)}k` : '—'} ·{' '}
            {duration}
          </span>
        </div>
      </div>

      {expanded && (
        <div className="border-t border-divider px-3 py-2 space-y-1.5">
          {objective ? (
            <div className="text-[10px] text-t-text-secondary leading-relaxed whitespace-pre-wrap">
              {objective}
            </div>
          ) : null}
          {agent.toolCalls.map((tc, i) => (
            <div key={i} className="text-[10px] text-t-text-muted">
              <span className="font-mono text-accent">{tc.name}</span>
              {tc.output ? (
                <span className="ml-1 text-t-text-secondary">→ {tc.output.slice(0, 80)}</span>
              ) : null}
            </div>
          ))}
          {agent.resultSummary ? (
            <div className="mt-1 pt-1 border-t border-divider text-[10px] text-t-text-secondary leading-relaxed whitespace-pre-wrap">
              {agent.resultSummary}
            </div>
          ) : null}
        </div>
      )}
    </div>
  );
}
