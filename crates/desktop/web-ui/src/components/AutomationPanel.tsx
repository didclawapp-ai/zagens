import { useCallback, useEffect, useState, type FormEvent } from 'react';
import {
  fetchTasks,
  fetchSkills,
  createTask,
  createSkill,
  importSkillLocal,
  installSkillRemote,
  cancelTask,
  clearFinishedTasks,
  type RuntimeConnectionState,
} from '../api/client';
import type { TaskSummary, SkillEntry, CreateTaskRequest } from '../types/automation';
import { isTerminalTaskStatus } from '../types/automation';
import { useT } from '../i18n';
import { isRuntimeApiAvailable } from '../lib/runtimeReachable';
import { confirmDialog } from '../lib/confirmDialog';
import { markTasksSeen } from '../lib/inspectorUnread';
import { toast } from '../lib/toast';

/** 定时自动化（GET /v1/automations）暂不展示 — 见 docs/desktop/TUI_DS_PICK_GAP.md */
type TabId = 'tasks' | 'skills';

const TAB_LABELS: Record<TabId, string> = {
  tasks: '任务',
  skills: '技能',
};

function tabBtn(active: boolean) {
  return `px-3 py-1.5 text-xs font-medium rounded transition-colors ${
    active
      ? 'bg-accent-soft text-accent'
      : 'text-t-text-muted hover:text-t-text hover:bg-hover'
  }`;
}

const TASK_STATUS_LABEL: Record<string, string> = {
  queued: '排队中',
  pending: '排队中',
  running: '运行中',
  paused: '已暂停',
  completed: '已完成',
  failed: '失败',
  canceled: '已取消',
};

const TASK_STATUS_COLOR: Record<string, string> = {
  queued: 'text-t-text-muted',
  pending: 'text-t-text-muted',
  running: 'text-amber-text',
  paused: 'text-t-text-muted',
  completed: 'text-success',
  failed: 'text-t-error',
  canceled: 'text-t-text-muted',
};

const MODE_OPTIONS: Array<{ value: string; label: string }> = [
  { value: 'agent', label: 'Agent' },
  { value: 'plan', label: 'Plan' },
  { value: 'yolo', label: 'YOLO' },
];

function canCancelTask(status: string): boolean {
  return status === 'queued' || status === 'running' || status === 'pending' || status === 'paused';
}

export type AutomationPanelVariant = 'tasks' | 'skills' | 'both';

export default function AutomationPanel({
  runtimeConn,
  streaming = false,
  runtimeSessionEstablished = false,
  variant = 'both',
}: {
  runtimeConn: RuntimeConnectionState;
  streaming?: boolean;
  runtimeSessionEstablished?: boolean;
  /** U2: split Task vs Skills into separate inspector views. */
  variant?: AutomationPanelVariant;
}) {
  const { t } = useT();
  const runtimeReady = isRuntimeApiAvailable(runtimeConn, {
    streaming,
    sessionEstablished: runtimeSessionEstablished,
  });
  const lockedTab: TabId = variant === 'skills' ? 'skills' : 'tasks';
  const [tab, setTab] = useState<TabId>(lockedTab);
  const showTabBar = variant === 'both';
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  const [skills, setSkills] = useState<SkillEntry[]>([]);
  const [skillsDirectory, setSkillsDirectory] = useState<string>('');
  const [skillWarnings, setSkillWarnings] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showCreateTask, setShowCreateTask] = useState(false);
  const [showCreateSkill, setShowCreateSkill] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [cancelingId, setCancelingId] = useState<string | null>(null);
  const [clearingFinished, setClearingFinished] = useState(false);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      if (variant === 'tasks') {
        const t = await fetchTasks();
        setTasks(t);
      } else if (variant === 'skills') {
        const s = await fetchSkills();
        setSkills(s.skills);
        setSkillsDirectory(typeof s.directory === 'string' ? s.directory : String(s.directory ?? ''));
        setSkillWarnings(s.warnings ?? []);
      } else {
        const [t, s] = await Promise.all([fetchTasks(), fetchSkills()]);
        setTasks(t);
        setSkills(s.skills);
        setSkillsDirectory(typeof s.directory === 'string' ? s.directory : String(s.directory ?? ''));
        setSkillWarnings(s.warnings ?? []);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [variant]);

  useEffect(() => {
    if (runtimeReady) {
      reload();
    }
  }, [runtimeReady, reload]);

  const handleCreateTask = async (req: CreateTaskRequest) => {
    setCreating(true);
    setCreateError(null);
    try {
      await createTask(req);
      setShowCreateTask(false);
      await reload();
    } catch (e) {
      setCreateError(e instanceof Error ? e.message : String(e));
    } finally {
      setCreating(false);
    }
  };

  const handleCancelTask = async (id: string) => {
    setCancelingId(id);
    try {
      await cancelTask(id);
      await reload();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setCancelingId(null);
    }
  };

  const terminalTaskCount = tasks.filter((t) => isTerminalTaskStatus(t.status)).length;

  const handleClearFinishedTasks = async () => {
    if (terminalTaskCount === 0) {
      return;
    }
    const ok = await confirmDialog(
      t('automation.clearFinishedConfirm', { count: String(terminalTaskCount) }),
      t('automation.clearFinishedTitle'),
    );
    if (!ok) {
      return;
    }
    setClearingFinished(true);
    setError(null);
    try {
      const { removed } = await clearFinishedTasks();
      markTasksSeen([]);
      await reload();
      if (removed > 0) {
        toast.success(t('automation.clearFinishedDone', { count: String(removed) }));
      }
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      toast.error(msg);
    } finally {
      setClearingFinished(false);
    }
  };

  if (!runtimeReady) {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center space-y-2">
        <p>{t('automation.waitingRuntime')}</p>
        <p className="text-[10px]">{t('automation.waitingDetail')}</p>
      </div>
    );
  }

  const emptyTasks = variant !== 'skills' && tasks.length === 0;
  const emptySkills = variant !== 'tasks' && skills.length === 0;
  const panelEmpty = emptyTasks && emptySkills;

  if (loading && panelEmpty) {
    return <div className="p-4 text-xs text-t-text-muted text-center">{t('automation.loading')}</div>;
  }

  if (error && panelEmpty) {
    return (
      <div className="p-4 space-y-2">
        <p className="text-xs text-t-error">{t('automation.loadFailed', { error })}</p>
        <button type="button" onClick={reload} className="text-xs text-accent hover:underline">
          {t('automation.retry')}
        </button>
      </div>
    );
  }

  const activeTab = showTabBar ? tab : lockedTab;

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-divider shrink-0 flex-wrap">
        {showTabBar && (
          <div className="flex items-center gap-1">
            {(Object.keys(TAB_LABELS) as TabId[]).map((k) => (
              <button key={k} type="button" onClick={() => setTab(k)} className={tabBtn(tab === k)}>
                {TAB_LABELS[k]}
              </button>
            ))}
          </div>
        )}
        <div className={`${showTabBar ? 'ml-auto' : ''} flex items-center gap-2`}>
          {activeTab === 'tasks' && (
            <>
              <button
                type="button"
                onClick={() => void handleClearFinishedTasks()}
                disabled={clearingFinished || terminalTaskCount === 0}
                className="px-2.5 py-1 text-[11px] font-medium rounded-md border border-card-border bg-canvas-alt hover:bg-hover text-t-text disabled:opacity-50"
                title={t('automation.clearFinishedHint')}
              >
                {clearingFinished ? t('automation.clearingFinished') : t('automation.clearFinished')}
              </button>
              <button
                type="button"
                onClick={() => {
                  setShowCreateTask((v) => !v);
                  setCreateError(null);
                }}
                className="px-2.5 py-1 text-[11px] font-medium rounded-md border border-card-border bg-canvas-alt hover:bg-hover text-t-text"
              >
                {showCreateTask ? t('automation.close') : t('automation.newTask')}
              </button>
            </>
          )}
          {activeTab === 'skills' && (
            <>
              <button
                type="button"
                onClick={() => {
                  setShowCreateSkill((v) => !v);
                }}
                className="px-2.5 py-1 text-[11px] font-medium rounded-md border border-card-border bg-canvas-alt hover:bg-hover text-t-text"
              >
                {showCreateSkill ? t('automation.close') : t('automation.addSkill')}
              </button>
              <button
                type="button"
                onClick={() => reload()}
                disabled={loading}
                className="px-2.5 py-1 text-[11px] font-medium rounded-md border border-card-border bg-canvas-alt hover:bg-hover text-t-text disabled:opacity-50"
              >
                刷新
              </button>
            </>
          )}
        </div>
      </div>

      <div className="overflow-y-auto px-3 py-2 flex-1 min-h-0">
        {activeTab === 'tasks' && (
          <>
            {showCreateTask && (
              <CreateTaskForm onSubmit={handleCreateTask} submitting={creating} errorText={createError} />
            )}
            <TasksList tasks={tasks} onCancel={handleCancelTask} cancelingId={cancelingId} />
          </>
        )}
        {activeTab === 'skills' && (
          <SkillsList
            skills={skills}
            skillsDirectory={skillsDirectory}
            warnings={skillWarnings}
            onRefresh={reload}
            loading={loading}
            showCreate={showCreateSkill}
            onSkillCreated={async () => {
              setShowCreateSkill(false);
              await reload();
            }}
          />
        )}
      </div>
    </div>
  );
}

function CreateTaskForm({
  onSubmit,
  submitting,
  errorText,
}: {
  onSubmit: (req: CreateTaskRequest) => void;
  submitting: boolean;
  errorText: string | null;
}) {
  const [prompt, setPrompt] = useState('');
  const [mode, setMode] = useState('agent');
  const [model, setModel] = useState('');
  const [workspace, setWorkspace] = useState('');
  const [allowShell, setAllowShell] = useState(false);
  const [trustMode, setTrustMode] = useState(false);
  const [autoApprove, setAutoApprove] = useState(false);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const body: CreateTaskRequest = {
      prompt: prompt.trim(),
      mode,
      ...(model.trim() ? { model: model.trim() } : {}),
      ...(workspace.trim() ? { workspace: workspace.trim() } : {}),
      allow_shell: allowShell,
      trust_mode: trustMode,
      auto_approve: autoApprove,
    };
    onSubmit(body);
  };

  return (
    <form
      onSubmit={submit}
      className="mb-4 rounded-lg border border-card-border bg-canvas-alt p-3 space-y-2"
    >
      <div className="text-[11px] font-medium text-t-text">新建后台任务</div>
      <p className="text-[10px] text-t-text-muted leading-relaxed">
        对应运行时的 <span className="font-mono">POST /v1/tasks</span>
        。未填工作区时使用当前运行时工作区；未填模型时使用默认文本模型。
      </p>
      <textarea
        value={prompt}
        onChange={(e) => setPrompt(e.target.value)}
        placeholder="任务描述 / 提示词（必填）"
        rows={4}
        className="w-full rounded-md border border-card-border bg-canvas px-2 py-1.5 text-xs text-t-text placeholder:text-t-text-muted/70 resize-y min-h-[72px]"
        required
        disabled={submitting}
      />
      <div className="grid grid-cols-2 gap-2">
        <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
          模式
          <select
            value={mode}
            onChange={(e) => setMode(e.target.value)}
            disabled={submitting}
            className="rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text"
          >
            {MODE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
        </label>
        <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
          模型（可选）
          <input
            type="text"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="默认模型"
            disabled={submitting}
            className="rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text placeholder:text-t-text-muted/60"
          />
        </label>
      </div>
      <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
        工作区路径（可选）
        <input
          type="text"
          value={workspace}
          onChange={(e) => setWorkspace(e.target.value)}
          placeholder="留空则使用运行时默认工作区"
          disabled={submitting}
          className="rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text placeholder:text-t-text-muted/60 font-mono"
        />
      </label>
      <div className="flex flex-wrap gap-x-4 gap-y-1 text-[10px] text-t-text">
        <label className="inline-flex items-center gap-1.5 cursor-pointer">
          <input
            type="checkbox"
            checked={allowShell}
            onChange={(e) => setAllowShell(e.target.checked)}
            disabled={submitting}
          />
          允许 Shell
        </label>
        <label className="inline-flex items-center gap-1.5 cursor-pointer">
          <input
            type="checkbox"
            checked={trustMode}
            onChange={(e) => setTrustMode(e.target.checked)}
            disabled={submitting}
          />
          信任模式
        </label>
        <label className="inline-flex items-center gap-1.5 cursor-pointer">
          <input
            type="checkbox"
            checked={autoApprove}
            onChange={(e) => setAutoApprove(e.target.checked)}
            disabled={submitting}
          />
          自动批准工具
        </label>
      </div>
      {errorText && <p className="text-[10px] text-t-error">{errorText}</p>}
      <div className="flex justify-end gap-2 pt-1">
        <button
          type="submit"
          disabled={submitting || !prompt.trim()}
          className="px-3 py-1.5 text-xs font-medium rounded-md bg-accent text-white hover:opacity-90 disabled:opacity-40"
        >
          {submitting ? '提交中…' : '创建'}
        </button>
      </div>
    </form>
  );
}

function TasksList({
  tasks,
  onCancel,
  cancelingId,
}: {
  tasks: TaskSummary[];
  onCancel: (id: string) => void;
  cancelingId: string | null;
}) {
  if (tasks.length === 0) {
    return (
      <p className="text-xs text-t-text-muted text-center py-6">
        暂无任务。点击「新建任务」可enqueue一条后台任务。
      </p>
    );
  }
  return (
    <div className="space-y-2">
      {tasks.map((t) => (
        <div key={t.id} className="rounded-lg border border-card-border bg-canvas-alt p-3">
          <div className="flex items-center gap-2">
            <span className="font-mono text-[10px] text-t-text-muted shrink-0">{t.id.slice(0, 10)}</span>
            <span className="text-xs text-t-text truncate flex-1 min-w-0">{t.prompt_summary}</span>
            <span className={`text-[10px] font-medium shrink-0 ${TASK_STATUS_COLOR[t.status] ?? 'text-t-text-muted'}`}>
              {TASK_STATUS_LABEL[t.status] ?? t.status}
            </span>
            {canCancelTask(t.status) && (
              <button
                type="button"
                onClick={() => onCancel(t.id)}
                disabled={cancelingId === t.id}
                className="shrink-0 text-[10px] text-t-error hover:underline disabled:opacity-50"
              >
                {cancelingId === t.id ? '取消中…' : '取消'}
              </button>
            )}
          </div>
          <div className="mt-1 text-[10px] text-t-text-muted">
            {t.model} · {t.mode}
            {t.duration_ms != null && ` · ${(t.duration_ms / 1000).toFixed(1)}s`}
          </div>
        </div>
      ))}
    </div>
  );
}

type TranslateFn = ReturnType<typeof useT>['t'];

function SkillWriteLocationFields({
  t,
  scope,
  setScope,
  customParent,
  setCustomParent,
  pickBusy,
  submitting,
  onPickRoot,
  replaceExisting,
  setReplaceExisting,
  showReplace,
}: {
  t: TranslateFn;
  scope: 'global' | 'workspace';
  setScope: (v: 'global' | 'workspace') => void;
  customParent: string;
  setCustomParent: (v: string) => void;
  pickBusy: boolean;
  submitting: boolean;
  onPickRoot: () => void;
  replaceExisting: boolean;
  setReplaceExisting: (v: boolean) => void;
  showReplace: boolean;
}) {
  return (
    <>
      <div className="text-[10px] text-t-text-muted flex flex-col gap-1">
        <span className="text-t-text">{t('automation.writeLocation')}</span>
        <label className="inline-flex items-center gap-1.5 cursor-pointer">
          <input
            type="radio"
            name="skill-scope"
            checked={scope === 'workspace'}
            onChange={() => setScope('workspace')}
            disabled={submitting}
          />
          {t('automation.skillWorkspace')}
        </label>
        <label className="inline-flex items-center gap-1.5 cursor-pointer">
          <input
            type="radio"
            name="skill-scope"
            checked={scope === 'global'}
            onChange={() => setScope('global')}
            disabled={submitting}
          />
          {t('automation.skillGlobal')}
        </label>
      </div>
      <div className="space-y-1">
        <div className="text-[10px] text-t-text-muted">{t('automation.skillCustomDir')}</div>
        <div className="flex flex-wrap gap-2 items-center">
          <input
            type="text"
            value={customParent}
            onChange={(ev) => setCustomParent(ev.target.value)}
            placeholder={t('automation.skillCustomPlaceholder')}
            disabled={submitting}
            className="flex-1 min-w-[160px] rounded-md border border-card-border bg-canvas px-2 py-1 text-[10px] text-t-text font-mono placeholder:text-t-text-muted/60"
          />
          <button
            type="button"
            onClick={onPickRoot}
            disabled={submitting || pickBusy}
            className="px-2 py-1 text-[10px] rounded-md border border-card-border bg-canvas hover:bg-hover text-t-text disabled:opacity-50"
          >
            {pickBusy ? t('automation.selectingFolder') : t('automation.selectFolder')}
          </button>
          {customParent ? (
            <button
              type="button"
              onClick={() => setCustomParent('')}
              disabled={submitting}
              className="text-[10px] text-accent hover:underline disabled:opacity-50"
            >
              {t('automation.clear')}
            </button>
          ) : null}
        </div>
        <p className="text-[10px] text-t-text-muted leading-relaxed">{t('automation.skillCustomNotice')}</p>
      </div>
      {showReplace ? (
        <label className="inline-flex items-center gap-1.5 text-[10px] text-t-text-muted cursor-pointer">
          <input
            type="checkbox"
            checked={replaceExisting}
            onChange={(ev) => setReplaceExisting(ev.target.checked)}
            disabled={submitting}
          />
          {t('automation.replaceExisting')}
        </label>
      ) : null}
    </>
  );
}

function SkillsList({
  skills,
  skillsDirectory,
  warnings,
  onRefresh,
  loading,
  showCreate,
  onSkillCreated,
}: {
  skills: SkillEntry[];
  skillsDirectory: string;
  warnings: string[];
  onRefresh: () => void;
  loading: boolean;
  showCreate: boolean;
  onSkillCreated: () => Promise<void>;
}) {
  const { t } = useT();
  const [panelMode, setPanelMode] = useState<'create' | 'import'>('create');
  const [importKind, setImportKind] = useState<'folder' | 'remote'>('folder');
  const [skillName, setSkillName] = useState('');
  const [sourceDirectory, setSourceDirectory] = useState('');
  const [installSpec, setInstallSpec] = useState('');
  const [replaceExisting, setReplaceExisting] = useState(false);
  const [scope, setScope] = useState<'global' | 'workspace'>('workspace');
  const [customParent, setCustomParent] = useState('');
  const [pickBusy, setPickBusy] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const scopePayload = () => ({
    scope,
    ...(customParent.trim() ? { parent_directory: customParent.trim() } : {}),
    ...(replaceExisting ? { replace: true } : {}),
  });

  const pickDirectory = async (title: string, onPick: (dir: string) => void, defaultPath?: string) => {
    setPickBusy(true);
    setSubmitError(null);
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({
        directory: true,
        multiple: false,
        title,
        ...(defaultPath?.trim()
          ? ({ defaultPath: defaultPath.trim() } as Record<string, string>)
          : {}),
      });
      const dir = firstDirectoryFromPickerResult(selected);
      if (dir) {
        onPick(dir);
      }
    } catch {
      setSubmitError(t('automation.dialogNotAvailable'));
    } finally {
      setPickBusy(false);
    }
  };

  const pickSkillsRoot = () =>
    void pickDirectory(t('automation.selectSkillRoot'), (dir) => setCustomParent(dir), skillsDirectory);

  const pickSkillSource = () =>
    void pickDirectory(t('automation.pickSkillSourceDialog'), (dir) => setSourceDirectory(dir));

  const resetFormFields = () => {
    setSkillName('');
    setSourceDirectory('');
    setInstallSpec('');
    setCustomParent('');
    setReplaceExisting(false);
  };

  const submitCreate = async (e: FormEvent) => {
    e.preventDefault();
    const name = skillName.trim();
    if (!name) {
      setSubmitError(t('automation.skillDirRequired'));
      return;
    }
    setSubmitting(true);
    setSubmitError(null);
    try {
      await createSkill({ name, ...scopePayload() });
      resetFormFields();
      await onSkillCreated();
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  const submitImportFolder = async (e: FormEvent) => {
    e.preventDefault();
    const src = sourceDirectory.trim();
    if (!src) {
      setSubmitError(t('automation.importSourceRequired'));
      return;
    }
    setSubmitting(true);
    setSubmitError(null);
    try {
      await importSkillLocal({ source_directory: src, ...scopePayload() });
      resetFormFields();
      await onSkillCreated();
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  const submitInstallRemote = async (e: FormEvent) => {
    e.preventDefault();
    const spec = installSpec.trim();
    if (!spec) {
      setSubmitError(t('automation.installSpecRequired'));
      return;
    }
    setSubmitting(true);
    setSubmitError(null);
    try {
      await installSkillRemote({ spec, ...scopePayload() });
      resetFormFields();
      await onSkillCreated();
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  const modeTab = (id: 'create' | 'import', label: string) => (
    <button
      key={id}
      type="button"
      onClick={() => setPanelMode(id)}
      disabled={submitting}
      className={`px-2.5 py-1 text-[10px] font-medium rounded-md border transition-colors disabled:opacity-50 ${
        panelMode === id
          ? 'border-accent/40 bg-accent-soft text-accent'
          : 'border-card-border bg-canvas text-t-text-muted hover:text-t-text hover:bg-hover'
      }`}
    >
      {label}
    </button>
  );

  const importKindTab = (id: 'folder' | 'remote', label: string) => (
    <button
      key={id}
      type="button"
      onClick={() => setImportKind(id)}
      disabled={submitting}
      className={`px-2 py-0.5 text-[10px] rounded border transition-colors disabled:opacity-50 ${
        importKind === id
          ? 'border-accent/30 bg-accent-soft/60 text-accent'
          : 'border-transparent text-t-text-muted hover:text-t-text'
      }`}
    >
      {label}
    </button>
  );

  return (
    <div className="space-y-3">
      {showCreate && (
        <div className="rounded-lg border border-card-border bg-canvas-alt p-3 space-y-2 mb-2">
          <div className="flex flex-wrap gap-1.5">
            {modeTab('create', t('automation.skillModeCreate'))}
            {modeTab('import', t('automation.skillModeImport'))}
          </div>
          {panelMode === 'create' ? (
            <form onSubmit={(ev) => void submitCreate(ev)} className="space-y-2">
              <div className="text-[11px] font-medium text-t-text">{t('automation.createSkillTitle')}</div>
              <p className="text-[10px] text-t-text-muted leading-relaxed">{t('automation.createSkillDesc')}</p>
              <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
                {t('automation.skillDirName')}
                <input
                  type="text"
                  value={skillName}
                  onChange={(ev) => setSkillName(ev.target.value)}
                  placeholder={t('automation.skillNamePlaceholder')}
                  disabled={submitting}
                  className="rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text font-mono placeholder:text-t-text-muted/60"
                />
              </label>
              <SkillWriteLocationFields
                t={t}
                scope={scope}
                setScope={setScope}
                customParent={customParent}
                setCustomParent={setCustomParent}
                pickBusy={pickBusy}
                submitting={submitting}
                onPickRoot={pickSkillsRoot}
                replaceExisting={replaceExisting}
                setReplaceExisting={setReplaceExisting}
                showReplace={false}
              />
              {submitError ? <p className="text-[10px] text-t-error break-all">{submitError}</p> : null}
              <div className="flex justify-end pt-1">
                <button
                  type="submit"
                  disabled={submitting || !skillName.trim()}
                  className="px-3 py-1.5 text-xs font-medium rounded-md bg-accent text-white hover:opacity-90 disabled:opacity-40"
                >
                  {submitting ? t('automation.creating') : t('automation.submit')}
                </button>
              </div>
            </form>
          ) : (
            <div className="space-y-2">
              <div className="flex flex-wrap gap-1 border-b border-card-border/60 pb-1.5">
                {importKindTab('folder', t('automation.importFromFolder'))}
                {importKindTab('remote', t('automation.importFromNetwork'))}
              </div>
              {importKind === 'folder' ? (
                <form onSubmit={(ev) => void submitImportFolder(ev)} className="space-y-2">
                  <div className="text-[11px] font-medium text-t-text">{t('automation.importFolderTitle')}</div>
                  <p className="text-[10px] text-t-text-muted leading-relaxed">{t('automation.importFolderDesc')}</p>
                  <div className="space-y-1">
                    <div className="text-[10px] text-t-text-muted">{t('automation.importFolderSource')}</div>
                    <div className="flex flex-wrap gap-2 items-center">
                      <input
                        type="text"
                        value={sourceDirectory}
                        onChange={(ev) => setSourceDirectory(ev.target.value)}
                        placeholder={t('automation.skillCustomPlaceholder')}
                        disabled={submitting}
                        className="flex-1 min-w-[160px] rounded-md border border-card-border bg-canvas px-2 py-1 text-[10px] text-t-text font-mono placeholder:text-t-text-muted/60"
                      />
                      <button
                        type="button"
                        onClick={pickSkillSource}
                        disabled={submitting || pickBusy}
                        className="px-2 py-1 text-[10px] rounded-md border border-card-border bg-canvas hover:bg-hover text-t-text disabled:opacity-50"
                      >
                        {pickBusy ? t('automation.selectingFolder') : t('automation.pickSkillSource')}
                      </button>
                    </div>
                    <p className="text-[10px] text-t-text-muted leading-relaxed">{t('automation.importSkillSourceHint')}</p>
                  </div>
                  <SkillWriteLocationFields
                    t={t}
                    scope={scope}
                    setScope={setScope}
                    customParent={customParent}
                    setCustomParent={setCustomParent}
                    pickBusy={pickBusy}
                    submitting={submitting}
                    onPickRoot={pickSkillsRoot}
                    replaceExisting={replaceExisting}
                    setReplaceExisting={setReplaceExisting}
                    showReplace
                  />
                  {submitError ? <p className="text-[10px] text-t-error break-all">{submitError}</p> : null}
                  <div className="flex justify-end pt-1">
                    <button
                      type="submit"
                      disabled={submitting || !sourceDirectory.trim()}
                      className="px-3 py-1.5 text-xs font-medium rounded-md bg-accent text-white hover:opacity-90 disabled:opacity-40"
                    >
                      {submitting ? t('automation.importing') : t('automation.importAction')}
                    </button>
                  </div>
                </form>
              ) : (
                <form onSubmit={(ev) => void submitInstallRemote(ev)} className="space-y-2">
                  <div className="text-[11px] font-medium text-t-text">{t('automation.importRemoteTitle')}</div>
                  <p className="text-[10px] text-t-text-muted leading-relaxed">{t('automation.importRemoteDesc')}</p>
                  <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
                    {t('automation.installSpec')}
                    <input
                      type="text"
                      value={installSpec}
                      onChange={(ev) => setInstallSpec(ev.target.value)}
                      placeholder={t('automation.installSpecPlaceholder')}
                      disabled={submitting}
                      className="rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text font-mono placeholder:text-t-text-muted/60"
                    />
                  </label>
                  <SkillWriteLocationFields
                    t={t}
                    scope={scope}
                    setScope={setScope}
                    customParent={customParent}
                    setCustomParent={setCustomParent}
                    pickBusy={pickBusy}
                    submitting={submitting}
                    onPickRoot={pickSkillsRoot}
                    replaceExisting={replaceExisting}
                    setReplaceExisting={setReplaceExisting}
                    showReplace
                  />
                  {submitError ? <p className="text-[10px] text-t-error break-all">{submitError}</p> : null}
                  <div className="flex justify-end pt-1">
                    <button
                      type="submit"
                      disabled={submitting || !installSpec.trim()}
                      className="px-3 py-1.5 text-xs font-medium rounded-md bg-accent text-white hover:opacity-90 disabled:opacity-40"
                    >
                      {submitting ? t('automation.installing') : t('automation.installAction')}
                    </button>
                  </div>
                </form>
              )}
            </div>
          )}
        </div>
      )}
      <div className="rounded-lg border border-card-border bg-canvas-alt p-3 space-y-2">
        <p className="text-[10px] text-t-text-muted leading-relaxed">{t('automation.skillDirectoryDesc')}</p>
        {skillsDirectory ? (
          <p className="text-[10px] font-mono text-t-text break-all">{skillsDirectory}</p>
        ) : (
          <p className="text-[10px] text-t-text-muted">{t('automation.skillLoadAfterConnect')}</p>
        )}
        <p className="text-[10px] text-t-text-muted leading-relaxed">{t('automation.skillFooterDescImport')}</p>
        <button
          type="button"
          onClick={onRefresh}
          disabled={loading}
          className="text-[10px] text-accent hover:underline disabled:opacity-50"
        >
          {t('automation.reloadList')}
        </button>
      </div>
      {warnings.length > 0 && (
        <ul className="text-[10px] text-amber-text list-disc pl-4 space-y-0.5">
          {warnings.map((w, i) => (
            <li key={i}>{w}</li>
          ))}
        </ul>
      )}
      {skills.length === 0 ? (
        <p className="text-xs text-t-text-muted text-center py-4">{t('automation.noSkills')}</p>
      ) : (
        <div className="space-y-2">
          {skills.map((s) => (
            <div key={s.name} className="rounded-lg border border-card-border bg-canvas-alt p-3">
              <div className="text-xs font-semibold text-t-text">{s.name}</div>
              <div className="mt-0.5 text-[10px] text-t-text-muted">{s.description}</div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function firstDirectoryFromPickerResult(selected: unknown): string | null {
  if (selected == null) {
    return null;
  }
  if (typeof selected === 'string' && selected.trim().length > 0) {
    return selected;
  }
  if (Array.isArray(selected) && typeof selected[0] === 'string' && selected[0].trim().length > 0) {
    return selected[0];
  }
  return null;
}
