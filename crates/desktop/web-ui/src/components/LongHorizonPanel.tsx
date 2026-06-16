import { useCallback, useEffect, useRef, useState } from 'react';
import {
  fetchThreadHarnessCycles,
  fetchThreadHarnessTaskGraph,
  getThreadContext,
} from '../api/client';
import { useT } from '../i18n';
import {
  HARNESS_CYCLE_ADVANCED_EVENT,
  PANEL_CONTEXT_EVENT,
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
  HarnessCompletionGate,
  HarnessMacroLoop,
  HarnessNode,
  HarnessTaskGraph,
  LongHorizonPanelTab,
} from '../lib/types/longHorizon';

interface Props {
  threadId: string;
  /** Composer turn active (生成中) — timer ticks until this is false. */
  streaming?: boolean;
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

// Status-dot color: completed → green (live progress at a glance).
function statusDotClass(status: string): string {
  switch (status) {
    case 'completed':
      return 'text-emerald-600 dark:text-emerald-400';
    case 'in_progress':
      return 'text-sky-600 dark:text-sky-400';
    default:
      return 'text-t-text-muted';
  }
}

// Row text color matching the dot semantics.
function statusLineClass(status: string): string {
  switch (status) {
    case 'completed':
      return 'text-emerald-700 dark:text-emerald-400/90';
    case 'in_progress':
      return 'font-medium text-t-text';
    default:
      return 'text-t-text-muted';
  }
}

// mm:ss, rolling up to h:mm:ss past an hour. Number-only (no label).
function formatElapsed(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const s = total % 60;
  const m = Math.floor(total / 60) % 60;
  const h = Math.floor(total / 3600);
  const pad = (n: number) => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${pad(m)}:${pad(s)}`;
}

function filledBlocks(pct: number): number {
  return Math.min(10, Math.max(0, Math.round(pct / 10)));
}

function progressBar(pct: number): string {
  const filled = filledBlocks(pct);
  return '█'.repeat(filled) + '░'.repeat(10 - filled);
}

/** P0-3: checklist 100% but completion gate still has gaps (observe) or manifest failed. */
function isConditionalComplete(graph: HarnessTaskGraph): boolean {
  const gate = graph.completion_gate;
  if (!gate?.active || graph.completion_pct < 100 || graph.open_items > 0) {
    return false;
  }
  if (gate.last_manifest_passed === false) return true;
  if ((gate.first_gap_count ?? 0) > 0) return true;
  if ((gate.integration_gap_count ?? 0) > 0) return true;
  return false;
}

function macroPhaseLabel(
  phase: string | null | undefined,
  t: (k: string, vars?: Record<string, string>) => string,
): string {
  switch (phase) {
    case 'craft':
      return t('longHorizon.macroPhaseCraft');
    case 'remediation':
      return t('longHorizon.macroPhaseRemediation');
    case 'unmet':
      return t('longHorizon.macroPhaseUnmet');
    case 'implement':
    default:
      return t('longHorizon.macroPhaseImplement');
  }
}

function MacroLoopSummary({
  macro,
  t,
}: {
  macro: HarnessMacroLoop;
  t: (k: string, vars?: Record<string, string>) => string;
}) {
  if (!macro.configured && !macro.active) return null;
  const phaseLabel = macroPhaseLabel(macro.phase, t);
  return (
    <div className="mb-2 rounded border border-violet-500/30 bg-violet-500/10 px-2 py-1.5 text-[10px]">
      <div className="font-medium text-violet-800 dark:text-violet-200">
        {t('longHorizon.macroSummaryTitle')}
      </div>
      <ul className="mt-1 space-y-0.5 text-t-text-muted">
        <li>
          {t('longHorizon.macroPhaseLine', { phase: phaseLabel })}
          {macro.awaiting_confirm
            ? ` · ${t('longHorizon.macroAwaitingConfirm')}`
            : ''}
        </li>
        <li>
          {t('longHorizon.macroCyclesLine', {
            used: String(macro.macro_cycles_used),
            craft: String(macro.craft_rounds_this_cycle),
          })}
        </li>
        {(macro.last_blockers_count ?? 0) > 0 ? (
          <li className="text-amber-700 dark:text-amber-300">
            {t('longHorizon.macroBlockersLine', {
              n: String(macro.last_blockers_count),
            })}
          </li>
        ) : null}
        {macro.macro_task_id ? (
          <li className="font-mono text-[9px] opacity-80">{macro.macro_task_id}</li>
        ) : null}
      </ul>
    </div>
  );
}

function TaskGraphView({ graph, t }: { graph: HarnessTaskGraph; t: (k: string, vars?: Record<string, string>) => string }) {
  const conditionalComplete = isConditionalComplete(graph);
  return (
    <div className="space-y-3 text-xs text-t-text">
      {graph.macro_loop?.configured ? (
        <MacroLoopSummary macro={graph.macro_loop} t={t} />
      ) : null}
      {conditionalComplete ? (
        <div className="rounded border border-amber-500/40 bg-amber-500/10 px-2 py-1.5 text-amber-800 dark:text-amber-200">
          <span className="font-medium">{t('longHorizon.conditionalCompleteTitle')}</span>
          <p className="mt-0.5 text-[10px] leading-snug opacity-90">
            {t('longHorizon.conditionalCompleteHint')}
          </p>
        </div>
      ) : null}
      {graph.objective ? (
        <p className="font-medium leading-snug">{graph.objective}</p>
      ) : null}
      <div className="text-t-text-muted">
        <span className="font-mono">
          <span className="text-amber-600 dark:text-amber-400">
            {'█'.repeat(filledBlocks(graph.completion_pct))}
          </span>
          <span className="text-t-text-muted">
            {'░'.repeat(10 - filledBlocks(graph.completion_pct))}
          </span>
        </span>{' '}
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
        (() => {
          // DEMO5 #1 UI收尾: when the checklist is the completion authority
          // (non-empty) and the task is done (100%), the plan's still-pending
          // phases are a display-only outline, NOT open work. Annotate the header
          // and dim those phases so "100% complete" doesn't visually clash with a
          // row of grey pending dots ("进度100%但清单没关闭" confusion).
          const hasPendingPhases = graph.phases.some((p) => p.status !== 'completed');
          const planIsOutline = graph.checklist.length > 0 && hasPendingPhases;
          const dimPending = planIsOutline && graph.completion_pct >= 100;
          return (
            <section>
              <h4 className="mb-1 font-semibold text-t-text-muted">
                {t('longHorizon.plan')}
                {planIsOutline ? (
                  <span className="ml-1 font-normal text-[10px] text-t-text-muted">
                    {t('longHorizon.planOutlineNote')}
                  </span>
                ) : null}
              </h4>
              <ul className="space-y-1">
                {graph.phases.map((phase) => {
                  const muted = dimPending && phase.status !== 'completed';
                  return (
                    <li
                      key={phase.step}
                      className={
                        muted
                          ? 'text-t-text-muted line-through opacity-50'
                          : statusLineClass(phase.status)
                      }
                    >
                      <span className={`mr-1 ${statusDotClass(phase.status)}`}>
                        {statusSymbol(phase.status)}
                      </span>
                      {phase.step}
                    </li>
                  );
                })}
              </ul>
            </section>
          );
        })()
      ) : null}
      {graph.checklist.length > 0 ? (
        <section>
          <h4 className="mb-1 font-semibold text-t-text-muted">{t('longHorizon.checklist')}</h4>
          <ul className="space-y-1">
            {graph.checklist.map((item) => (
              <li key={item.id} className={statusLineClass(item.status)}>
                <span className={`mr-1 ${statusDotClass(item.status)}`}>
                  {statusSymbol(item.status)}
                </span>
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

// Clock time HH:MM:SS from epoch millis (local), for the node decision trail.
function formatClock(tsMs: number): string {
  const d = new Date(tsMs);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

// Color class per node kind (and verify verdict): continue/advance → green,
// skip/warning → amber, incomplete_stop/halt → red, verify mismatch → orange.
function nodeKindClass(kind: string, payload?: Record<string, unknown> | null): string {
  if (kind === 'verify_gate') {
    const verdict = typeof payload?.verdict === 'string' ? payload.verdict : '';
    return verdict === 'mismatch' || verdict === 'unverified_acceptance'
      ? 'text-orange-600 dark:text-orange-400'
      : 'text-t-text-muted';
  }
  if (
    kind === 'continue_injected' ||
    kind === 'step_limit_continue' ||
    kind === 'loop_guard_continue' ||
    kind === 'cycle_advanced'
  ) {
    return 'text-emerald-600 dark:text-emerald-400';
  }
  if (kind === 'unverified_acceptance_nudge' || kind === 'verify_mismatch_nudge') {
    // DEMO3 false-green / P0-2 verify mismatch guards — orange family.
    return 'text-orange-600 dark:text-orange-400';
  }
  if (kind === 'plan_checklist_drift_nudge') {
    return 'text-amber-600 dark:text-amber-400';
  }
  if (kind === 'integration_gate') {
    const enforce = payload?.enforce === true;
    const reinject = payload?.reinject === true;
    if (enforce && reinject) {
      return 'text-orange-600 dark:text-orange-400';
    }
    return 'text-sky-600 dark:text-sky-400';
  }
  if (kind === 'incomplete_stop' || kind === 'halt') {
    return 'text-red-600 dark:text-red-400';
  }
  if (kind === 'gate_skip' || kind === 'blocked' || kind === 'context_warning') {
    return 'text-amber-600 dark:text-amber-400';
  }
  if (
    kind === 'manifest_gate_start' ||
    kind === 'manifest_gate' ||
    kind === 'manifest_gate_result'
  ) {
    const passed =
      payload?.passed === true ||
      payload?.pass === true ||
      payload?.last_manifest_passed === true;
    const failed =
      payload?.passed === false ||
      payload?.pass === false ||
      payload?.observe === true;
    if (passed) return 'text-emerald-600 dark:text-emerald-400';
    if (failed) return 'text-amber-600 dark:text-amber-400';
    return 'text-sky-600 dark:text-sky-400';
  }
  if (kind === 'completion_audit') {
    return payload?.pass === true
      ? 'text-emerald-600 dark:text-emerald-400'
      : 'text-amber-600 dark:text-amber-400';
  }
  if (kind === 'audit_unmet') {
    return 'text-red-600 dark:text-red-400';
  }
  if (kind === 'macro_phase') {
    const phase = typeof payload?.phase === 'string' ? payload.phase : '';
    if (phase === 'craft' || phase === 'remediation') {
      return 'text-violet-600 dark:text-violet-400';
    }
    if (payload?.awaiting_confirm === true) {
      return 'text-amber-600 dark:text-amber-400';
    }
    return 'text-sky-600 dark:text-sky-400';
  }
  if (kind === 'macro_craft_start' || kind === 'macro_craft_result') {
    return 'text-violet-600 dark:text-violet-400';
  }
  if (kind === 'macro_unmet') {
    return 'text-red-600 dark:text-red-400';
  }
  return 'text-t-text-muted';
}

// Terse key=value summary from the node payload (reason / open_items /
// nudge_count / verdict / converted / emitted), in a stable order.
function nodePayloadSummary(payload?: Record<string, unknown> | null): string {
  if (!payload) return '';
  const keys = [
    'verdict',
    'reason',
    'open_items',
    'nudge_count',
    'emitted',
    'converted',
    'item',
    'manifest_round',
    'audit_round',
    'first_gap_count',
    'failing_count',
    'missing_deliverables',
    'pass',
    'passed',
    'observe',
    'enforce',
    'gap_count',
    'gate_reinject_while_blocked',
    'phase',
    'macro_cycle',
    'task_id',
    'agent_id',
    'blockers_count',
    'remaining_blockers',
    'awaiting_confirm',
    'macro_cycles_used',
  ];
  const parts: string[] = [];
  for (const k of keys) {
    const v = payload[k];
    if (v != null && (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean')) {
      parts.push(`${k}=${v}`);
    }
  }
  return parts.join(' · ');
}

function CompletionGateSummary({
  gate,
  t,
}: {
  gate: HarnessCompletionGate;
  t: (k: string, vars?: Record<string, string>) => string;
}) {
  if (!gate.active) return null;
  const mode = gate.mode ?? '—';
  const manifestOk =
    gate.last_manifest_passed === true
      ? t('longHorizon.gateManifestOk')
      : gate.last_manifest_passed === false
        ? t('longHorizon.gateManifestFail')
        : t('longHorizon.gateManifestUnknown');
  const auditOk =
    gate.last_audit_pass === true
      ? t('longHorizon.gateAuditOk')
      : gate.last_audit_pass === false
        ? t('longHorizon.gateAuditFail')
        : t('longHorizon.gateAuditUnknown');
  return (
    <div className="mb-2 rounded border border-t-border/40 bg-t-surface-elevated/50 px-2 py-1.5 text-[10px]">
      <div className="font-medium text-t-text-secondary">{t('longHorizon.gateSummaryTitle')}</div>
      <ul className="mt-1 space-y-0.5 text-t-text-muted">
        <li>
          {t('longHorizon.gateMode', { mode })}{' '}
          · {t('longHorizon.gateRounds', {
            manifest: String(gate.manifest_round),
            audit: String(gate.audit_round),
          })}
        </li>
        {gate.auto_verify_replay || gate.toolchain_gate ? (
          <li>
            {t('longHorizon.gateGenericSources', {
              replay: gate.auto_verify_replay ?? 'off',
              toolchain: gate.toolchain_gate ?? 'off',
            })}
          </li>
        ) : null}
        <li>
          {manifestOk} · {auditOk}
        </li>
        {gate.first_gap_count != null ? (
          <li>{t('longHorizon.gateFirstGap', { n: String(gate.first_gap_count) })}</li>
        ) : null}
        {(gate.integration_gap_count ?? 0) > 0 ? (
          <li className="text-amber-700 dark:text-amber-300">
            {t('longHorizon.gateIntegrationGap', {
              n: String(gate.integration_gap_count),
            })}
          </li>
        ) : null}
        {gate.gate_reinject_while_blocked > 0 ? (
          <li className="text-amber-700 dark:text-amber-300">
            {t('longHorizon.gateReinjectBlocked', {
              n: String(gate.gate_reinject_while_blocked),
            })}
          </li>
        ) : null}
        {gate.last_unmet_reason ? (
          <li className="text-red-700 dark:text-red-300">
            {t('longHorizon.gateAuditUnmet', { reason: gate.last_unmet_reason })}
          </li>
        ) : null}
      </ul>
    </div>
  );
}

function NodesView({
  nodes,
  completionGate,
  macroLoop,
  t,
}: {
  nodes: HarnessNode[];
  completionGate?: HarnessCompletionGate | null;
  macroLoop?: HarnessMacroLoop | null;
  t: (k: string, vars?: Record<string, string>) => string;
}) {
  const hasGate = completionGate?.active === true;
  const hasMacro = macroLoop?.configured === true;
  if (nodes.length === 0 && !hasGate && !hasMacro) {
    return <p className="text-xs text-t-text-muted">{t('longHorizon.nodesEmpty')}</p>;
  }
  // Newest first for readability.
  const ordered = [...nodes].reverse();
  const macroNodes = ordered.filter((n) => n.kind.startsWith('macro_'));
  const microNodes = ordered.filter((n) => !n.kind.startsWith('macro_'));
  return (
    <div className="space-y-2 text-xs">
      {hasMacro && macroLoop ? <MacroLoopSummary macro={macroLoop} t={t} /> : null}
      {hasGate && completionGate ? (
        <CompletionGateSummary gate={completionGate} t={t} />
      ) : null}
      {macroNodes.length > 0 ? (
        <p className="text-[10px] font-semibold uppercase tracking-wider text-violet-700 dark:text-violet-300">
          {t('longHorizon.macroNodesSection')}
        </p>
      ) : null}
      <ul className="space-y-1">
      {macroNodes.map((n, i) => (
        <li
          key={`macro-${n.ts_ms}-${i}`}
          className="flex flex-col gap-0.5 border-b border-t-border/20 pb-1 last:border-0"
        >
          <div className="flex items-baseline gap-2">
            <span className="font-mono text-[10px] tabular-nums text-t-text-muted">
              {formatClock(n.ts_ms)}
            </span>
            <span className={`font-medium ${nodeKindClass(n.kind, n.payload)}`}>
              {n.kind}
            </span>
          </div>
          {nodePayloadSummary(n.payload) ? (
            <span className="pl-[3.25rem] font-mono text-[10px] text-t-text-muted">
              {nodePayloadSummary(n.payload)}
            </span>
          ) : null}
        </li>
      ))}
      </ul>
      {microNodes.length > 0 && macroNodes.length > 0 ? (
        <p className="pt-1 text-[10px] font-semibold uppercase tracking-wider text-t-text-muted">
          {t('longHorizon.microNodesSection')}
        </p>
      ) : null}
    <ul className="space-y-1">
      {microNodes.map((n, i) => (
        <li
          key={`${n.ts_ms}-${i}`}
          className="flex flex-col gap-0.5 border-b border-t-border/20 pb-1 last:border-0"
        >
          <div className="flex items-baseline gap-2">
            <span className="font-mono text-[10px] tabular-nums text-t-text-muted">
              {formatClock(n.ts_ms)}
            </span>
            <span className={`font-medium ${nodeKindClass(n.kind, n.payload)}`}>
              {n.kind}
            </span>
          </div>
          {nodePayloadSummary(n.payload) ? (
            <span className="pl-[3.25rem] font-mono text-[10px] text-t-text-muted">
              {nodePayloadSummary(n.payload)}
            </span>
          ) : null}
        </li>
      ))}
    </ul>
    </div>
  );
}

export default function LongHorizonPanel({ threadId, streaming = false, pollFast = false }: Props) {
  const { t } = useT();
  const [tab, setTab] = useState<LongHorizonPanelTab>('task');
  const [graph, setGraph] = useState<HarnessTaskGraph | null>(null);
  const [cycles, setCycles] = useState<HarnessCycles | null>(null);
  const [context, setContext] = useState<ThreadContextSnapshot | null>(null);
  const [elapsedMs, setElapsedMs] = useState(0);
  /** Accumulated ms from frozen segments (same thread, no per-round reset at 100%). */
  const frozenMsRef = useRef(0);
  /** Start of the current ticking segment (while completion < 100%). */
  const segmentStartRef = useRef<number | null>(null);

  const taskActive =
    !!graph && (graph.phases.length > 0 || graph.checklist.length > 0);
  const taskCompleted = !!graph && graph.completion_pct >= 100;
  const conditionalComplete = !!graph && isConditionalComplete(graph);
  /** Timer ticks while composer turn is active (生成中), not when checklist hits 100%. */
  const sessionInProgress = streaming && taskActive;

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
    frozenMsRef.current = 0;
    segmentStartRef.current = null;
    setElapsedMs(0);
  }, [threadId]);

  // Client-side stopwatch: ticks while the composer turn is active (streaming / 生成中).
  // Includes tool execution between model chunks. Freezes when streaming ends; accumulates
  // across LHT reinject rounds without resetting at 100% checklist. Only resets on threadId change.
  useEffect(() => {
    const freezeSegment = () => {
      if (segmentStartRef.current !== null) {
        frozenMsRef.current += Date.now() - segmentStartRef.current;
        segmentStartRef.current = null;
        setElapsedMs(frozenMsRef.current);
      }
    };

    if (!threadId) {
      freezeSegment();
      return;
    }

    if (!streaming) {
      freezeSegment();
      return;
    }

    if (!taskActive) {
      return;
    }

    if (segmentStartRef.current === null) {
      segmentStartRef.current = Date.now();
    }
    const tick = () => {
      const segment = segmentStartRef.current
        ? Date.now() - segmentStartRef.current
        : 0;
      setElapsedMs(frozenMsRef.current + segment);
    };
    tick();
    const id = window.setInterval(tick, 1000);
    return () => window.clearInterval(id);
  }, [streaming, taskActive, threadId]);

  useEffect(() => {
    const onPush = (ev: Event) => {
      const detail = (ev as CustomEvent<TaskGraphPanelPayload>).detail;
      if (detail?.task_graph) {
        setGraph(detail.task_graph);
      }
    };
    // Live context updates while streaming (channel C `panel.context`), so the
    // Context tab no longer waits on the 30s task-graph poll.
    const onContext = (ev: Event) => {
      const detail = (ev as CustomEvent<ThreadContextSnapshot>).detail;
      if (detail && typeof detail.estimated_input_tokens === 'number') {
        setContext(detail);
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
    window.addEventListener(PANEL_CONTEXT_EVENT, onContext);
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
      window.removeEventListener(PANEL_CONTEXT_EVENT, onContext);
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
    { id: 'nodes', label: t('longHorizon.tabNodes') },
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
        {taskActive ? (
          <span
            className={`ml-auto self-center font-mono text-xs tabular-nums ${
              sessionInProgress
                ? 'text-t-text-muted'
                : taskCompleted
                  ? conditionalComplete
                    ? 'text-amber-600 dark:text-amber-400'
                    : 'text-emerald-600 dark:text-emerald-400'
                  : 'text-t-text-muted'
            }`}
            title={
              sessionInProgress
                ? t('longHorizon.timerRunning')
                : t('longHorizon.timerFrozen')
            }
          >
            {formatElapsed(elapsedMs)}
          </span>
        ) : null}
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
        {tab === 'nodes' && (
          <NodesView
            nodes={graph?.recent_nodes ?? []}
            completionGate={graph?.completion_gate}
            macroLoop={graph?.macro_loop}
            t={t}
          />
        )}
      </div>
    </div>
  );
}
