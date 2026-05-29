import { useCallback, useEffect, useState } from 'react';
import {
  fetchThreadHarnessCycles,
  fetchThreadHarnessTaskGraph,
  getThreadContext,
} from '../api/client';
import { useT } from '../i18n';
import {
  HARNESS_CYCLE_ADVANCED_EVENT,
  PANEL_TASK_GRAPH_EVENT,
  type TaskGraphPanelPayload,
} from '../lib/panelChannel';
import { SIDECAR_READY_PANEL_EVENT } from '../lib/sidecarPanelRecovery';
import type { ThreadContextSnapshot } from '../lib/contextUsage';
import {
  TASK_GRAPH_POLL_IDLE_MS,
  TASK_GRAPH_POLL_STREAMING_MS,
} from '../lib/runtimePoll';
import type {
  HarnessCycles,
  HarnessTaskGraph,
  LongHorizonPanelTab,
} from '../lib/types/longHorizon';

interface Props {
  threadId: string;
  pollFast?: boolean;
}

function statusSymbol(status: string): string {
  switch (status) {
    case 'in_progress':
      return '◎';
    case 'completed':
      return '●';
    default:
      return '○';
  }
}

function progressBar(pct: number): string {
  const filled = Math.min(10, Math.round(pct / 10));
  return '█'.repeat(filled) + '░'.repeat(10 - filled);
}

function TaskGraphView({ graph, t }: { graph: HarnessTaskGraph; t: (k: string, vars?: Record<string, string>) => string }) {
  return (
    <div className="space-y-3 text-xs text-t-text">
      {graph.objective ? (
        <p className="font-medium leading-snug">{graph.objective}</p>
      ) : null}
      <div className="text-t-text-muted">
        <span className="font-mono">{progressBar(graph.completion_pct)}</span>{' '}
        {graph.completion_pct}% · {t('longHorizon.openItems', { count: String(graph.open_items) })}
      </div>
      {(graph.lht_blocked || (graph.nudge_count ?? 0) > 0) && (
        <div className="flex flex-wrap gap-2">
          {graph.lht_blocked ? (
            <span className="rounded bg-amber-500/15 px-2 py-0.5 text-amber-700 dark:text-amber-300">
              {t('longHorizon.blocked')}
            </span>
          ) : null}
          {(graph.nudge_count ?? 0) > 0 ? (
            <span className="rounded bg-canvas-alt px-2 py-0.5 text-t-text-muted">
              {t('longHorizon.nudges', { count: String(graph.nudge_count ?? 0) })}
            </span>
          ) : null}
        </div>
      )}
      {graph.phases.length > 0 ? (
        <section>
          <h4 className="mb-1 font-semibold text-t-text-muted">{t('longHorizon.plan')}</h4>
          <ul className="space-y-1">
            {graph.phases.map((phase) => (
              <li
                key={phase.step}
                className={
                  phase.status === 'in_progress' ? 'font-medium text-t-text' : 'text-t-text-muted'
                }
              >
                <span className="mr-1">{statusSymbol(phase.status)}</span>
                {phase.step}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
      {graph.checklist.length > 0 ? (
        <section>
          <h4 className="mb-1 font-semibold text-t-text-muted">{t('longHorizon.checklist')}</h4>
          <ul className="space-y-1">
            {graph.checklist.map((item) => (
              <li
                key={item.id}
                className={
                  item.status === 'in_progress' ? 'font-medium text-t-text' : 'text-t-text-muted'
                }
              >
                <span className="mr-1">{statusSymbol(item.status)}</span>
                {item.content}
                {item.verify_command ? (
                  <span className="mt-0.5 block font-mono text-[10px] text-t-text-muted">
                    {item.verify_command}
                  </span>
                ) : null}
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}

function CycleView({
  cycles,
  t,
}: {
  cycles: HarnessCycles | null;
  t: (k: string, vars?: Record<string, string>) => string;
}) {
  if (!cycles || (cycles.briefings.length === 0 && !(cycles.archives?.length))) {
    return (
      <p className="text-xs text-t-text-muted">{t('longHorizon.cyclesEmpty')}</p>
    );
  }
  return (
    <div className="space-y-3 text-xs text-t-text">
      <p className="text-t-text-muted">
        {t('longHorizon.currentCycle', { n: String(cycles.current_cycle) })}
        {cycles.context_pressure_pct != null
          ? ` · ${t('longHorizon.contextPressure', {
              pct: String(cycles.context_pressure_pct),
            })}`
          : null}
      </p>
      {cycles.briefings.map((b) => (
        <details key={b.cycle} className="rounded border border-t-border/40 p-2">
          <summary className="cursor-pointer font-medium">
            {t('longHorizon.cycleN', { n: String(b.cycle) })}
            <span className="ml-2 font-normal text-t-text-muted">
              {b.token_estimate} tok
            </span>
          </summary>
          <p className="mt-2 whitespace-pre-wrap text-t-text-muted leading-snug">
            {b.briefing_preview}
          </p>
        </details>
      ))}
      {(cycles.archives ?? []).map((a) => (
        <div
          key={`arch-${a.cycle}`}
          className="rounded border border-dashed border-t-border/40 p-2 text-t-text-muted"
        >
          {t('longHorizon.cycleArchive', {
            n: String(a.cycle),
            count: String(a.message_count),
          })}
        </div>
      ))}
    </div>
  );
}

function ContextThresholdBar({
  usagePct,
  windowTokens,
  cycleThresholdTokens,
  lhtLowPct,
  lhtHighPct,
  t,
}: {
  usagePct: number;
  windowTokens: number;
  cycleThresholdTokens?: number | null;
  lhtLowPct?: number | null;
  lhtHighPct?: number | null;
  t: (k: string, vars?: Record<string, string>) => string;
}) {
  const cyclePct =
    cycleThresholdTokens != null && windowTokens > 0
      ? Math.min(99, Math.round((cycleThresholdTokens / windowTokens) * 100))
      : null;
  const low = lhtLowPct ?? 75;
  const high = lhtHighPct ?? 85;
  return (
    <div className="space-y-1">
      <div className="relative h-3 w-full overflow-hidden rounded bg-canvas-alt">
        {low < high ? (
          <div
            className="absolute inset-y-0 bg-amber-500/20"
            style={{ left: `${low}%`, width: `${high - low}%` }}
            title={t('longHorizon.lhtWarningBand', { low: String(low), high: String(high) })}
          />
        ) : null}
        {cyclePct != null ? (
          <div
            className="absolute inset-y-0 w-px bg-sky-500/80"
            style={{ left: `${cyclePct}%` }}
            title={t('longHorizon.cycleThresholdLine', {
              n: String(cycleThresholdTokens ?? ''),
            })}
          />
        ) : null}
        <div
          className="absolute inset-y-0 left-0 bg-t-text/30"
          style={{ width: `${Math.min(100, Math.max(0, usagePct))}%` }}
        />
      </div>
      <div className="flex flex-wrap gap-x-3 gap-y-0.5 text-[10px] text-t-text-muted">
        <span>{t('longHorizon.usageNow', { pct: String(usagePct) })}</span>
        {cyclePct != null ? (
          <span>{t('longHorizon.cycleAtPct', { pct: String(cyclePct) })}</span>
        ) : null}
        <span>{t('longHorizon.lhtBandShort', { low: String(low), high: String(high) })}</span>
      </div>
    </div>
  );
}

function ContextView({
  ctx,
  cycles,
  t,
}: {
  ctx: ThreadContextSnapshot | null;
  cycles: HarnessCycles | null;
  t: (k: string, vars?: Record<string, string>) => string;
}) {
  if (!ctx) {
    return <p className="text-xs text-t-text-muted">{t('longHorizon.contextEmpty')}</p>;
  }
  const pct = Math.round(ctx.usage_percent);
  const windowTokens =
    ctx.context_window_tokens ?? cycles?.context_window_tokens ?? 1_000_000;
  return (
    <div className="space-y-2 text-xs text-t-text">
      <ContextThresholdBar
        usagePct={pct}
        windowTokens={windowTokens}
        cycleThresholdTokens={cycles?.cycle_threshold_tokens}
        lhtLowPct={cycles?.lht_warning_low_pct}
        lhtHighPct={cycles?.lht_warning_high_pct}
        t={t}
      />
      <div>
        <span className="font-mono">{progressBar(pct)}</span> {pct}%
      </div>
      <ul className="space-y-1 text-t-text-muted">
        <li>
          {t('longHorizon.estimatedTokens', {
            n: String(ctx.estimated_input_tokens),
          })}
        </li>
        <li>
          {t('longHorizon.windowTokens', {
            n: String(windowTokens),
          })}
        </li>
        {cycles?.context_pressure_pct != null ? (
          <li>
            {t('longHorizon.contextPressure', {
              pct: String(cycles.context_pressure_pct),
            })}
          </li>
        ) : null}
        <li>
          {t('longHorizon.messageCount', { n: String(ctx.message_count) })}
        </li>
        {ctx.should_compact ? (
          <li className="text-amber-700 dark:text-amber-300">{t('longHorizon.shouldCompact')}</li>
        ) : null}
      </ul>
    </div>
  );
}

export default function LongHorizonPanel({ threadId, pollFast = false }: Props) {
  const { t } = useT();
  const [tab, setTab] = useState<LongHorizonPanelTab>('task');
  const [graph, setGraph] = useState<HarnessTaskGraph | null>(null);
  const [cycles, setCycles] = useState<HarnessCycles | null>(null);
  const [context, setContext] = useState<ThreadContextSnapshot | null>(null);

  const fetchGraph = useCallback(async () => {
    if (!threadId) {
      setGraph(null);
      return;
    }
    try {
      const data = await fetchThreadHarnessTaskGraph(threadId);
      setGraph(data as HarnessTaskGraph);
    } catch {
      if (!pollFast) {
        setGraph(null);
      }
    }
  }, [pollFast, threadId]);

  const fetchCycles = useCallback(async () => {
    if (!threadId) {
      setCycles(null);
      return;
    }
    try {
      const data = await fetchThreadHarnessCycles(threadId);
      setCycles(data as HarnessCycles);
    } catch {
      if (!pollFast) {
        setCycles(null);
      }
    }
  }, [pollFast, threadId]);

  const fetchContext = useCallback(async () => {
    if (!threadId) {
      setContext(null);
      return;
    }
    try {
      const data = await getThreadContext(threadId);
      setContext(data);
    } catch {
      if (!pollFast) {
        setContext(null);
      }
    }
  }, [pollFast, threadId]);

  useEffect(() => {
    setGraph(null);
    setCycles(null);
    setContext(null);
  }, [threadId]);

  useEffect(() => {
    const onPush = (ev: Event) => {
      const detail = (ev as CustomEvent<TaskGraphPanelPayload>).detail;
      if (detail?.task_graph) {
        setGraph(detail.task_graph);
      }
    };
    const onCycleAdvanced = () => {
      void fetchCycles();
      void fetchGraph();
    };
    const onSidecarReady = () => {
      void fetchGraph();
      void fetchCycles();
      void fetchContext();
    };
    window.addEventListener(PANEL_TASK_GRAPH_EVENT, onPush);
    window.addEventListener(HARNESS_CYCLE_ADVANCED_EVENT, onCycleAdvanced);
    window.addEventListener(SIDECAR_READY_PANEL_EVENT, onSidecarReady);
    void fetchGraph();
    void fetchCycles();
    void fetchContext();
    const ms = pollFast ? TASK_GRAPH_POLL_STREAMING_MS : TASK_GRAPH_POLL_IDLE_MS;
    const id = window.setInterval(() => {
      void fetchGraph();
      if (tab === 'cycle') void fetchCycles();
      if (tab === 'context') void fetchContext();
    }, ms);
    return () => {
      window.clearInterval(id);
      window.removeEventListener(PANEL_TASK_GRAPH_EVENT, onPush);
      window.removeEventListener(HARNESS_CYCLE_ADVANCED_EVENT, onCycleAdvanced);
      window.removeEventListener(SIDECAR_READY_PANEL_EVENT, onSidecarReady);
    };
  }, [fetchContext, fetchCycles, fetchGraph, pollFast, tab, threadId]);

  useEffect(() => {
    if (tab === 'cycle') void fetchCycles();
    if (tab === 'context') void fetchContext();
  }, [tab, fetchCycles, fetchContext]);

  const tabs: { id: LongHorizonPanelTab; label: string }[] = [
    { id: 'task', label: t('longHorizon.tabTask') },
    { id: 'cycle', label: t('longHorizon.tabCycle') },
    { id: 'context', label: t('longHorizon.tabContext') },
  ];

  const emptyTask =
    !graph || (!graph.phases.length && !graph.checklist.length);

  return (
    <div className="flex h-full flex-col p-2">
      <div className="mb-2 flex shrink-0 gap-1 border-b border-t-border/40 pb-2">
        {tabs.map(({ id, label }) => (
          <button
            key={id}
            type="button"
            onClick={() => setTab(id)}
            className={
              tab === id
                ? 'rounded bg-canvas-alt px-2 py-0.5 text-xs font-medium text-t-text'
                : 'rounded px-2 py-0.5 text-xs text-t-text-muted hover:text-t-text'
            }
          >
            {label}
          </button>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto">
        {tab === 'task' &&
          (emptyTask ? (
            <div className="flex h-full items-center justify-center p-4 text-xs text-t-text-muted">
              {t('longHorizon.empty')}
            </div>
          ) : (
            <TaskGraphView graph={graph!} t={t} />
          ))}
        {tab === 'cycle' && <CycleView cycles={cycles} t={t} />}
        {tab === 'context' && <ContextView ctx={context} cycles={cycles} t={t} />}
      </div>
    </div>
  );
}
