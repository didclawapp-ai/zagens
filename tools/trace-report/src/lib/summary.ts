import { eventKind } from './bundle';
import type { TraceBundle, TraceEventEnvelope } from '../types';

export type SummaryFinding = {
  severity: 'ok' | 'warn' | 'fail';
  text: string;
};

export type ExecutiveSummary = {
  headline: string;
  lead: string;
  bullets: string[];
  findings: SummaryFinding[];
};

function countEventKinds(events: TraceEventEnvelope[]): Record<string, number> {
  const counts: Record<string, number> = {};
  for (const envelope of events) {
    const kind = eventKind(envelope.payload as Record<string, unknown>);
    counts[kind] = (counts[kind] ?? 0) + 1;
  }
  return counts;
}

function humanizeEffect(key: string): string {
  return key.replace(/_/g, ' ');
}

function truncate(text: string, max = 160): string {
  const trimmed = text.trim();
  if (trimmed.length <= max) return trimmed;
  return `${trimmed.slice(0, max - 1)}…`;
}

function latestTurn(bundle: TraceBundle) {
  const turns = bundle.replay_summary.turns;
  return turns.length > 0 ? turns[turns.length - 1] : undefined;
}

function harnessProgress(bundle: TraceBundle): Record<string, unknown> | undefined {
  const graph = bundle.harness?.task_graph as Record<string, unknown> | undefined;
  return graph?.progress as Record<string, unknown> | undefined;
}

export function buildExecutiveSummary(bundle: TraceBundle): ExecutiveSummary {
  const { replay_summary: replay, events, analysis } = bundle;
  const turns = replay.turns;
  const coherentCount = turns.filter((t) => t.coherence_ok).length;
  const turnsWithEvents = turns.filter((t) => t.event_count > 0).length;
  const latest = latestTurn(bundle);
  const kindCounts = countEventKinds(events);
  const toolFinished = kindCounts.tool_call_finished ?? 0;
  const findings: SummaryFinding[] = [];
  const bullets: string[] = [];

  let headline: string;
  let lead: string;

  if (replay.coherence_ok) {
    headline = 'Kernel replay verified';
    lead =
      turns.length <= 1
        ? `This thread recorded ${events.length} kernel events in ${turns.length || 1} turn(s); replay coherence checks passed.`
        : `All ${coherentCount} turn(s) with events passed kernel replay coherence (${events.length} events total).`;
    findings.push({
      severity: 'ok',
      text: 'Turn projection matches persisted kernel_events (log-driven replay).',
    });
  } else {
    headline = 'Coherence check failed';
    lead =
      replay.coherence_error ??
      `${coherentCount}/${turnsWithEvents || turns.length} turns coherent across this thread.`;
    findings.push({
      severity: 'fail',
      text: 'At least one turn diverges from log-driven replay — inspect Turn Map for the first failing turn.',
    });
  }

  if (latest) {
    const outcome = latest.outcome ?? 'unknown';
    bullets.push(
      `Latest turn ${latest.turn_id} ended with outcome “${outcome}” (${latest.event_count} events).`,
    );
    if (!latest.coherence_ok && latest.coherence_error) {
      findings.push({
        severity: 'fail',
        text: `${latest.turn_id}: ${truncate(latest.coherence_error, 200)}`,
      });
    }
  }

  const failedTurn = turns.find((t) => !t.coherence_ok && t.coherence_error);
  if (failedTurn && failedTurn.turn_id !== latest?.turn_id) {
    findings.push({
      severity: 'fail',
      text: `First incoherent turn ${failedTurn.turn_id}: ${truncate(failedTurn.coherence_error ?? '', 200)}`,
    });
  }

  if (toolFinished > 0) {
    bullets.push(`${toolFinished} tool call(s) completed across the trace.`);
  }

  const guardBits: string[] = [];
  const loopGuard = kindCounts.loop_guard_triggered ?? 0;
  const stepLimit = kindCounts.step_limit_continuation ?? 0;
  const steer = kindCounts.steer_injected ?? 0;
  const capacity = kindCounts.capacity_checkpoint ?? 0;
  if (loopGuard > 0) guardBits.push(`${loopGuard} loop-guard trigger(s)`);
  if (stepLimit > 0) guardBits.push(`${stepLimit} step-limit continuation(s)`);
  if (steer > 0) guardBits.push(`${steer} LHT steer injection(s)`);
  if (capacity > 0) guardBits.push(`${capacity} capacity checkpoint(s)`);
  if (guardBits.length > 0) {
    bullets.push(`Guards / LHT: ${guardBits.join(', ')}.`);
    if (loopGuard > 0 || stepLimit > 0) {
      findings.push({
        severity: 'warn',
        text: 'Guard lane activity suggests the agent hit loop or step limits — review Timeline › Guards.',
      });
    }
  }

  const effectEntries = Object.entries(replay.effect_counts ?? {}).sort((a, b) => b[1] - a[1]);
  if (effectEntries.length > 0) {
    const top = effectEntries
      .slice(0, 4)
      .map(([k, v]) => `${humanizeEffect(k)} ×${v}`)
      .join(', ');
    bullets.push(`Top replay effects: ${top}.`);
  }

  const progress = harnessProgress(bundle);
  if (progress) {
    const pct = progress.percent;
    const open = progress.open_items;
    bullets.push(
      `Harness snapshot: ${pct != null ? `${String(pct)}% complete` : 'progress recorded'}${open != null ? `, ${String(open)} open checklist item(s)` : ''}.`,
    );
    if (typeof pct === 'number' && pct < 100 && (typeof open !== 'number' || open > 0)) {
      findings.push({
        severity: 'warn',
        text: 'Task graph was not fully complete at export time — cross-check Harness tab.',
      });
    }
  }

  const compactions = analysis?.compaction_timeline?.length ?? 0;
  const checkpoints = analysis?.capacity_checkpoints?.length ?? 0;
  if (compactions > 0 || checkpoints > 0) {
    bullets.push(
      `Memory plane: ${compactions} compaction event(s), ${checkpoints} capacity checkpoint(s).`,
    );
  }

  if (latest?.outcome === 'Interrupted' || latest?.outcome === 'interrupted') {
    findings.push({
      severity: 'warn',
      text: 'Latest turn ended as Interrupted — user cancel, error, or outer boundary may have stopped the loop.',
    });
  }

  if (bullets.length === 0) {
    bullets.push(`${events.length} kernel events captured; open Timeline for lane-level detail.`);
  }

  return { headline, lead, bullets, findings };
}
