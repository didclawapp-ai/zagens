import { useCallback, useEffect, useState, type FormEvent } from 'react';
import {
  createNightQueueTask,
  fetchGatePresets,
  fetchNightQueue,
  postNightQueueBriefing,
  runNightQueue,
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
import { isActiveNightQueueStatus } from '../types/nightQueue';

function statusClass(status: NightQueueTaskStatus): string {
  switch (status) {
    case 'passed':
      return 'text-emerald-500';
    case 'failed':
    case 'rolled_back':
      return 'text-red-400';
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
  };
  return t(map[status] ?? 'nightQueue.statusPending');
}

function taskGateCount(task: NightQueueTask): number {
  return task.gate?.length ?? 0;
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
  const [presets, setPresets] = useState<GatePreset[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [creating, setCreating] = useState(false);
  const [running, setRunning] = useState(false);
  const [briefing, setBriefing] = useState(false);

  const [prompt, setPrompt] = useState('');
  const [gatePreset, setGatePreset] = useState('');
  const [gateInline, setGateInline] = useState('');
  const [useWorktree, setUseWorktree] = useState(true);

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

  const handleBriefing = async () => {
    setBriefing(true);
    setError(null);
    try {
      const res = await postNightQueueBriefing(true);
      toast.success(t('nightQueue.briefingDone'));
      if (res.handoff_path) {
        toast.info(res.handoff_path);
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      setError(msg);
      toast.error(msg);
    } finally {
      setBriefing(false);
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
          disabled={running || !tasks.some((x) => x.status === 'pending')}
          onClick={() => void handleRun()}
        >
          {running ? t('nightQueue.runningQueue') : t('nightQueue.runQueue')}
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
          className="px-2.5 py-1 text-xs rounded border border-t-border/60 hover:bg-hover ml-auto"
          onClick={() => void reload()}
        >
          {t('nightQueue.refresh')}
        </button>
      </div>

      {lastRunAt ? (
        <p className="px-3 py-1 text-[10px] text-t-text-muted border-b border-t-border/40">
          {t('nightQueue.lastRun', { at: new Date(lastRunAt).toLocaleString() })}
        </p>
      ) : null}

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
              {t('nightQueue.cancel')}
            </button>
          </div>
        </form>
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
        {tasks.map((task) => (
          <div
            key={task.id}
            className="rounded-lg border border-t-border/60 bg-t-surface/40 p-3 space-y-1"
          >
            <div className="flex items-start justify-between gap-2">
              <span className={`text-[10px] font-medium uppercase ${statusClass(task.status)}`}>
                {statusLabel(task.status, t)}
              </span>
              <span className="text-[10px] text-t-text-muted font-mono truncate max-w-[40%]">
                {task.id}
              </span>
            </div>
            <p className="text-xs text-t-text whitespace-pre-wrap break-words">{task.prompt}</p>
            {taskGateCount(task) > 0 ? (
              <p className="text-[10px] text-t-text-muted">
                {t('nightQueue.gateCount', { count: String(taskGateCount(task)) })}
              </p>
            ) : null}
            {task.gate_summary ? (
              <pre className="text-[10px] text-t-text-muted whitespace-pre-wrap font-mono bg-t-bg/50 rounded p-2 max-h-24 overflow-y-auto">
                {task.gate_summary}
              </pre>
            ) : null}
            {task.error ? (
              <p className="text-[10px] text-red-400">{task.error}</p>
            ) : null}
          </div>
        ))}
      </div>
    </div>
  );
}
