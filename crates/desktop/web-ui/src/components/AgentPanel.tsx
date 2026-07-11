import { useCallback, useEffect, useMemo, useState } from 'react';
import type { RuntimeConnectionState } from '../api/client';
import { useT } from '../i18n';
import {
  fetchCraftBlackboardTasks,
  onCraftBlackboardChanged,
  type CraftBlackboardTaskSummary,
} from '../lib/craftBlackboard';
import { CRAFT_BLACKBOARD_POLL_MS } from '../lib/runtimePoll';
import { isRuntimeApiAvailable } from '../lib/runtimeReachable';
import { downloadTextFile, fetchSubagentJournalJson } from '../lib/subagentJournal';
import { agentTypeLabel, isLikelySubAgentId, truncateObjective } from '../lib/agentSpawnMeta';
import { toast } from '../lib/toast';
import type { AgentState } from '../types/agent';

interface Props {
  agents: AgentState[];
  /** Composer / thread workspace — CRAFT blackboards live under `{workspace}/.zagens/blackboards/`. */
  workspaceRoot: string;
  runtimeConn: RuntimeConnectionState;
  streaming?: boolean;
  runtimeSessionEstablished?: boolean;
}

export default function AgentPanel({
  agents,
  workspaceRoot,
  runtimeConn,
  streaming = false,
  runtimeSessionEstablished = false,
}: Props) {
  const { t } = useT();
  const runtimeReady = isRuntimeApiAvailable(runtimeConn, {
    streaming,
    sessionEstablished: runtimeSessionEstablished,
  });

  const visible = useMemo(
    () => agents.filter((a) => isLikelySubAgentId(a.agentId)),
    [agents],
  );
  const running = visible.filter((a) => a.status === 'spawned' || a.status === 'running');
  const completed = visible.filter((a) => a.status === 'completed');
  const interrupted = visible.filter((a) => a.status === 'interrupted');

  const [craftTasks, setCraftTasks] = useState<CraftBlackboardTaskSummary[]>([]);

  const refreshCraftTasks = useCallback(async () => {
    if (!runtimeReady) {
      setCraftTasks([]);
      return;
    }
    const ws = workspaceRoot.trim();
    if (!ws) {
      setCraftTasks([]);
      return;
    }
    try {
      const tasks = await fetchCraftBlackboardTasks(ws);
      setCraftTasks(tasks);
    } catch {
      // Keep last good snapshot on transient errors.
    }
  }, [runtimeReady, workspaceRoot]);

  useEffect(() => {
    void refreshCraftTasks();
    if (!runtimeReady) return;
    const interval = window.setInterval(() => {
      void refreshCraftTasks();
    }, CRAFT_BLACKBOARD_POLL_MS);
    const unsub = onCraftBlackboardChanged(() => {
      void refreshCraftTasks();
    });
    return () => {
      window.clearInterval(interval);
      unsub();
    };
  }, [refreshCraftTasks, runtimeReady]);

  return (
    <div className="overflow-y-auto px-3 py-3 space-y-3">
      <div className="grid grid-cols-3 gap-2">
        <div className="rounded-lg border border-card-border bg-canvas-alt p-2 text-center">
          <div className="text-sm font-bold text-amber-text">{running.length}</div>
          <div className="text-[9px] text-t-text-muted mt-0.5">{t('agentPanel.running')}</div>
        </div>
        <div className="rounded-lg border border-card-border bg-canvas-alt p-2 text-center">
          <div className="text-sm font-bold text-success">{completed.length}</div>
          <div className="text-[9px] text-t-text-muted mt-0.5">{t('agentPanel.completed')}</div>
        </div>
        <div className="rounded-lg border border-card-border bg-canvas-alt p-2 text-center">
          <div className="text-sm font-bold text-t-text-muted">{interrupted.length}</div>
          <div className="text-[9px] text-t-text-muted mt-0.5">{t('agentPanel.interrupted')}</div>
        </div>
      </div>

      {visible.length === 0 && (
        <p className="text-xs text-t-text-muted text-center py-6">{t('agentPanel.noAgents')}</p>
      )}

      {visible.map((a) => (
        <AgentCard key={a.agentId} agent={a} workspaceRoot={workspaceRoot} />
      ))}

      {craftTasks.length > 0 ? (
        <section className="pt-2 border-t border-divider space-y-2">
          <h3 className="text-[10px] font-semibold uppercase tracking-wide text-t-text-muted px-0.5">
            {t('agentPanel.craftTasksTitle')}
          </h3>
          {craftTasks.map((task) => (
            <CraftTaskCard key={task.taskId} task={task} />
          ))}
        </section>
      ) : null}
    </div>
  );
}

function verdictClass(verdict: string | null): string {
  if (!verdict) return 'text-t-text-muted';
  const v = verdict.toUpperCase();
  if (v === 'BLOCKER' || v === 'FAIL') return 'text-t-error-text font-semibold';
  if (v === 'MAJOR') return 'text-amber-text font-medium';
  if (v === 'PASS') return 'text-success font-medium';
  return 'text-t-text-secondary';
}

function CraftTaskCard({ task }: { task: CraftBlackboardTaskSummary }) {
  const { t } = useT();
  return (
    <div className="rounded-lg border border-card-border bg-canvas-alt px-3 py-2 space-y-1.5">
      <div className="font-mono text-[10px] text-accent truncate" title={task.taskId}>{task.taskId}</div>
      <div className="grid grid-cols-2 gap-x-3 gap-y-1 text-[10px]">
        <div className="text-t-text-muted">{t('agentPanel.craftExplorer')} <span className="text-t-text-secondary">{task.explorerDone ? t('agentPanel.craftYes') : t('agentPanel.craftDash')}</span></div>
        <div className="text-t-text-muted">{t('agentPanel.craftRounds')} <span className="text-t-text-secondary">{task.implementerRounds}</span></div>
        <div className="text-t-text-muted col-span-2">{t('agentPanel.craftReviewer')} <span className={verdictClass(task.reviewerVerdict)}>{task.reviewerVerdict ?? t('agentPanel.craftDash')}</span></div>
        {task.verifierSummary ? (
          <div className="text-[10px] text-t-text-muted col-span-2 line-clamp-2" title={task.verifierSummary}>
            {t('agentPanel.craftVerifier')}{' '}
            <span className="text-t-text-secondary">{task.verifierSummary}</span>
          </div>
        ) : null}
      </div>
    </div>
  );
}

function toolsDisplayCount(agent: AgentState): number {
  return Math.max(agent.toolsExecuted ?? 0, agent.toolCalls.length);
}

function AgentCard({ agent, workspaceRoot }: { agent: AgentState; workspaceRoot: string }) {
  const { t } = useT();
  const [expanded, setExpanded] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const [exporting, setExporting] = useState(false);
  const isActive = agent.status === 'running' || agent.status === 'spawned';
  const isTerminal = agent.status === 'completed' || agent.status === 'interrupted';

  useEffect(() => {
    if (!isActive) {
      return;
    }
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [isActive]);

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
  const duration =
    agent.durationMs != null && agent.durationMs > 0
      ? `${(agent.durationMs / 1000).toFixed(1)}s`
      : agent.completedAt != null
        ? `${((agent.completedAt - agent.spawnedAt) / 1000).toFixed(1)}s`
        : isActive
          ? `${((now - agent.spawnedAt) / 1000).toFixed(0)}s`
          : '—';
  const stepTimeoutSec =
    agent.stepTimeoutMs && agent.stepTimeoutMs > 0
      ? Math.round(agent.stepTimeoutMs / 1000)
      : null;
  const stepsLine =
    agent.maxSteps != null && agent.maxSteps > 0
      ? t('agentPanel.stepProgress', {
          done: String(agent.stepsTaken ?? 0),
          max: String(agent.maxSteps),
        })
      : agent.stepsTaken != null && agent.stepsTaken > 0
        ? t('agentPanel.stepsTaken', { count: String(agent.stepsTaken) })
        : null;
  const toolCount = toolsDisplayCount(agent);
  const typeLabel = agentTypeLabel(agent.agentType);
  const title = agent.nickname?.trim() || typeLabel || agent.agentId.slice(0, 12);
  const objective = agent.objective?.trim() ?? '';
  const objectivePreview = objective ? truncateObjective(objective, 200) : null;

  const onExportJournal = async (e: React.MouseEvent) => {
    e.stopPropagation();
    if (exporting) return;
    setExporting(true);
    try {
      const json = await fetchSubagentJournalJson(workspaceRoot, agent.agentId);
      if (!json) {
        toast.warning(t('agentPanel.exportJournalMissing'));
        return;
      }
      downloadTextFile(`subagent-journal-${agent.agentId.slice(0, 12)}.json`, json);
      toast.success(t('agentPanel.exportJournalOk'));
    } catch {
      toast.error(t('agentPanel.exportJournalMissing'));
    } finally {
      setExporting(false);
    }
  };

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
          <p className="text-[10px] text-t-text-muted pl-4 italic">{t('agentPanel.objectiveLoading')}</p>
        )}
        {agent.progressStatus ? (
          <p
            className={`text-[10px] pl-4 truncate ${isActive ? 'text-amber-text/90' : 'text-t-text-muted'}`}
            title={agent.progressStatus}
          >
            {agent.progressStatus}
          </p>
        ) : null}
        {stepsLine ? (
          <p className="text-[10px] text-t-text-muted pl-4">
            {stepsLine}
            {stepTimeoutSec != null && isActive
              ? ` · ${t('agentPanel.stepCap', { sec: String(stepTimeoutSec) })}`
              : null}
          </p>
        ) : null}
        {agent.stuckSuspected && isActive ? (
          <p className="text-[10px] text-t-error-text pl-4">{t('agentPanel.stuckSuspected')}</p>
        ) : null}
        {agent.taskId?.trim() ? (
          <p
            className="text-[9px] text-t-text-muted pl-4 font-mono truncate"
            title={agent.taskId}
          >
            {t('agentPanel.workPackage')}: {agent.taskId}
          </p>
        ) : null}
        <div className="flex items-center gap-2 pl-4 text-[10px] text-t-text-muted">
          <span>
            {t('agentPanel.toolsCount', { count: String(toolCount) })} ·{' '}
            {agent.tokens > 0 ? `${(agent.tokens / 1000).toFixed(1)}k` : '—'} · {duration}
          </span>
        </div>
      </div>
      {expanded && (
        <div
          className="border-t border-divider px-3 py-2 space-y-1.5"
          onClick={(e) => e.stopPropagation()}
        >
          {objective ? (
            <div className="text-[10px] text-t-text-secondary leading-relaxed whitespace-pre-wrap">
              {objective}
            </div>
          ) : null}
          {agent.toolCalls.map((tc, i) => (
            <div key={i} className="text-[10px] text-t-text-muted">
              <span className="font-mono text-accent">{tc.name}</span>
              <span className="ml-1 text-t-text-muted">({tc.status})</span>
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
          {isTerminal || toolCount > 0 ? (
            <button
              type="button"
              className="mt-1 text-[10px] text-accent hover:underline disabled:opacity-50"
              disabled={exporting}
              onClick={(e) => void onExportJournal(e)}
            >
              {exporting ? t('agentPanel.exportJournalBusy') : t('agentPanel.exportJournal')}
            </button>
          ) : null}
        </div>
      )}
    </div>
  );
}
