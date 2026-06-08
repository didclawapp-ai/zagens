import { useCallback, useEffect, useState, type FormEvent } from 'react';
import {
  createAutomation,
  deleteAutomation,
  fetchAutomationRuns,
  fetchAutomations,
  pauseAutomation,
  resumeAutomation,
  runAutomation,
  updateAutomation,
  type RuntimeConnectionState,
} from '../api/client';
import type {
  AutomationRecord,
  AutomationRunRecord,
  AutomationTriggerKind,
  CreateAutomationRequest,
  UpdateAutomationRequest,
} from '../types/automation';
import { useT } from '../i18n';
import { isRuntimeApiAvailable } from '../lib/runtimeReachable';
import { confirmDialog } from '../lib/confirmDialog';
import {
  WEEKDAYS,
  WORKDAYS,
  buildRrule,
  defaultScheduleFormValues,
  describeRrule,
  formValuesFromAutomation,
  type ScheduleFormValues,
  type ScheduleKind,
} from '../lib/scheduleRrule';

const MODE_OPTIONS: Array<{ value: string; label: string }> = [
  { value: 'agent', label: 'Agent' },
  { value: 'plan', label: 'Plan' },
  { value: 'yolo', label: 'YOLO' },
];

const RUN_STATUS_COLOR: Record<string, string> = {
  queued: 'text-t-text-muted',
  running: 'text-amber-text',
  completed: 'text-success',
  failed: 'text-t-error',
  canceled: 'text-t-text-muted',
};

function formatDateTime(iso: string | null): string {
  if (!iso) return '—';
  try {
    return new Date(iso).toLocaleString();
  } catch {
    return iso;
  }
}

function triggerKindOf(item: AutomationRecord): AutomationTriggerKind {
  return item.trigger_kind === 'task' ? 'task' : 'prompt';
}

interface Props {
  runtimeConn: RuntimeConnectionState;
  streaming?: boolean;
  runtimeSessionEstablished?: boolean;
  onOpenTasks?: (taskId?: string) => void;
}

export default function ScheduledAutomationsPanel({
  runtimeConn,
  streaming = false,
  runtimeSessionEstablished = false,
  onOpenTasks,
}: Props) {
  const { t } = useT();
  const runtimeReady = isRuntimeApiAvailable(runtimeConn, {
    streaming,
    sessionEstablished: runtimeSessionEstablished,
  });
  const [items, setItems] = useState<AutomationRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [formMode, setFormMode] = useState<'closed' | 'create' | 'edit' | 'duplicate'>('closed');
  const [editingItem, setEditingItem] = useState<AutomationRecord | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [expandedHistoryId, setExpandedHistoryId] = useState<string | null>(null);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await fetchAutomations();
      setItems(list);
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
    const timer = window.setInterval(() => void reload(), 30_000);
    return () => window.clearInterval(timer);
  }, [runtimeReady, reload]);

  const closeForm = () => {
    setFormMode('closed');
    setEditingItem(null);
  };

  const openCreate = () => {
    setEditingItem(null);
    setFormMode('create');
  };

  const openEdit = (item: AutomationRecord) => {
    setEditingItem(item);
    setFormMode('edit');
  };

  const openDuplicate = (item: AutomationRecord) => {
    setEditingItem(item);
    setFormMode('duplicate');
  };

  const handleDelete = async (id: string) => {
    const ok = await confirmDialog(t('schedule.deleteConfirm'), t('schedule.deleteTitle'));
    if (!ok) return;
    setBusyId(id);
    try {
      await deleteAutomation(id);
      if (expandedHistoryId === id) setExpandedHistoryId(null);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyId(null);
    }
  };

  const handleToggle = async (item: AutomationRecord) => {
    setBusyId(item.id);
    try {
      if (item.status === 'active') {
        await pauseAutomation(item.id);
      } else {
        await resumeAutomation(item.id);
      }
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyId(null);
    }
  };

  const handleRunNow = async (id: string) => {
    setBusyId(id);
    try {
      await runAutomation(id);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusyId(null);
    }
  };

  if (!runtimeReady) {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center space-y-2">
        <p>{t('schedule.waitingRuntime')}</p>
        <p className="text-[10px]">{t('schedule.waitingDetail')}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-divider shrink-0 flex-wrap">
        <p className="text-[10px] text-t-text-muted flex-1 min-w-[12rem]">{t('schedule.intro')}</p>
        <button
          type="button"
          onClick={() => (formMode === 'closed' ? openCreate() : closeForm())}
          className="px-2.5 py-1 text-[11px] font-medium rounded-md border border-card-border bg-canvas-alt hover:bg-hover text-t-text"
        >
          {formMode === 'closed' ? t('schedule.newAutomation') : t('schedule.close')}
        </button>
        <button
          type="button"
          onClick={() => void reload()}
          disabled={loading}
          className="px-2.5 py-1 text-[11px] font-medium rounded-md border border-card-border bg-canvas-alt hover:bg-hover text-t-text disabled:opacity-50"
        >
          {t('schedule.refresh')}
        </button>
      </div>

      <div className="overflow-y-auto px-3 py-2 flex-1 min-h-0">
        {formMode !== 'closed' && (
          <AutomationForm
            mode={formMode}
            initialItem={editingItem}
            onCancel={closeForm}
            onSaved={async () => {
              closeForm();
              await reload();
            }}
          />
        )}

        {error && items.length === 0 && (
          <div className="p-4 space-y-2">
            <p className="text-xs text-t-error">{t('schedule.loadFailed', { error })}</p>
            <button type="button" onClick={() => void reload()} className="text-xs text-accent hover:underline">
              {t('schedule.retry')}
            </button>
          </div>
        )}

        {loading && items.length === 0 ? (
          <div className="p-4 text-xs text-t-text-muted text-center">{t('schedule.loading')}</div>
        ) : items.length === 0 ? (
          <p className="text-xs text-t-text-muted text-center py-6">{t('schedule.noAutomations')}</p>
        ) : (
          <div className="space-y-2">
            {items.map((item) => (
              <AutomationCard
                key={item.id}
                item={item}
                busy={busyId === item.id}
                historyOpen={expandedHistoryId === item.id}
                onToggleHistory={() =>
                  setExpandedHistoryId((prev) => (prev === item.id ? null : item.id))
                }
                onRunNow={() => void handleRunNow(item.id)}
                onToggle={() => void handleToggle(item)}
                onEdit={() => openEdit(item)}
                onDuplicate={() => openDuplicate(item)}
                onDelete={() => void handleDelete(item.id)}
                onOpenTask={(taskId) => onOpenTasks?.(taskId)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function AutomationCard({
  item,
  busy,
  historyOpen,
  onToggleHistory,
  onRunNow,
  onToggle,
  onEdit,
  onDuplicate,
  onDelete,
  onOpenTask,
}: {
  item: AutomationRecord;
  busy: boolean;
  historyOpen: boolean;
  onToggleHistory: () => void;
  onRunNow: () => void;
  onToggle: () => void;
  onEdit: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onOpenTask: (taskId: string) => void;
}) {
  const { t } = useT();
  const kind = triggerKindOf(item);
  const workspace = item.cwds?.[0] ?? '';

  return (
    <div className="rounded-lg border border-card-border bg-canvas-alt p-3">
      <div className="flex items-start gap-2">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span className="text-xs font-semibold text-t-text truncate">{item.name}</span>
            <span
              className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium ${
                kind === 'task'
                  ? 'bg-accent-soft text-accent'
                  : 'bg-canvas text-t-text-muted border border-card-border'
              }`}
            >
              {kind === 'task' ? t('schedule.triggerTask') : t('schedule.triggerPrompt')}
            </span>
          </div>
          <div className="mt-0.5 text-[10px] text-t-text-muted line-clamp-2">{item.prompt}</div>
          {kind === 'task' && (
            <div className="mt-1 text-[10px] text-t-text-muted font-mono">
              {[item.mode ?? 'agent', item.model ?? t('schedule.defaultModel')]
                .filter(Boolean)
                .join(' · ')}
              {item.allow_shell ? ' · shell' : ''}
              {item.trust_mode ? ' · trust' : ''}
              {item.auto_approve === false ? ' · manual-approve' : ''}
              {workspace ? ` · ${workspace}` : ''}
            </div>
          )}
          <div className="mt-1.5 text-[10px] text-t-text-muted space-y-0.5">
            <div>
              {t('schedule.rrule')}:{' '}
              <span className="font-mono text-t-text">{describeRrule(item.rrule, t)}</span>
            </div>
            <div>
              {t('schedule.nextRun')}: {formatDateTime(item.next_run_at)}
            </div>
            <div>
              {t('schedule.lastRun')}: {formatDateTime(item.last_run_at)}
            </div>
          </div>
        </div>
        <span
          className={`shrink-0 text-[10px] font-medium ${
            item.status === 'active' ? 'text-success' : 'text-t-text-muted'
          }`}
        >
          {item.status === 'active' ? t('schedule.statusActive') : t('schedule.statusPaused')}
        </span>
      </div>

      {historyOpen && (
        <RunHistorySection automationId={item.id} onOpenTask={onOpenTask} />
      )}

      <div className="mt-2 flex flex-wrap gap-2">
        <button
          type="button"
          disabled={busy}
          onClick={onRunNow}
          className="text-[10px] text-accent hover:underline disabled:opacity-50"
        >
          {t('schedule.runNow')}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={onToggle}
          className="text-[10px] text-t-text hover:underline disabled:opacity-50"
        >
          {item.status === 'active' ? t('schedule.pause') : t('schedule.resume')}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={onEdit}
          className="text-[10px] text-t-text hover:underline disabled:opacity-50"
        >
          {t('schedule.edit')}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={onDuplicate}
          className="text-[10px] text-t-text hover:underline disabled:opacity-50"
        >
          {t('schedule.duplicate')}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={onToggleHistory}
          className="text-[10px] text-t-text hover:underline disabled:opacity-50"
        >
          {historyOpen ? t('schedule.hideHistory') : t('schedule.showHistory')}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={onDelete}
          className="text-[10px] text-t-error hover:underline disabled:opacity-50"
        >
          {t('schedule.delete')}
        </button>
      </div>
    </div>
  );
}

function RunHistorySection({
  automationId,
  onOpenTask,
}: {
  automationId: string;
  onOpenTask: (taskId: string) => void;
}) {
  const { t } = useT();
  const [runs, setRuns] = useState<AutomationRunRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [errorText, setErrorText] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setErrorText(null);
    void fetchAutomationRuns(automationId, 20)
      .then((list) => {
        if (!cancelled) setRuns(list);
      })
      .catch((e) => {
        if (!cancelled) {
          setErrorText(e instanceof Error ? e.message : String(e));
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [automationId]);

  if (loading) {
    return (
      <p className="mt-2 text-[10px] text-t-text-muted">{t('schedule.historyLoading')}</p>
    );
  }
  if (errorText) {
    return <p className="mt-2 text-[10px] text-t-error">{errorText}</p>;
  }
  if (runs.length === 0) {
    return (
      <p className="mt-2 text-[10px] text-t-text-muted">{t('schedule.noRuns')}</p>
    );
  }

  return (
    <div className="mt-2 rounded-md border border-card-border/70 bg-canvas p-2 space-y-1.5">
      <div className="text-[10px] font-medium text-t-text">{t('schedule.runHistory')}</div>
      {runs.map((run) => (
        <div
          key={run.id}
          className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[10px] border-b border-card-border/40 last:border-0 pb-1.5 last:pb-0"
        >
          <span className={`font-medium ${RUN_STATUS_COLOR[run.status] ?? 'text-t-text-muted'}`}>
            {t(`schedule.runStatus.${run.status}` as 'schedule.runStatus.completed')}
          </span>
          <span className="text-t-text-muted">{formatDateTime(run.scheduled_for)}</span>
          {run.task_id ? (
            <button
              type="button"
              onClick={() => onOpenTask(run.task_id!)}
              className="text-accent hover:underline font-mono"
            >
              {t('schedule.openTask')} {run.task_id.slice(0, 8)}
            </button>
          ) : null}
          {run.error ? (
            <span className="text-t-error truncate max-w-full" title={run.error}>
              {run.error}
            </span>
          ) : null}
        </div>
      ))}
    </div>
  );
}

function AutomationForm({
  mode,
  initialItem,
  onCancel,
  onSaved,
}: {
  mode: 'create' | 'edit' | 'duplicate';
  initialItem: AutomationRecord | null;
  onCancel: () => void;
  onSaved: () => Promise<void>;
}) {
  const { t } = useT();
  const defaults = defaultScheduleFormValues();
  const initial = initialItem
    ? formValuesFromAutomation(initialItem)
    : null;

  const [name, setName] = useState(
    mode === 'duplicate' && initial ? `${initial.name} (${t('schedule.copySuffix')})` : initial?.name ?? '',
  );
  const [prompt, setPrompt] = useState(initial?.prompt ?? '');
  const [triggerKind, setTriggerKind] = useState<AutomationTriggerKind>(initial?.triggerKind ?? 'prompt');
  const [mode_, setMode] = useState(initial?.mode ?? 'agent');
  const [model, setModel] = useState(initial?.model ?? '');
  const [workspace, setWorkspace] = useState(initial?.workspace ?? '');
  const [allowShell, setAllowShell] = useState(initial?.allowShell ?? false);
  const [trustMode, setTrustMode] = useState(initial?.trustMode ?? false);
  const [autoApprove, setAutoApprove] = useState(initial?.autoApprove ?? true);
  const [scheduleKind, setScheduleKind] = useState<ScheduleKind>(initial?.scheduleKind ?? defaults.scheduleKind);
  const [intervalMinutes, setIntervalMinutes] = useState(initial?.intervalMinutes ?? defaults.intervalMinutes);
  const [intervalHours, setIntervalHours] = useState(initial?.intervalHours ?? defaults.intervalHours);
  const [intervalDays, setIntervalDays] = useState(initial?.intervalDays ?? defaults.intervalDays);
  const [intervalMonths, setIntervalMonths] = useState(initial?.intervalMonths ?? defaults.intervalMonths);
  const [monthDay, setMonthDay] = useState(initial?.monthDay ?? defaults.monthDay);
  const [onceAt, setOnceAt] = useState(initial?.onceAt ?? defaults.onceAt);
  const [days, setDays] = useState<string[]>(initial?.days ?? defaults.days);
  const [hour, setHour] = useState(initial?.hour ?? defaults.hour);
  const [minute, setMinute] = useState(initial?.minute ?? defaults.minute);
  const [customRrule, setCustomRrule] = useState(initial?.customRrule ?? '');
  const [restrictWeekdays, setRestrictWeekdays] = useState(initial?.restrictWeekdays ?? false);
  const [submitting, setSubmitting] = useState(false);
  const [errorText, setErrorText] = useState<string | null>(null);

  const toggleDay = (day: string) => {
    setDays((prev) => (prev.includes(day) ? prev.filter((d) => d !== day) : [...prev, day]));
  };

  const applyWorkdays = () => {
    setScheduleKind('weekly');
    setDays([...WORKDAYS]);
  };

  const scheduleValues = (): ScheduleFormValues => ({
    scheduleKind,
    intervalMinutes,
    intervalHours,
    intervalDays,
    intervalMonths,
    days,
    hour,
    minute,
    monthDay,
    onceAt,
    customRrule,
    restrictWeekdays,
  });

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    const trimmedName = name.trim();
    const trimmedPrompt = prompt.trim();
    if (!trimmedName || !trimmedPrompt) return;

    const rrule = buildRrule(scheduleValues());

    setSubmitting(true);
    setErrorText(null);
    try {
      if (mode === 'edit' && initialItem) {
        const body: UpdateAutomationRequest = {
          name: trimmedName,
          prompt: trimmedPrompt,
          rrule,
          trigger_kind: triggerKind,
          cwds: workspace.trim() ? [workspace.trim()] : [],
        };
        if (triggerKind === 'task') {
          body.mode = mode_;
          body.model = model.trim() || undefined;
          body.allow_shell = allowShell;
          body.trust_mode = trustMode;
          body.auto_approve = autoApprove;
        }
        await updateAutomation(initialItem.id, body);
      } else {
        const body: CreateAutomationRequest = {
          name: trimmedName,
          prompt: trimmedPrompt,
          rrule,
          trigger_kind: triggerKind,
        };
        const ws = workspace.trim();
        if (ws) body.cwds = [ws];
        if (triggerKind === 'task') {
          body.mode = mode_;
          if (model.trim()) body.model = model.trim();
          body.allow_shell = allowShell;
          body.trust_mode = trustMode;
          body.auto_approve = autoApprove;
        }
        await createAutomation(body);
      }
      await onSaved();
    } catch (err) {
      setErrorText(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  const titleKey =
    mode === 'edit'
      ? 'schedule.editTitle'
      : mode === 'duplicate'
        ? 'schedule.duplicateTitle'
        : 'schedule.createTitle';

  const scheduleKindBtn = (kind: ScheduleKind, labelKey: string) => (
    <button
      key={kind}
      type="button"
      onClick={() => setScheduleKind(kind)}
      disabled={submitting}
      className={`px-2.5 py-1 text-[10px] font-medium rounded-md border transition-colors disabled:opacity-50 ${
        scheduleKind === kind
          ? 'border-accent/40 bg-accent-soft text-accent'
          : 'border-card-border bg-canvas text-t-text-muted hover:text-t-text'
      }`}
    >
      {t(labelKey as 'schedule.kindDaily')}
    </button>
  );

  return (
    <form
      onSubmit={(e) => void submit(e)}
      className="mb-4 rounded-lg border border-card-border bg-canvas-alt p-3 space-y-2"
    >
      <div className="text-[11px] font-medium text-t-text">{t(titleKey)}</div>
      <p className="text-[10px] text-t-text-muted leading-relaxed">{t('schedule.createDesc')}</p>

      <div className="flex flex-wrap gap-1.5">
        {(['prompt', 'task'] as AutomationTriggerKind[]).map((kind) => (
          <button
            key={kind}
            type="button"
            onClick={() => setTriggerKind(kind)}
            disabled={submitting}
            className={`px-2.5 py-1 text-[10px] font-medium rounded-md border transition-colors disabled:opacity-50 ${
              triggerKind === kind
                ? 'border-accent/40 bg-accent-soft text-accent'
                : 'border-card-border bg-canvas text-t-text-muted hover:text-t-text'
            }`}
          >
            {kind === 'task' ? t('schedule.triggerTask') : t('schedule.triggerPrompt')}
          </button>
        ))}
      </div>

      <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
        {t('schedule.name')}
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
          disabled={submitting}
          className="rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text"
        />
      </label>
      <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
        {triggerKind === 'task' ? t('schedule.taskPrompt') : t('schedule.prompt')}
        <textarea
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          rows={3}
          required
          disabled={submitting}
          className="rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text resize-y min-h-[64px]"
        />
      </label>

      {triggerKind === 'task' && (
        <div className="rounded-md border border-card-border/70 bg-canvas p-2.5 space-y-2">
          <div className="grid grid-cols-2 gap-2">
            <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
              {t('schedule.mode')}
              <select
                value={mode_}
                onChange={(e) => setMode(e.target.value)}
                disabled={submitting}
                className="rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text"
              >
                {MODE_OPTIONS.map((o) => (
                  <option key={o.value} value={o.value}>{o.label}</option>
                ))}
              </select>
            </label>
            <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
              {t('schedule.modelOptional')}
              <input
                type="text"
                value={model}
                onChange={(e) => setModel(e.target.value)}
                disabled={submitting}
                className="rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text font-mono"
              />
            </label>
          </div>
          <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
            {t('schedule.workspacePathOptional')}
            <input
              type="text"
              value={workspace}
              onChange={(e) => setWorkspace(e.target.value)}
              disabled={submitting}
              className="rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text font-mono"
            />
          </label>
          <div className="flex flex-wrap gap-x-4 gap-y-1 text-[10px] text-t-text">
            <label className="inline-flex items-center gap-1.5 cursor-pointer">
              <input type="checkbox" checked={allowShell} onChange={(e) => setAllowShell(e.target.checked)} disabled={submitting} />
              {t('schedule.allowShell')}
            </label>
            <label className="inline-flex items-center gap-1.5 cursor-pointer">
              <input type="checkbox" checked={trustMode} onChange={(e) => setTrustMode(e.target.checked)} disabled={submitting} />
              {t('schedule.trustMode')}
            </label>
            <label className="inline-flex items-center gap-1.5 cursor-pointer">
              <input type="checkbox" checked={autoApprove} onChange={(e) => setAutoApprove(e.target.checked)} disabled={submitting} />
              {t('schedule.autoApprove')}
            </label>
          </div>
        </div>
      )}

      <div className="flex items-center justify-between gap-2 pt-1 flex-wrap">
        <div className="text-[10px] font-medium text-t-text">{t('schedule.scheduleSection')}</div>
        <button
          type="button"
          onClick={applyWorkdays}
          disabled={submitting}
          className="text-[10px] text-accent hover:underline disabled:opacity-50"
        >
          {t('schedule.applyWorkdays')}
        </button>
      </div>

      <div className="flex flex-wrap gap-1.5">
        {scheduleKindBtn('minutely', 'schedule.kindMinutely')}
        {scheduleKindBtn('hourly', 'schedule.kindHourly')}
        {scheduleKindBtn('daily', 'schedule.kindDaily')}
        {scheduleKindBtn('weekly', 'schedule.kindWeekly')}
        {scheduleKindBtn('monthly', 'schedule.kindMonthly')}
        {scheduleKindBtn('once', 'schedule.kindOnce')}
        {scheduleKindBtn('custom', 'schedule.kindCustom')}
      </div>

      {scheduleKind === 'custom' ? (
        <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
          {t('schedule.customRrule')}
          <textarea
            value={customRrule}
            onChange={(e) => setCustomRrule(e.target.value)}
            rows={2}
            required
            disabled={submitting}
            placeholder="FREQ=WEEKLY;BYDAY=MO,WE,FR;BYHOUR=9;BYMINUTE=0"
            className="rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text font-mono resize-y"
          />
          <span className="text-[9px] leading-relaxed">{t('schedule.customRruleHint')}</span>
        </label>
      ) : scheduleKind === 'minutely' ? (
        <div className="space-y-2">
          <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
            {t('schedule.intervalMinutes')}
            <input
              type="number"
              min={1}
              max={120}
              value={intervalMinutes}
              onChange={(e) => setIntervalMinutes(Math.max(1, Number(e.target.value) || 1))}
              disabled={submitting}
              className="w-24 rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text"
            />
          </label>
          <label className="inline-flex items-center gap-1.5 text-[10px] text-t-text cursor-pointer">
            <input
              type="checkbox"
              checked={restrictWeekdays}
              onChange={(e) => setRestrictWeekdays(e.target.checked)}
              disabled={submitting}
            />
            {t('schedule.workdaysOnly')}
          </label>
        </div>
      ) : scheduleKind === 'hourly' ? (
        <div className="space-y-2">
          <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
            {t('schedule.intervalHours')}
            <input
              type="number"
              min={1}
              max={24}
              value={intervalHours}
              onChange={(e) => setIntervalHours(Math.max(1, Number(e.target.value) || 1))}
              disabled={submitting}
              className="w-24 rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text"
            />
          </label>
          <label className="inline-flex items-center gap-1.5 text-[10px] text-t-text cursor-pointer">
            <input
              type="checkbox"
              checked={restrictWeekdays}
              onChange={(e) => setRestrictWeekdays(e.target.checked)}
              disabled={submitting}
            />
            {t('schedule.workdaysOnly')}
          </label>
        </div>
      ) : scheduleKind === 'once' ? (
        <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
          {t('schedule.onceAt')}
          <input
            type="datetime-local"
            value={onceAt}
            onChange={(e) => setOnceAt(e.target.value)}
            required
            disabled={submitting}
            className="rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text"
          />
          <span className="text-[9px] leading-relaxed">{t('schedule.onceHint')}</span>
        </label>
      ) : (
        <div className="space-y-2">
          {scheduleKind === 'weekly' && (
            <>
              <div className="text-[10px] text-t-text-muted">{t('schedule.weekdays')}</div>
              <div className="flex flex-wrap gap-1">
                {WEEKDAYS.map((day) => (
                  <button
                    key={day}
                    type="button"
                    onClick={() => toggleDay(day)}
                    disabled={submitting}
                    className={`px-2 py-0.5 text-[10px] rounded border disabled:opacity-50 ${
                      days.includes(day)
                        ? 'border-accent/40 bg-accent-soft text-accent'
                        : 'border-card-border text-t-text-muted'
                    }`}
                  >
                    {t(`schedule.weekday.${day}` as 'schedule.weekday.MO')}
                  </button>
                ))}
              </div>
            </>
          )}
          {scheduleKind === 'daily' && (
            <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
              {t('schedule.intervalDays')}
              <input
                type="number"
                min={1}
                max={30}
                value={intervalDays}
                onChange={(e) => setIntervalDays(Math.max(1, Number(e.target.value) || 1))}
                disabled={submitting}
                className="w-24 rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text"
              />
            </label>
          )}
          {scheduleKind === 'monthly' && (
            <div className="flex flex-wrap gap-2">
              <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
                {t('schedule.monthDay')}
                <input
                  type="number"
                  min={1}
                  max={31}
                  value={monthDay}
                  onChange={(e) => setMonthDay(Math.min(31, Math.max(1, Number(e.target.value) || 1)))}
                  disabled={submitting}
                  className="w-20 rounded-md border border-card-border bg-canvas px-2 py-1 text-xs"
                />
              </label>
              <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
                {t('schedule.intervalMonths')}
                <input
                  type="number"
                  min={1}
                  max={12}
                  value={intervalMonths}
                  onChange={(e) => setIntervalMonths(Math.max(1, Number(e.target.value) || 1))}
                  disabled={submitting}
                  className="w-20 rounded-md border border-card-border bg-canvas px-2 py-1 text-xs"
                />
              </label>
            </div>
          )}
          <div className="flex gap-2">
            <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
              {t('schedule.hour')}
              <input
                type="number"
                min={0}
                max={23}
                value={hour}
                onChange={(e) => setHour(Math.min(23, Math.max(0, Number(e.target.value) || 0)))}
                disabled={submitting}
                className="w-20 rounded-md border border-card-border bg-canvas px-2 py-1 text-xs"
              />
            </label>
            <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
              {t('schedule.minute')}
              <input
                type="number"
                min={0}
                max={59}
                value={minute}
                onChange={(e) => setMinute(Math.min(59, Math.max(0, Number(e.target.value) || 0)))}
                disabled={submitting}
                className="w-20 rounded-md border border-card-border bg-canvas px-2 py-1 text-xs"
              />
            </label>
          </div>
        </div>
      )}

      {errorText && <p className="text-[10px] text-t-error">{errorText}</p>}

      <div className="flex justify-end gap-2 pt-1">
        <button
          type="button"
          onClick={onCancel}
          disabled={submitting}
          className="px-3 py-1.5 text-xs font-medium rounded-md border border-card-border text-t-text hover:bg-hover disabled:opacity-40"
        >
          {t('schedule.cancel')}
        </button>
        <button
          type="submit"
          disabled={submitting || !name.trim() || !prompt.trim()}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-accent text-white hover:opacity-90 disabled:opacity-40"
        >
          {submitting
            ? t('schedule.submitting')
            : mode === 'edit'
              ? t('schedule.save')
              : t('schedule.submit')}
        </button>
      </div>
    </form>
  );
}
