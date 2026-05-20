import { useEffect, useMemo, useState } from 'react';
import {
  fetchThreadScratchpadStatus,
  type ScratchpadInventoryArea,
  type ScratchpadStatus,
} from '../api/client';
import { useT } from '../i18n';
import {
  SCRATCHPAD_STATUS_POLL_IDLE_MS,
  SCRATCHPAD_STATUS_POLL_STREAMING_MS,
} from '../lib/runtimePoll';
import { PANEL_SCRATCHPAD_EVENT } from '../lib/panelChannel';

interface AuditScratchpadBarProps {
  threadId: string | null;
  streaming?: boolean;
  onOpenWorkspacePath?: (relPath: string) => void;
  /** Active sub-agents from SSE (D2.2). */
  subagentActiveCount?: number;
  /** Chat shows agent_spawn but panel has zero rows (D2.2). */
  narrativeSpawnSuspected?: boolean;
}

const AREA_STATUS_CLASS: Record<string, string> = {
  pending: 'border border-divider bg-hover text-t-text-muted',
  in_progress: 'border border-amber/25 bg-amber-bg text-amber-text',
  done: 'border border-success/25 bg-success-bg text-success',
  deferred: 'border border-accent/20 bg-accent-soft text-accent',
};

function areaStatusClass(status: string): string {
  return AREA_STATUS_CLASS[status] ?? AREA_STATUS_CLASS.pending;
}

function dismissStorageKey(threadId: string, runId: string): string {
  return `ds-pick:audit-bar-dismissed:${threadId}:${runId}`;
}

export default function AuditScratchpadBar({
  threadId,
  streaming = false,
  onOpenWorkspacePath,
  subagentActiveCount = 0,
  narrativeSpawnSuspected = false,
}: AuditScratchpadBarProps) {
  const { t } = useT();
  const [status, setStatus] = useState<ScratchpadStatus | null>(null);
  const [inventoryOpen, setInventoryOpen] = useState(false);
  const [dismissed, setDismissed] = useState(false);

  const runId = status?.run_id ?? null;
  const dismissKey =
    threadId && runId ? dismissStorageKey(threadId, runId) : null;

  useEffect(() => {
    if (!dismissKey) {
      setDismissed(false);
      return;
    }
    try {
      setDismissed(sessionStorage.getItem(dismissKey) === '1');
    } catch {
      setDismissed(false);
    }
  }, [dismissKey]);

  useEffect(() => {
    if (!threadId) {
      setStatus(null);
      return;
    }
    let cancelled = false;
    const load = async () => {
      try {
        const data = await fetchThreadScratchpadStatus(threadId);
        if (!cancelled) {
          setStatus(data);
        }
      } catch {
        if (!cancelled && !streaming) {
          setStatus(null);
        }
      }
    };
    const refresh = () => {
      void load();
    };
    const onPanelPush = (ev: Event) => {
      const detail = (ev as CustomEvent<ScratchpadStatus | null>).detail;
      if (!cancelled && detail && typeof detail === 'object') {
        setStatus(detail);
      }
    };
    void load();
    window.addEventListener(PANEL_SCRATCHPAD_EVENT, onPanelPush);
    const intervalMs = streaming
      ? SCRATCHPAD_STATUS_POLL_STREAMING_MS
      : SCRATCHPAD_STATUS_POLL_IDLE_MS;
    const timer = window.setInterval(refresh, intervalMs);
    return () => {
      cancelled = true;
      window.removeEventListener(PANEL_SCRATCHPAD_EVENT, onPanelPush);
      window.clearInterval(timer);
    };
  }, [threadId, streaming]);

  const metrics = useMemo(() => {
    if (!status?.run_id) {
      return null;
    }
    const total = status.areas_total ?? 0;
    const done = status.areas_done ?? 0;
    const deferred = status.areas_deferred ?? 0;
    const inProgress = status.areas_in_progress ?? 0;
    const accounted = done + deferred + inProgress;
    const pct = total > 0 ? Math.round((accounted / total) * 100) : 0;
    const notesTotal = status.notes_total ?? 0;
    const checklistCompleted = status.checklist_completed ?? 0;
    const checklistTotal = status.checklist_total ?? 0;
    const warnings = status.contract_warnings ?? [];
    const contractViolation =
      warnings.includes('notes_without_accounted') ||
      (notesTotal > 0 && accounted === 0);
    const dualTrackMismatch = warnings.includes('checklist_inventory_mismatch');
    return {
      total,
      done,
      deferred,
      inProgress,
      accounted,
      pct,
      notesTotal,
      checklistCompleted,
      checklistTotal,
      warnings,
      contractViolation,
      dualTrackMismatch,
    };
  }, [status]);

  if (!status?.run_id || !metrics || dismissed) {
    return null;
  }

  const dismissBar = () => {
    setDismissed(true);
    if (dismissKey) {
      try {
        sessionStorage.setItem(dismissKey, '1');
      } catch {
        /* ignore */
      }
    }
  };

  const areas: ScratchpadInventoryArea[] = status.areas ?? [];
  const resume = status.resume_area_id;
  const hasAttention =
    metrics.contractViolation ||
    metrics.dualTrackMismatch ||
    narrativeSpawnSuspected ||
    metrics.warnings.length > 0;

  const shellClass = hasAttention
    ? 'border border-card-border bg-canvas-alt text-t-text border-l-2 border-l-amber'
    : 'border border-card-border bg-canvas-alt text-t-text border-l-2 border-l-accent';

  const verified = status.findings_verified ?? 0;
  const open = status.findings_open ?? 0;
  const openHigh = status.findings_open_high ?? 0;
  const openMed = status.findings_open_medium ?? 0;
  const openLow = status.findings_open_low ?? 0;
  const verifiedHigh = status.findings_verified_high ?? 0;

  return (
    <div className="shrink-0 px-4 pb-2">
      <div
        className={`relative mx-auto w-full max-w-3xl rounded-lg px-3 py-2 pr-8 text-xs ${shellClass}`}
        role="status"
        aria-live="polite"
      >
        <button
          type="button"
          className="absolute right-1.5 top-1.5 flex h-5 w-5 items-center justify-center rounded-md text-sm leading-none text-t-text-muted transition-colors hover:bg-hover hover:text-t-text"
          aria-label={t('auditScratchpad.dismiss')}
          title={t('auditScratchpad.dismiss')}
          onClick={dismissBar}
        >
          ×
        </button>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <span className="font-semibold text-t-text">{t('auditScratchpad.title')}</span>
          <span className="text-t-text-secondary font-mono text-[11px]">
            {status.path ?? status.run_id}
          </span>
          <span className="text-t-text">
            {t('auditScratchpad.progress', {
              accounted: String(metrics.accounted),
              total: String(metrics.total),
              pct: String(metrics.pct),
            })}
            {metrics.done > 0 || metrics.inProgress > 0 || metrics.deferred > 0 ? (
              <span className="text-t-text-muted">
                {metrics.done > 0
                  ? ` · ${t('auditScratchpad.statusDone', { count: String(metrics.done) })}`
                  : ''}
                {metrics.inProgress > 0
                  ? ` · ${t('auditScratchpad.statusInProgress', { count: String(metrics.inProgress) })}`
                  : ''}
                {metrics.deferred > 0
                  ? ` · ${t('auditScratchpad.statusDeferred', { count: String(metrics.deferred) })}`
                  : ''}
              </span>
            ) : null}
          </span>
          {metrics.checklistTotal > 0 ? (
            <span
              className={
                metrics.dualTrackMismatch
                  ? 'rounded-md border border-amber/25 bg-amber-bg px-1.5 py-0.5 text-amber-text'
                  : 'text-t-text-secondary'
              }
            >
              {t('auditScratchpad.checklistTrack', {
                completed: String(metrics.checklistCompleted),
                total: String(metrics.checklistTotal),
              })}
            </span>
          ) : null}
          {resume ? (
            <span className="text-t-text-secondary">
              {t('auditScratchpad.resumeArea')}{' '}
              <code className="rounded bg-hover px-1 py-0.5 font-mono text-[11px] text-t-text">
                {resume}
              </code>
            </span>
          ) : metrics.accounted >= metrics.total && metrics.total > 0 ? (
            <span className="font-medium text-success">{t('auditScratchpad.inventoryComplete')}</span>
          ) : null}
          <span className="text-t-text-muted">
            {t('auditScratchpad.findingsStrip', {
              verified: String(verified),
              open: String(open),
            })}
            {openHigh + openMed + openLow + verifiedHigh > 0 ? (
              <span className="ml-1">
                {openHigh > 0 ? ` · ${t('auditScratchpad.findingsHigh', { count: String(openHigh) })}` : ''}
                {openMed > 0
                  ? ` · ${t('auditScratchpad.findingsMedium', { count: String(openMed) })}`
                  : ''}
                {openLow > 0 ? ` · ${t('auditScratchpad.findingsLow', { count: String(openLow) })}` : ''}
                {verifiedHigh > 0
                  ? ` · ✓${t('auditScratchpad.findingsHigh', { count: String(verifiedHigh) })}`
                  : ''}
              </span>
            ) : null}
          </span>
          {subagentActiveCount > 0 ? (
            <span className="text-t-text-secondary">
              {t('auditScratchpad.subagentsActive', { count: String(subagentActiveCount) })}
            </span>
          ) : null}
          {metrics.contractViolation ? (
            <span className="inline-flex items-center rounded-md border border-amber/25 bg-amber-bg px-1.5 py-0.5 font-medium text-amber-text">
              {t('auditScratchpad.contractViolation')}
            </span>
          ) : null}
          {metrics.dualTrackMismatch && !metrics.contractViolation ? (
            <span className="inline-flex items-center rounded-md border border-amber/25 bg-amber-bg px-1.5 py-0.5 font-medium text-amber-text">
              {t('auditScratchpad.dualTrackMismatch')}
            </span>
          ) : null}
          {narrativeSpawnSuspected ? (
            <span className="inline-flex items-center rounded-md border border-amber/25 bg-amber-bg px-1.5 py-0.5 font-medium text-amber-text">
              {t('auditScratchpad.narrativeSpawn')}
            </span>
          ) : null}
          {metrics.warnings
            .filter(
              (w) =>
                w !== 'notes_without_accounted' && w !== 'checklist_inventory_mismatch',
            )
            .map((w) => (
              <span
                key={w}
                className="inline-flex items-center rounded-md border border-amber/25 bg-amber-bg px-1.5 py-0.5 font-medium text-amber-text"
              >
                {w}
              </span>
            ))}
          {areas.length > 0 ? (
            <button
              type="button"
              className="ml-auto rounded-md border border-card-border bg-card px-2 py-0.5 text-[11px] text-t-text-secondary transition-colors hover:bg-hover hover:text-t-text"
              aria-expanded={inventoryOpen}
              title={t('auditScratchpad.toggleInventory')}
              onClick={() => setInventoryOpen((v) => !v)}
            >
              inventory {areas.length}
              {inventoryOpen ? ' ▴' : ' ▾'}
            </button>
          ) : null}
        </div>
        {inventoryOpen && areas.length > 0 ? (
          <ul className="mt-2 max-h-48 overflow-y-auto rounded-md border border-card-border bg-card">
            {areas.map((area) => {
              const notesCount = area.notes_count ?? 0;
              const path = area.path?.trim() ?? '';
              const canOpen = Boolean(path && onOpenWorkspacePath);
              return (
                <li
                  key={area.id}
                  className="flex items-center gap-2 border-b border-divider px-2 py-1.5 last:border-b-0"
                >
                  <span
                    className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide ${areaStatusClass(area.status)}`}
                  >
                    {area.status}
                  </span>
                  <code className="shrink-0 font-mono text-[10px] text-t-text-muted">{area.id}</code>
                  {canOpen ? (
                    <button
                      type="button"
                      className="min-w-0 flex-1 truncate text-left text-[11px] text-accent underline-offset-2 hover:underline"
                      title={t('auditScratchpad.openPath', { path })}
                      onClick={() => onOpenWorkspacePath?.(path)}
                    >
                      {path}
                    </button>
                  ) : (
                    <span className="min-w-0 flex-1 truncate text-[11px] text-t-text-secondary">
                      {path}
                    </span>
                  )}
                  {notesCount > 0 ? (
                    <span className="shrink-0 text-[10px] text-t-text-muted">
                      {t('auditScratchpad.areaNotes', { count: String(notesCount) })}
                    </span>
                  ) : null}
                </li>
              );
            })}
          </ul>
        ) : null}
      </div>
    </div>
  );
}
