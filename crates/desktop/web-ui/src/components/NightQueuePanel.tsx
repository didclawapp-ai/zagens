import { invoke } from '@tauri-apps/api/core';
import { useCallback, useEffect, useMemo, useState, type FormEvent } from 'react';
import {
  cancelNightQueueTask,
  clearNightQueueFinished,
  createNightQueueTask,
  deleteNightQueueTask,
  fetchGatePresets,
  fetchNightQueue,
  postNightQueueBriefing,
  retryNightQueueTask,
  runNightQueue,
  stopNightQueue,
  type RuntimeConnectionState,
} from '../api/client';
import { useT } from '../i18n';
import { isRuntimeApiAvailable } from '../lib/runtimeReachable';
import { toast } from '../lib/toast';
import type {
  GatePreset,
  NightQueueTask,
  NightQueueTaskStatus,
} from '../types/nightQueue';
import {
  formatNightQueueDuration,
  isActiveNightQueueStatus,
  isTerminalNightQueueStatus,
  shortNightQueueId,
} from '../types/nightQueue';

function statusClass(status: NightQueueTaskStatus): string {
  switch (status) {
    case 'passed':
      return 'text-emerald-500';
    case 'failed':
    case 'rolled_back':
      return 'text-red-400';
    case 'canceled':
      return 'text-amber-500';
    case 'running':
      return 'text-accent';
    default:
      return 'text-t-text-muted';
  }
}

function statusLabel(
  status: NightQueueTaskStatus,
  t: (key: import('../i18n/keys').TranslationKey) => string,
): string {
  const map: Record<NightQueueTaskStatus, import('../i18n/keys').TranslationKey> = {
    pending: 'nightQueue.statusPending',
    running: 'nightQueue.statusRunning',
    passed: 'nightQueue.statusPassed',
    failed: 'nightQueue.statusFailed',
    rolled_back: 'nightQueue.statusRolledBack',
    canceled: 'nightQueue.statusCanceled',
  };
  return t(map[status] ?? 'nightQueue.statusPending');
}

function taskGateCount(task: NightQueueTask): number {
  return task.gate?.length ?? 0;
}

function isPlaceholderWorktree(path?: string | null): boolean {
  return !path || path === '<allocate-on-run>';
}

async function openAbsPath(path: string): Promise<void> {
  await invoke('open_with_system_app', { path });
}

export default function NightQueuePanel({
  runtimeConn,
  streaming = false,
  runtimeSessionEstablished = false,
}: {
  runtimeConn: RuntimeConnectionState;
  streaming?: boolean;
  runtimeSessionEstablished?: boolean;
}) {
  const { t } = useT();
  const runtimeReady = isRuntimeApiAvailable(runtimeConn, {
    streaming,
    sessionEstablished: runtimeSessionEstablished,
  });

  const [tasks, setTasks] = useState<NightQueueTask[]>([]);
  const [lastRunAt, setLastRunAt] = useState<string | null>(null);
  const [queuePath, setQueuePath] = useState<string | null>(null);
  const [presets, setPresets] = useState<GatePreset[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [creating, setCreating] = useState(false);
  const [running, setRunning] = useState(false);
  const [stopping, setStopping] = useState(false);
  const [briefing, setBriefing] = useState(false);
  const [briefingMd, setBriefingMd] = useState<string | null>(null);
  const [handoffPath, setHandoffPath] = useState<string | null>(null);
  const [showBriefing, setShowBriefing] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());

  const [prompt, setPrompt] = useState('');
  const [gatePreset, setGatePreset] = useState('');
  const [gateInline, setGateInline] = useState('');
  const [useWorktree, setUseWorktree] = useState(true);

  const counts = useMemo(() => {
    const c = {
      pending: 0,
      running: 0,
      passed: 0,
      failed: 0,
      rolled_back: 0,
      canceled: 0,
    };
    for (const task of tasks) {
      c[task.status] += 1;
    }
    return c;
  }, [tasks]);

  const hasRunning = counts.running > 0;
  const hasPending = counts.pending > 0;
  const hasFinished =
    counts.passed + counts.failed + counts.rolled_back + counts.canceled > 0;

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [queue, presetRes] = await Promise.all([
        fetchNightQueue(),
        fetchGatePresets(),
      ]);
      setTasks(queue.tasks);
      setLastRunAt(queue.last_run_at ?? null);
      setQueuePath(queue.queue_path ?? null);
      setPresets(presetRes.presets);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (runtimeReady) {
      void reload();
    }
  }, [runtimeReady, reload]);

  useEffect(() => {
    if (!runtimeReady) return;
    const hasActive = tasks.some((task) => isActiveNightQueueStatus(task.status));
    if (!hasActive) return;
    const id = window.setInterval(() => {
      void reload();
    }, 4000);
    return () => window.clearInterval(id);
  }, [runtimeReady, tasks, reload]);

  useEffect(() => {
    if (!hasRunning) return;
    const id = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [hasRunning]);

  const handleCreate = async (e: FormEvent) => {
    e.preventDefault();
    const trimmed = prompt.trim();
    if (!trimmed) return;
    setCreating(true);
    setError(null);
    try {
      const gate = gateInline.trim() ? [gateInline.trim()] : undefined;
      await createNightQueueTask({
        prompt: trimmed,
        gate,
        gate_preset: gatePreset || undefined,
        use_worktree: useWorktree,
      });
      setPrompt('');
      setGateInline('');
      setShowCreate(false);
      toast.success(t('nightQueue.enqueued'));
      await reload();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      toast.error(msg);
    } finally {
      setCreating(false);
    }
  };

  const handleRun = async () => {
    setRunning(true);
    setError(null);
    try {
      const report = await runNightQueue({
        use_worktree: useWorktree,
        write_briefing: true,
      });
      toast.success(
        t('nightQueue.runDone', {
          ran: String(report.ran),
          passed: String(report.passed),
          failed: String(report.failed),
          canceled: String(report.canceled ?? 0),
        }),
      );
      await reload();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      toast.error(msg);
    } finally {
      setRunning(false);
    }
  };

  const handleStop = async () => {
    setStopping(true);
    setError(null);
    try {
      const res = await stopNightQueue();
      if (res.stopped) {
        toast.info(t('nightQueue.stopRequested'));
      } else if ((res.reclaimed ?? 0) > 0) {
        toast.success(
          t('nightQueue.reclaimed', { count: String(res.reclaimed ?? 0) }),
        );
      } else {
        toast.info(t('nightQueue.stopIdle'));
      }
      await reload();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      toast.error(msg);
    } finally {
      setStopping(false);
    }
  };

  const handleBriefing = async () => {
    setBriefing(true);
    setError(null);
    try {
      const res = await postNightQueueBriefing(true);
      setBriefingMd(res.markdown);
      setHandoffPath(res.handoff_path ?? null);
      setShowBriefing(true);
      toast.success(t('nightQueue.briefingDone'));
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      toast.error(msg);
    } finally {
      setBriefing(false);
    }
  };

  const handleClearFinished = async () => {
    setError(null);
    try {
      const res = await clearNightQueueFinished();
      toast.success(t('nightQueue.cleared', { count: String(res.removed) }));
      await reload();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      toast.error(msg);
    }
  };

  const withTaskBusy = async (taskId: string, fn: () => Promise<void>) => {
    setBusyId(taskId);
    setError(null);
    try {
      await fn();
      await reload();
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      // Server may have already mutated the queue (e.g. DELETE 204 client parse glitch).
      if (/queue task not found/i.test(msg)) {
        await reload();
        toast.info(t('nightQueue.alreadyGone'));
      } else {
        setError(msg);
        toast.error(msg);
      }
    } finally {
      setBusyId(null);
    }
  };

  if (!runtimeReady) {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center space-y-2">
        <p>{t('nightQueue.waitingRuntime')}</p>
        <p className="text-[10px]">{t('nightQueue.waitingDetail')}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full min-h-0">
      <div className="flex flex-wrap items-center gap-2 px-3 py-2 border-b border-t-border/60 shrink-0">
        <button
          type="button"
          className="px-2.5 py-1 text-xs rounded bg-accent text-white hover:opacity-90 disabled:opacity-50"
          onClick={() => setShowCreate((v) => !v)}
        >
          {t('nightQueue.enqueue')}
        </button>
        <button
          type="button"
          className="px-2.5 py-1 text-xs rounded border border-t-border/60 hover:bg-hover disabled:opacity-50"
          disabled={running || !hasPending || hasRunning}
          onClick={() => void handleRun()}
        >
          {running ? t('nightQueue.runningQueue') : t('nightQueue.runQueue')}
        </button>
        <button
          type="button"
          className="px-2.5 py-1 text-xs rounded border border-red-400/50 text-red-400 hover:bg-red-500/10 disabled:opacity-50"
          disabled={stopping || (!hasRunning && !running)}
          onClick={() => void handleStop()}
        >
          {stopping ? t('nightQueue.stopping') : t('nightQueue.stop')}
        </button>
        <button
          type="button"
          className="px-2.5 py-1 text-xs rounded border border-t-border/60 hover:bg-hover disabled:opacity-50"
          disabled={briefing || tasks.length === 0}
          onClick={() => void handleBriefing()}
        >
          {briefing ? t('nightQueue.briefingLoading') : t('nightQueue.briefing')}
        </button>
        <button
          type="button"
          className="px-2.5 py-1 text-xs rounded border border-t-border/60 hover:bg-hover disabled:opacity-50"
          disabled={!hasFinished}
          onClick={() => void handleClearFinished()}
        >
          {t('nightQueue.clearFinished')}
        </button>
        <button
          type="button"
          className="px-2.5 py-1 text-xs rounded border border-t-border/60 hover:bg-hover ml-auto"
          onClick={() => void reload()}
        >
          {t('nightQueue.refresh')}
        </button>
      </div>

      <div className="px-3 py-1.5 text-[10px] text-t-text-muted border-b border-t-border/40 space-y-0.5 shrink-0">
        <p>
          {t('nightQueue.summary', {
            pending: String(counts.pending),
            running: String(counts.running),
            passed: String(counts.passed),
            failed: String(counts.failed + counts.rolled_back),
            canceled: String(counts.canceled),
          })}
        </p>
        {lastRunAt ? (
          <p>{t('nightQueue.lastRun', { at: new Date(lastRunAt).toLocaleString() })}</p>
        ) : null}
        {queuePath ? (
          <p className="font-mono truncate" title={queuePath}>
            {queuePath}
          </p>
        ) : null}
        <p className="text-t-text-muted/80">{t('nightQueue.trustHint')}</p>
      </div>

      {showCreate ? (
        <form
          className="px-3 py-3 border-b border-t-border/60 space-y-2 shrink-0 bg-t-surface/30"
          onSubmit={(e) => void handleCreate(e)}
        >
          <label className="block text-[10px] text-t-text-muted">{t('nightQueue.promptLabel')}</label>
          <textarea
            className="w-full min-h-[72px] text-xs rounded border border-t-border/60 bg-t-bg px-2 py-1.5 resize-y"
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            placeholder={t('nightQueue.promptPlaceholder')}
            required
          />
          <label className="block text-[10px] text-t-text-muted">{t('nightQueue.gatePresetLabel')}</label>
          <select
            className="w-full text-xs rounded border border-t-border/60 bg-t-bg px-2 py-1.5"
            value={gatePreset}
            onChange={(e) => setGatePreset(e.target.value)}
          >
            <option value="">{t('nightQueue.gatePresetNone')}</option>
            {presets.map((p) => (
              <option key={p.id} value={p.id}>
                {p.id} — {p.description}
              </option>
            ))}
          </select>
          <label className="block text-[10px] text-t-text-muted">{t('nightQueue.gateInlineLabel')}</label>
          <input
            type="text"
            className="w-full text-xs rounded border border-t-border/60 bg-t-bg px-2 py-1.5 font-mono"
            value={gateInline}
            onChange={(e) => setGateInline(e.target.value)}
            placeholder="file_exists:path=deliverables/out.txt"
            disabled={Boolean(gatePreset)}
          />
          <label className="flex items-center gap-2 text-xs text-t-text-muted cursor-pointer">
            <input
              type="checkbox"
              checked={useWorktree}
              onChange={(e) => setUseWorktree(e.target.checked)}
            />
            {t('nightQueue.useWorktree')}
          </label>
          <div className="flex gap-2">
            <button
              type="submit"
              disabled={creating || !prompt.trim()}
              className="px-3 py-1 text-xs rounded bg-accent text-white disabled:opacity-50"
            >
              {creating ? t('nightQueue.enqueueing') : t('nightQueue.submitEnqueue')}
            </button>
            <button
              type="button"
              className="px-3 py-1 text-xs rounded border border-t-border/60"
              onClick={() => setShowCreate(false)}
            >
              {t('nightQueue.cancelForm')}
            </button>
          </div>
        </form>
      ) : null}

      {showBriefing && briefingMd ? (
        <div className="px-3 py-2 border-b border-t-border/60 shrink-0 space-y-1.5 bg-t-surface/20">
          <div className="flex items-center gap-2">
            <p className="text-[10px] font-medium text-t-text">{t('nightQueue.briefingPreview')}</p>
            {handoffPath ? (
              <button
                type="button"
                className="text-[10px] text-accent hover:underline"
                onClick={() => void openAbsPath(handoffPath).catch(() => toast.error(handoffPath))}
              >
                {t('nightQueue.openHandoff')}
              </button>
            ) : null}
            <button
              type="button"
              className="ml-auto text-[10px] text-t-text-muted hover:text-t-text"
              onClick={() => setShowBriefing(false)}
            >
              {t('nightQueue.hideBriefing')}
            </button>
          </div>
          <pre className="text-[10px] text-t-text-muted whitespace-pre-wrap font-mono bg-t-bg/50 rounded p-2 max-h-40 overflow-y-auto">
            {briefingMd}
          </pre>
        </div>
      ) : null}

      {error ? (
        <p className="px-3 py-2 text-xs text-red-400 border-b border-t-border/40">{error}</p>
      ) : null}

      <div className="flex-1 overflow-y-auto px-3 py-2 space-y-2 min-h-0">
        {loading && tasks.length === 0 ? (
          <p className="text-xs text-t-text-muted text-center py-6">{t('nightQueue.loading')}</p>
        ) : null}
        {!loading && tasks.length === 0 ? (
          <p className="text-xs text-t-text-muted text-center py-6">{t('nightQueue.empty')}</p>
        ) : null}
        {tasks.map((task) => {
          const duration = formatNightQueueDuration(
            task.started_at,
            task.finished_at,
            nowMs,
            task.status !== 'running',
          );
          const busy = busyId === task.id;
          const worktree =
            !isPlaceholderWorktree(task.worktree_path) ? task.worktree_path : null;

          return (
            <div
              key={task.id}
              className="rounded-lg border border-t-border/60 bg-t-surface/40 p-3 space-y-1.5"
            >
              <div className="flex items-start justify-between gap-2">
                <span className={`text-[10px] font-medium uppercase ${statusClass(task.status)}`}>
                  {statusLabel(task.status, t)}
                </span>
                <span
                  className="text-[10px] text-t-text-muted font-mono truncate max-w-[45%]"
                  title={task.id}
                >
                  {shortNightQueueId(task.id)}
                </span>
              </div>
              <p className="text-xs text-t-text whitespace-pre-wrap break-words">{task.prompt}</p>
              <div className="flex flex-wrap gap-x-3 gap-y-0.5 text-[10px] text-t-text-muted">
                {duration ? <span>{t('nightQueue.duration', { duration })}</span> : null}
                {taskGateCount(task) > 0 ? (
                  <span>{t('nightQueue.gateCount', { count: String(taskGateCount(task)) })}</span>
                ) : null}
                {task.created_at ? (
                  <span title={task.created_at}>
                    {t('nightQueue.createdAt', {
                      at: new Date(task.created_at).toLocaleString(),
                    })}
                  </span>
                ) : null}
              </div>
              {worktree ? (
                <button
                  type="button"
                  className="block text-[10px] text-accent hover:underline font-mono truncate max-w-full text-left"
                  title={worktree}
                  onClick={() => void openAbsPath(worktree).catch(() => toast.error(worktree))}
                >
                  {t('nightQueue.openWorktree')}
                </button>
              ) : null}
              {task.gate_summary ? (
                <pre className="text-[10px] text-t-text-muted whitespace-pre-wrap font-mono bg-t-bg/50 rounded p-2 max-h-24 overflow-y-auto">
                  {task.gate_summary}
                </pre>
              ) : null}
              {task.error ? (
                <p className="text-[10px] text-red-400">{task.error}</p>
              ) : null}
              <div className="flex flex-wrap gap-1.5 pt-0.5">
                {task.status === 'pending' ? (
                  <button
                    type="button"
                    className="px-2 py-0.5 text-[10px] rounded border border-t-border/60 hover:bg-hover disabled:opacity-50"
                    disabled={busy}
                    onClick={() =>
                      void withTaskBusy(task.id, async () => {
                        await cancelNightQueueTask(task.id);
                        toast.success(t('nightQueue.taskCanceled'));
                      })
                    }
                  >
                    {t('nightQueue.cancelTask')}
                  </button>
                ) : null}
                {task.status === 'running' ? (
                  <button
                    type="button"
                    className="px-2 py-0.5 text-[10px] rounded border border-red-400/50 text-red-400 hover:bg-red-500/10 disabled:opacity-50"
                    disabled={busy || stopping}
                    onClick={() =>
                      void withTaskBusy(task.id, async () => {
                        await cancelNightQueueTask(task.id);
                        toast.info(t('nightQueue.stopRequested'));
                      })
                    }
                  >
                    {t('nightQueue.stopTask')}
                  </button>
                ) : null}
                {isTerminalNightQueueStatus(task.status) ? (
                  <>
                    <button
                      type="button"
                      className="px-2 py-0.5 text-[10px] rounded border border-t-border/60 hover:bg-hover disabled:opacity-50"
                      disabled={busy}
                      onClick={() =>
                        void withTaskBusy(task.id, async () => {
                          await retryNightQueueTask(task.id);
                          toast.success(t('nightQueue.retried'));
                        })
                      }
                    >
                      {t('nightQueue.retry')}
                    </button>
                    <button
                      type="button"
                      className="px-2 py-0.5 text-[10px] rounded border border-t-border/60 hover:bg-hover disabled:opacity-50"
                      disabled={busy}
                      onClick={() =>
                        void withTaskBusy(task.id, async () => {
                          await deleteNightQueueTask(task.id);
                          toast.success(t('nightQueue.removed'));
                        })
                      }
                    >
                      {t('nightQueue.remove')}
                    </button>
                  </>
                ) : null}
                {task.status === 'pending' ? (
                  <button
                    type="button"
                    className="px-2 py-0.5 text-[10px] rounded border border-t-border/60 hover:bg-hover disabled:opacity-50"
                    disabled={busy}
                    onClick={() =>
                      void withTaskBusy(task.id, async () => {
                        await deleteNightQueueTask(task.id);
                        toast.success(t('nightQueue.removed'));
                      })
                    }
                  >
                    {t('nightQueue.remove')}
                  </button>
                ) : null}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
