import { useEffect, useState } from 'react';
import { fetchThreadScratchpadStatus, type ScratchpadStatus } from '../api/client';

interface AuditScratchpadBarProps {
  threadId: string | null;
  /** Poll faster while the assistant turn is streaming (audit in progress). */
  streaming?: boolean;
}

export default function AuditScratchpadBar({ threadId, streaming = false }: AuditScratchpadBarProps) {
  const [status, setStatus] = useState<ScratchpadStatus | null>(null);

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
        if (!cancelled) {
          setStatus(null);
        }
      }
    };
    const refresh = () => {
      void load();
    };
    void load();
    window.addEventListener('deepseek-scratchpad-changed', refresh);
    const intervalMs = streaming ? 3_000 : 12_000;
    const timer = window.setInterval(refresh, intervalMs);
    return () => {
      cancelled = true;
      window.removeEventListener('deepseek-scratchpad-changed', refresh);
      window.clearInterval(timer);
    };
  }, [threadId, streaming]);

  if (!status || !status.run_id) {
    return null;
  }

  const total = status.areas_total ?? 0;
  const done = status.areas_done ?? 0;
  const deferred = status.areas_deferred ?? 0;
  const inProgress = status.areas_in_progress ?? 0;
  const accounted = done + deferred + inProgress;
  const pct = total > 0 ? Math.round((accounted / total) * 100) : 0;
  const resume = status.resume_area_id;
  const notesTotal = status.notes_total ?? 0;

  return (
    <div className="shrink-0 px-4 pb-2">
      <div
        className="mx-auto w-full max-w-3xl rounded-lg border border-amber-500/30 bg-amber-500/10 px-3 py-2 text-xs text-amber-100/90"
        role="status"
        aria-live="polite"
      >
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <span className="font-medium text-amber-200">审计 scratchpad</span>
          <span>{status.path ?? status.run_id}</span>
          <span>
            进度 {accounted}/{total} ({pct}%)
            {done > 0 || inProgress > 0 || deferred > 0 ? (
              <span className="text-white/50">
                {' '}
                · done {done}
                {inProgress > 0 ? ` · 进行中 ${inProgress}` : ''}
                {deferred > 0 ? ` · deferred ${deferred}` : ''}
              </span>
            ) : null}
          </span>
          {resume ? (
            <span>
              续审区 <code className="text-amber-100">{resume}</code>
            </span>
          ) : accounted >= total && total > 0 ? (
            <span className="text-emerald-300/90">inventory 已完成</span>
          ) : null}
          <span className="text-white/50">
            notes {notesTotal} · verified {status.findings_verified ?? 0}
          </span>
          {notesTotal > 0 && done === 0 && inProgress === 0 ? (
            <span className="text-amber-200/80">待 scratchpad_set_area</span>
          ) : null}
        </div>
      </div>
    </div>
  );
}
