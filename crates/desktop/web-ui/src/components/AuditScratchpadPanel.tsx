import { useEffect, useMemo, useRef, useState } from 'react';
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

export interface AuditScratchpadPanelProps {
  threadId: string;
  /** Poll faster while the model is streaming or this panel is visible. */
  pollFast?: boolean;
  onOpenWorkspacePath?: (relPath: string) => void;
  subagentActiveCount?: number;
  narrativeSpawnSuspected?: boolean;
  /** Fired once per thread when scratchpad run_id first appears. */
  onDetected?: () => void;
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

export default function AuditScratchpadPanel({
  threadId,
  pollFast = false,
  onOpenWorkspacePath,
  subagentActiveCount = 0,
  narrativeSpawnSuspected = false,
  onDetected,
}: AuditScratchpadPanelProps) {
  const { t } = useT();
  const [status, setStatus] = useState<ScratchpadStatus | null>(null);
  const [inventoryOpen, setInventoryOpen] = useState(true);
  const autoDetectThreadRef = useRef<string | null>(null);
  const onDetectedRef = useRef(onDetected);
  onDetectedRef.current = onDetected;

  useEffect(() => {
    autoDetectThreadRef.current = null;
    setStatus(null);
  }, [threadId]);

  useEffect(() => {
    if (!threadId) {
      setStatus(null);
      return;
    }
    let cancelled = false;
    const apply = (data: ScratchpadStatus | null) => {
      if (cancelled) {
        return;
      }
      setStatus(data);
      if (data?.run_id && autoDetectThreadRef.current !== threadId) {
        autoDetectThreadRef.current = threadId;
        onDetectedRef.current?.();
      }
    };
    const load = async () => {
      try {
        const data = await fetchThreadScratchpadStatus(threadId);
        apply(data);
      } catch {
        if (!pollFast) {
          apply(null);
        }
      }
    };
    const onPanelPush = (ev: Event) => {
      const detail = (ev as CustomEvent<ScratchpadStatus | null>).detail;
      if (detail && typeof detail === 'object') {
        apply(detail);
      }
    };
    void load();
    window.addEventListener(PANEL_SCRATCHPAD_EVENT, onPanelPush);
    const intervalMs = pollFast
      ? SCRATCHPAD_STATUS_POLL_STREAMING_MS
      : SCRATCHPAD_STATUS_POLL_IDLE_MS;
    const timer = window.setInterval(() => void load(), intervalMs);
    return () => {
      cancelled = true;
      window.removeEventListener(PANEL_SCRATCHPAD_EVENT, onPanelPush);
      window.clearInterval(timer);
    };
  }, [threadId, pollFast]);

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

  if (!threadId) {
    return (
      <div className="p-4 text-sm text-t-text-muted">{t('auditScratchpad.needThread')}</div>
    );
  }

  if (!status?.run_id || !metrics) {
    return (
      <div className="p-4 text-sm text-t-text-muted leading-relaxed">{t('auditScratchpad.noRun')}</div>
    );
  }

  const areas: ScratchpadInventoryArea[] = status.areas ?? [];
  const resume = status.resume_area_id;
  const hasAttention =
    metrics.contractViolation ||
    metrics.dualTrackMismatch ||
    narrativeSpawnSuspected ||
    metrics.warnings.length > 0;

  const shellClass = hasAttention
    ? 'border border-amber/30 bg-canvas-alt'
    : 'border border-card-border bg-canvas-alt';

  const verified = status.findings_verified ?? 0;
  const open = status.findings_open ?? 0;
  const openHigh = status.findings_open_high ?? 0;
  const openMed = status.findings_open_medium ?? 0;
  const openLow = status.findings_open_low ?? 0;
  const verifiedHigh = status.findings_verified_high ?? 0;

  return (
    <div className="overflow-y-auto px-3 py-3 text-xs">
      <div className={`rounded-lg px-3 py-3 space-y-3 ${shellClass}`}>
        <div className="space-y-1">
          <div className="font-semibold text-sm text-t-text">{t('auditScratchpad.title')}</div>
          <div className="font-mono text-[11px] text-t-text-secondary break-all">
            {status.path ?? status.run_id}
          </div>
        </div>

        <div className="text-t-text leading-relaxed">
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
        </div>

        {metrics.checklistTotal > 0 ? (
          <p
            className={
              metrics.dualTrackMismatch
                ? 'rounded-md border border-amber/25 bg-amber-bg px-2 py-1 text-amber-text'
                : 'text-t-text-secondary'
            }
          >
            {t('auditScratchpad.checklistTrack', {
              completed: String(metrics.checklistCompleted),
              total: String(metrics.checklistTotal),
            })}
          </p>
        ) : null}

        {resume ? (
          <p className="text-t-text-secondary">
            {t('auditScratchpad.resumeArea')}{' '}
            <code className="rounded bg-hover px-1 py-0.5 font-mono text-[11px] text-t-text">
              {resume}
            </code>
          </p>
        ) : metrics.accounted >= metrics.total && metrics.total > 0 ? (
          <p className="font-medium text-success">{t('auditScratchpad.inventoryComplete')}</p>
        ) : null}

        <p className="text-t-text-muted">
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
        </p>

        {subagentActiveCount > 0 ? (
          <p className="text-t-text-secondary">
            {t('auditScratchpad.subagentsActive', { count: String(subagentActiveCount) })}
          </p>
        ) : null}

        <div className="flex flex-wrap gap-1.5">
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
              (w) => w !== 'notes_without_accounted' && w !== 'checklist_inventory_mismatch',
            )
            .map((w) => (
              <span
                key={w}
                className="inline-flex items-center rounded-md border border-amber/25 bg-amber-bg px-1.5 py-0.5 font-medium text-amber-text"
              >
                {w}
              </span>
            ))}
        </div>

        {areas.length > 0 ? (
          <div>
            <button
              type="button"
              className="w-full flex items-center justify-between rounded-md border border-card-border bg-card px-2.5 py-1.5 text-[11px] text-t-text-secondary transition-colors hover:bg-hover hover:text-t-text"
              aria-expanded={inventoryOpen}
              onClick={() => setInventoryOpen((v) => !v)}
            >
              <span>{t('auditScratchpad.inventoryTitle', { count: String(areas.length) })}</span>
              <span aria-hidden>{inventoryOpen ? '▴' : '▾'}</span>
            </button>
            {inventoryOpen ? (
              <ul className="mt-2 max-h-[min(50vh,320px)] overflow-y-auto rounded-md border border-card-border bg-card">
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
                      <code className="shrink-0 font-mono text-[10px] text-t-text-muted">
                        {area.id}
                      </code>
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
        ) : null}
      </div>
    </div>
  );
}
