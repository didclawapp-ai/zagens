import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  fetchThreadScratchpadStatus,
  initThreadScratchpad,
  type ScratchpadStatus,
} from '../api/client';
import { useT } from '../i18n';
import {
  SCRATCHPAD_STATUS_POLL_IDLE_MS,
  SCRATCHPAD_STATUS_POLL_STREAMING_MS,
} from '../lib/runtimePoll';
import { PANEL_SCRATCHPAD_EVENT } from '../lib/panelChannel';
import { toast } from '../lib/toast';
import AuditScratchpadRunCard from './AuditScratchpadRunCard';

export interface AuditScratchpadPanelProps {
  threadId: string;
  /** Composer workspace root — used for empty-state path hint. */
  workspaceRoot?: string;
  /** Poll faster while the model is streaming or this panel is visible. */
  pollFast?: boolean;
  onOpenWorkspacePath?: (relPath: string) => void;
  subagentActiveCount?: number;
  narrativeSpawnSuspected?: boolean;
  /** Fired once per thread when scratchpad run_id first appears. */
  onDetected?: () => void;
}

function scratchpadRelPath(threadId: string): string {
  return `.deepseek/scratchpad/${threadId}/`;
}

export default function AuditScratchpadPanel({
  threadId,
  workspaceRoot = '',
  pollFast = false,
  onOpenWorkspacePath,
  subagentActiveCount = 0,
  narrativeSpawnSuspected = false,
  onDetected,
}: AuditScratchpadPanelProps) {
  const { t } = useT();
  const [status, setStatus] = useState<ScratchpadStatus | null>(null);
  const [initBusy, setInitBusy] = useState(false);
  const autoDetectThreadRef = useRef<string | null>(null);
  const onDetectedRef = useRef(onDetected);
  onDetectedRef.current = onDetected;

  const pathHint = useMemo(() => {
    const rel = scratchpadRelPath(threadId);
    const ws = workspaceRoot.trim();
    return ws ? `${ws.replace(/[/\\]+$/, '')}/${rel}` : rel;
  }, [threadId, workspaceRoot]);

  const previousRuns = status?.previous_runs ?? [];

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

  const handleInitScratchpad = useCallback(async () => {
    if (!threadId || initBusy) return;
    setInitBusy(true);
    try {
      const data = await initThreadScratchpad(threadId);
      setStatus(data);
      if (autoDetectThreadRef.current !== threadId) {
        autoDetectThreadRef.current = threadId;
        onDetectedRef.current?.();
      }
    } catch (e) {
      toast.error(t('auditScratchpad.initFailed', { message: (e as Error).message }));
    } finally {
      setInitBusy(false);
    }
  }, [initBusy, threadId, t]);

  if (!threadId) {
    return (
      <div className="p-4 text-sm text-t-text-muted">{t('auditScratchpad.needThread')}</div>
    );
  }

  if (!status?.run_id) {
    return (
      <div className="p-4 space-y-3 text-sm text-t-text-muted leading-relaxed">
        <p className="font-medium text-t-text">{t('auditScratchpad.noRun')}</p>
        <p>{t('auditScratchpad.noRunDetail')}</p>
        <p className="font-mono text-[11px] text-t-text-secondary break-all">
          {t('auditScratchpad.noRunPath', { path: pathHint })}
        </p>
        <button
          type="button"
          className="rounded-md border border-card-border bg-card px-3 py-1.5 text-xs font-medium text-t-text transition-colors hover:bg-hover disabled:opacity-50"
          disabled={initBusy}
          onClick={() => void handleInitScratchpad()}
        >
          {initBusy ? t('auditScratchpad.initBusy') : t('auditScratchpad.initScratchpad')}
        </button>
      </div>
    );
  }

  return (
    <div className="overflow-y-auto px-3 py-3 text-xs space-y-3">
      <AuditScratchpadRunCard
        status={status}
        t={t}
        isLatest
        defaultExpanded
        onOpenWorkspacePath={onOpenWorkspacePath}
        subagentActiveCount={subagentActiveCount}
        narrativeSpawnSuspected={narrativeSpawnSuspected}
      />

      {previousRuns.length > 0 ? (
        <div className="space-y-2">
          <p className="text-[11px] font-medium text-t-text-muted px-1">
            {t('auditScratchpad.previousRunsHeading', { count: String(previousRuns.length) })}
          </p>
          {previousRuns.map((run) => (
            <AuditScratchpadRunCard
              key={run.run_id ?? run.path}
              status={run}
              t={t}
              collapsible
              defaultExpanded={false}
              onOpenWorkspacePath={onOpenWorkspacePath}
            />
          ))}
        </div>
      ) : null}
    </div>
  );
}
