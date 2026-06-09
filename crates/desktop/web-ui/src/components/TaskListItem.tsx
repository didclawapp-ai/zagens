import { useCallback, useEffect, useRef, useState } from 'react';
import { fetchTask } from '../api/client';
import { useT } from '../i18n';
import type { TaskRecord, TaskSummary } from '../types/automation';

const TASK_STATUS_COLOR: Record<string, string> = {
  queued: 'text-t-text-muted',
  pending: 'text-t-text-muted',
  running: 'text-amber-text',
  paused: 'text-t-text-muted',
  completed: 'text-success',
  failed: 'text-t-error',
  canceled: 'text-t-text-muted',
};

function taskStatusLabel(
  t: ReturnType<typeof useT>['t'],
  status: string,
): string {
  const key = `automation.${status}` as 'automation.queued';
  if (
    status === 'queued' ||
    status === 'pending' ||
    status === 'running' ||
    status === 'paused' ||
    status === 'completed' ||
    status === 'failed' ||
    status === 'canceled'
  ) {
    return t(key);
  }
  return status;
}

function canCancelTask(status: string): boolean {
  return status === 'queued' || status === 'running' || status === 'pending' || status === 'paused';
}

function isActiveTask(status: string): boolean {
  return status === 'queued' || status === 'running';
}

type TaskListItemProps = {
  task: TaskSummary;
  highlighted: boolean;
  expanded: boolean;
  onToggleExpand: () => void;
  onCancel: (id: string) => void;
  cancelingId: string | null;
  onOpenTaskThread?: (threadId: string) => void;
};

export default function TaskListItem({
  task,
  highlighted,
  expanded,
  onToggleExpand,
  onCancel,
  cancelingId,
  onOpenTaskThread,
}: TaskListItemProps) {
  const { t } = useT();
  const highlightRef = useRef<HTMLDivElement | null>(null);
  const [detail, setDetail] = useState<TaskRecord | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);

  useEffect(() => {
    if (!highlighted || !highlightRef.current) return;
    highlightRef.current.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }, [highlighted]);

  const loadDetail = useCallback(async () => {
    setDetailLoading(true);
    setDetailError(null);
    try {
      const record = await fetchTask(task.id);
      setDetail(record);
    } catch (e) {
      setDetailError(e instanceof Error ? e.message : String(e));
    } finally {
      setDetailLoading(false);
    }
  }, [task.id]);

  useEffect(() => {
    if (!expanded) {
      return;
    }
    void loadDetail();
  }, [expanded, loadDetail]);

  useEffect(() => {
    if (!expanded || !isActiveTask(task.status)) {
      return;
    }
    const timer = window.setInterval(() => {
      void loadDetail();
    }, 3000);
    return () => window.clearInterval(timer);
  }, [expanded, task.status, loadDetail]);

  const showDetail = detail ?? null;

  return (
    <div
      ref={highlighted ? highlightRef : undefined}
      className={`rounded-lg border bg-canvas-alt p-3 transition-colors ${
        highlighted ? 'border-accent ring-2 ring-accent/30' : 'border-card-border'
      }`}
    >
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={onToggleExpand}
          className="shrink-0 text-[10px] text-t-text-muted hover:text-t-text w-4"
          aria-expanded={expanded}
          aria-label={expanded ? t('automation.hideDetails') : t('automation.viewDetails')}
        >
          {expanded ? '▾' : '▸'}
        </button>
        <span className="font-mono text-[10px] text-t-text-muted shrink-0">{task.id.slice(0, 10)}</span>
        <button
          type="button"
          onClick={onToggleExpand}
          className="text-xs text-t-text truncate flex-1 min-w-0 text-left hover:underline"
        >
          {task.prompt_summary}
        </button>
        <span className={`text-[10px] font-medium shrink-0 ${TASK_STATUS_COLOR[task.status] ?? 'text-t-text-muted'}`}>
          {taskStatusLabel(t, task.status)}
        </span>
        {canCancelTask(task.status) && (
          <button
            type="button"
            onClick={() => onCancel(task.id)}
            disabled={cancelingId === task.id}
            className="shrink-0 text-[10px] text-t-error hover:underline disabled:opacity-50"
          >
            {cancelingId === task.id ? t('automation.canceling') : t('automation.cancel')}
          </button>
        )}
      </div>
      <div className="mt-1 text-[10px] text-t-text-muted pl-6">
        {task.model} · {task.mode}
        {task.duration_ms != null && ` · ${(task.duration_ms / 1000).toFixed(1)}s`}
      </div>
      {!expanded && task.error && task.status === 'failed' && (
        <p className="mt-1.5 pl-6 text-[10px] text-t-error line-clamp-2" title={task.error}>
          {task.error}
        </p>
      )}
      {expanded && (
        <div className="mt-2 pl-6 space-y-2 border-t border-card-border/50 pt-2">
          {detailLoading && !showDetail && (
            <p className="text-[10px] text-t-text-muted">{t('automation.loadingDetail')}</p>
          )}
          {detailError && (
            <p className="text-[10px] text-t-error">{detailError}</p>
          )}
          {showDetail && (
            <>
              <div>
                <div className="text-[10px] font-medium text-t-text">{t('automation.taskPromptLabel')}</div>
                <pre className="mt-0.5 text-[10px] text-t-text whitespace-pre-wrap break-words font-sans">
                  {showDetail.prompt}
                </pre>
              </div>
              {showDetail.error && (
                <div>
                  <div className="text-[10px] font-medium text-t-error">{t('automation.taskError')}</div>
                  <pre className="mt-0.5 text-[10px] text-t-error whitespace-pre-wrap break-words font-sans">
                    {showDetail.error}
                  </pre>
                </div>
              )}
              <div>
                <div className="text-[10px] font-medium text-t-text">{t('automation.taskResult')}</div>
                {showDetail.result_summary ? (
                  <pre className="mt-0.5 text-[10px] text-t-text whitespace-pre-wrap break-words font-sans">
                    {showDetail.result_summary}
                  </pre>
                ) : isActiveTask(showDetail.status) ? (
                  <p className="mt-0.5 text-[10px] text-t-text-muted">{t('automation.noResultYet')}</p>
                ) : (
                  <p className="mt-0.5 text-[10px] text-t-text-muted">{t('automation.noTextResult')}</p>
                )}
                {showDetail.result_detail_path && (
                  <p className="mt-1 text-[10px] text-t-text-muted">{t('automation.resultTruncated')}</p>
                )}
              </div>
              {(showDetail.timeline?.length ?? 0) > 0 && (
                <div>
                  <div className="text-[10px] font-medium text-t-text">{t('automation.taskTimeline')}</div>
                  <ul className="mt-0.5 space-y-0.5">
                    {showDetail.timeline!.map((entry, i) => (
                      <li key={`${entry.timestamp}-${entry.kind}-${i}`} className="text-[10px] text-t-text-muted">
                        <span className="text-t-text">{entry.summary}</span>
                      </li>
                    ))}
                  </ul>
                </div>
              )}
              {(showDetail.tool_calls?.length ?? 0) > 0 && (
                <div>
                  <div className="text-[10px] font-medium text-t-text">{t('automation.taskTools')}</div>
                  <ul className="mt-0.5 space-y-1">
                    {showDetail.tool_calls!.map((tool) => (
                      <li key={tool.id} className="text-[10px] text-t-text-muted">
                        <span className="font-mono text-t-text">{tool.name}</span>
                        {' · '}
                        {tool.status}
                        {tool.output_summary ? (
                          <span className="block text-t-text mt-0.5 whitespace-pre-wrap break-words">
                            {tool.output_summary}
                          </span>
                        ) : null}
                      </li>
                    ))}
                  </ul>
                </div>
              )}
              {showDetail.thread_id && onOpenTaskThread && (
                <button
                  type="button"
                  onClick={() => onOpenTaskThread(showDetail.thread_id!)}
                  className="text-[10px] text-accent hover:underline"
                >
                  {t('automation.openInChat')}
                </button>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}
