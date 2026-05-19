import { useEffect, useState } from 'react';
import { fetchThreadScratchpadStatus, type ScratchpadStatus } from '../api/client';

interface AuditScratchpadBarProps {
  threadId: string | null;
}

export default function AuditScratchpadBar({ threadId }: AuditScratchpadBarProps) {
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
    void load();
    const timer = window.setInterval(() => {
      void load();
    }, 12_000);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [threadId]);

  if (!status || !status.run_id) {
    return null;
  }

  const total = status.areas_total ?? 0;
  const done = status.areas_done ?? 0;
  const pct = total > 0 ? Math.round((done / total) * 100) : 0;
  const resume = status.resume_area_id;

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
            进度 {done}/{total} ({pct}%)
          </span>
          {resume ? (
            <span>
              续审区 <code className="text-amber-100">{resume}</code>
            </span>
          ) : (
            <span className="text-emerald-300/90">inventory 已完成</span>
          )}
          <span className="text-white/50">
            notes {status.notes_total ?? 0} · verified {status.findings_verified ?? 0}
          </span>
        </div>
      </div>
    </div>
  );
}
