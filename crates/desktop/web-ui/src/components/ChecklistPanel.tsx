import { useEffect, useState, useCallback, useRef } from 'react';
import { fetchThreadChecklist } from '../api/client';

interface ChecklistItem {
  id: number;
  content: string;
  status: 'pending' | 'in_progress' | 'completed';
}

interface ChecklistData {
  items: ChecklistItem[];
  completion_pct: number;
  in_progress_id: number | null;
}

interface Props {
  threadId: string;
  /** Fired once when checklist data first arrives (auto-switches parent to this panel). */
  onDetected?: () => void;
}

export default function ChecklistPanel({ threadId, onDetected }: Props) {
  const [checklist, setChecklist] = useState<ChecklistData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const firedRef = useRef(false);

  const fetchChecklist = useCallback(async () => {
    if (!threadId) return;
    try {
      const data = await fetchThreadChecklist(threadId);
      setChecklist(data);
      setError(null);
      if (data && data.items && data.items.length > 0 && !firedRef.current) {
        firedRef.current = true;
        onDetected?.();
      }
    } catch {
      setError(null); // silent — poll will retry
    }
  }, [threadId, onDetected]);

  useEffect(() => {
    fetchChecklist();
    const interval = setInterval(fetchChecklist, 2000);
    return () => clearInterval(interval);
  }, [fetchChecklist]);

  if (error) {
    return <div className="p-4 text-sm text-t-text-muted">{error}</div>;
  }

  if (!checklist || checklist.items.length === 0) {
    return (
      <div className="p-4 text-sm text-t-text-muted">
        {'No checklist yet — the model has not called checklist_write'}
      </div>
    );
  }

  const statusIcon = (status: string) => {
    switch (status) {
      case 'completed':
        return (
          <svg className="w-4 h-4 shrink-0 text-green-500" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
            <path d="M5 13l4 4L19 7" />
          </svg>
        );
      case 'in_progress':
        return (
          <svg className="w-4 h-4 shrink-0 text-amber-500 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
            <circle cx="12" cy="12" r="10" strokeDasharray="32" />
          </svg>
        );
      default:
        return (
          <svg className="w-4 h-4 shrink-0 text-t-text-muted" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6">
            <circle cx="12" cy="12" r="10" />
          </svg>
        );
    }
  };

  return (
    <div className="p-4 overflow-y-auto text-sm">
      <div className="mb-3 flex items-center gap-2">
        <div className="flex-1 h-1.5 rounded-full bg-t-bg-hover overflow-hidden">
          <div
            className="h-full rounded-full bg-green-500 transition-all duration-300"
            style={{ width: `${checklist.completion_pct}%` }}
          />
        </div>
        <span className="text-xs text-t-text-muted tabular-nums">
          {checklist.completion_pct}%
        </span>
      </div>
      <ul className="space-y-1.5">
        {checklist.items.map((item) => (
          <li
            key={item.id}
            className={`flex items-start gap-2 py-1 px-1.5 rounded ${
              item.status === 'in_progress'
                ? 'bg-amber-500/10 border-l-2 border-amber-500'
                : ''
            }`}
          >
            <span className="mt-0.5">{statusIcon(item.status)}</span>
            <span
              className={`leading-snug ${
                item.status === 'completed'
                  ? 'text-t-text-muted line-through'
                  : 'text-t-text'
              }`}
            >
              {item.content}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}