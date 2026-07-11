import type { AgentState } from '../types/agent';
import { agentTypeLabel, truncateObjective } from '../lib/agentSpawnMeta';
import { useT } from '../i18n';

/** Compact sub-agent status under an `agent_spawn` tool card (AgentPanel linkage). */
export function AgentSpawnInline({ agent }: { agent: AgentState }) {
  const { t } = useT();
  const dotColor =
    agent.status === 'completed'
      ? 'bg-success'
      : agent.status === 'interrupted'
        ? 'bg-t-error'
        : 'bg-amber animate-pulse';
  const label =
    agent.status === 'completed'
      ? t('agentPanel.completed')
      : agent.status === 'interrupted'
        ? t('agentPanel.interrupted')
        : t('agentPanel.running');
  const typeLabel = agentTypeLabel(agent.agentType);
  const title = agent.nickname?.trim() || typeLabel || agent.agentId.slice(0, 12);
  const objective = agent.objective?.trim();
  const objectivePreview = objective ? truncateObjective(objective, 120) : null;
  const toolCount = Math.max(agent.toolsExecuted ?? 0, agent.toolCalls.length);
  const stepsHint =
    agent.maxSteps != null && agent.maxSteps > 0
      ? t('agentPanel.stepProgress', {
          done: String(agent.stepsTaken ?? 0),
          max: String(agent.maxSteps),
        })
      : agent.stepsTaken != null && agent.stepsTaken > 0
        ? t('agentPanel.stepsTaken', { count: String(agent.stepsTaken) })
        : null;
  const duration =
    agent.durationMs != null && agent.durationMs > 0
      ? `${(agent.durationMs / 1000).toFixed(1)}s`
      : agent.completedAt != null
        ? `${((agent.completedAt - agent.spawnedAt) / 1000).toFixed(1)}s`
        : null;

  return (
    <div
      className="mt-1.5 rounded-md border border-accent/25 bg-accent-soft/40 px-2.5 py-2 text-[11px]"
      role="status"
      aria-live="polite"
    >
      <div className="flex items-start gap-2">
        <span className={`mt-1 inline-block h-1.5 w-1.5 shrink-0 rounded-full ${dotColor}`} />
        <div className="min-w-0 flex-1 space-y-0.5">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="font-medium text-t-text">{title}</span>
            <span
              className={`font-medium ${agent.status === 'completed' ? 'text-success' : agent.status === 'interrupted' ? 'text-t-error-text' : 'text-amber-text'}`}
            >
              {label}
            </span>
          </div>
          <div className="font-mono text-[9px] text-t-text-muted truncate">{agent.agentId}</div>
          {objectivePreview ? (
            <p className="text-t-text-secondary leading-snug line-clamp-2">{objectivePreview}</p>
          ) : (
            <p className="text-t-text-muted italic">{t('agentPanel.objectiveLoading')}</p>
          )}
          {agent.progressStatus ? (
            <p className="text-amber-text/90 truncate" title={agent.progressStatus}>
              {agent.progressStatus}
            </p>
          ) : null}
          <p className="text-[10px] text-t-text-muted">
            {t('agentPanel.toolsCount', { count: String(toolCount) })}
            {stepsHint ? ` · ${stepsHint}` : ''}
            {duration ? ` · ${duration}` : ''}
          </p>
          {agent.resultSummary && agent.status === 'completed' ? (
            <p className="text-t-text-secondary line-clamp-2">{agent.resultSummary}</p>
          ) : null}
        </div>
      </div>
    </div>
  );
}
