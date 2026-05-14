import { useCallback, useEffect, useState, type FormEvent } from 'react';
import {
  fetchTasks,
  fetchSkills,
  createTask,
  createSkill,
  cancelTask,
  type RuntimeConnectionState,
} from '../api/client';
import type { TaskSummary, SkillEntry, CreateTaskRequest } from '../types/automation';
import { useT } from '../i18n';

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

export default function AutomationPanel({ runtimeConn }: { runtimeConn: RuntimeConnectionState }) {
  const { t } = useT();
  const [tab, setTab] = useState<TabId>('tasks');
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

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [t, s] = await Promise.all([fetchTasks(), fetchSkills()]);
      setTasks(t);
      setSkills(s.skills);
      setSkillsDirectory(typeof s.directory === 'string' ? s.directory : String(s.directory ?? ''));
      setSkillWarnings(s.warnings ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (runtimeConn === 'connected') {
      reload();
    }
  }, [runtimeConn, reload]);

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

  if (runtimeConn !== 'connected') {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center space-y-2">
        <p>{t('automation.waitingRuntime')}</p>
        <p className="text-[10px]">{t('automation.waitingDetail')}</p>
      </div>
    );
  }

  if (loading && tasks.length === 0 && skills.length === 0) {
    return <div className="p-4 text-xs text-t-text-muted text-center">{t('automation.loading')}</div>;
  }

  if (error && tasks.length === 0 && skills.length === 0) {
    return (
      <div className="p-4 space-y-2">
        <p className="text-xs text-t-error">{t('automation.loadFailed', { error })}</p>
        <button type="button" onClick={reload} className="text-xs text-accent hover:underline">
          {t('automation.retry')}
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-2 border-b border-divider shrink-0 flex-wrap">
        <div className="flex items-center gap-1">
          {(Object.keys(TAB_LABELS) as TabId[]).map((k) => (
            <button key={k} type="button" onClick={() => setTab(k)} className={tabBtn(tab === k)}>
              {TAB_LABELS[k]}
            </button>
          ))}
        </div>
        <div className="ml-auto flex items-center gap-2">
          {tab === 'tasks' && (
            <button
              type="button"
              onClick={() => {
                setShowCreateTask((v) => !v);
                setCreateError(null);
              }}
              className="px-2.5 py-1 text-[11px] font-medium rounded-md border border-card-border bg-canvas-alt hover:bg-hover text-t-text"
            >
              {showCreateTask ? '关闭' : '新建任务'}
            </button>
          )}
          {tab === 'skills' && (
            <>
              <button
                type="button"
                onClick={() => {
                  setShowCreateSkill((v) => !v);
                }}
                className="px-2.5 py-1 text-[11px] font-medium rounded-md border border-card-border bg-canvas-alt hover:bg-hover text-t-text"
              >
                {showCreateSkill ? '关闭' : '新建技能'}
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
        {tab === 'tasks' && (
          <>
            {showCreateTask && (
              <CreateTaskForm onSubmit={handleCreateTask} submitting={creating} errorText={createError} />
            )}
            <TasksList tasks={tasks} onCancel={handleCancelTask} cancelingId={cancelingId} />
          </>
        )}
        {tab === 'skills' && (
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
  const [skillName, setSkillName] = useState('');
  const [scope, setScope] = useState<'global' | 'workspace'>('workspace');
  const [customParent, setCustomParent] = useState('');
  const [pickBusy, setPickBusy] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const pickSkillsRoot = async () => {
    setPickBusy(true);
    setSubmitError(null);
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({
        directory: true,
        multiple: false,
        title: '选择技能根目录（须为已存在的全局或工作区技能目录）',
        ...(skillsDirectory.trim()
          ? ({ defaultPath: skillsDirectory.trim() } as Record<string, string>)
          : {}),
      });
      const dir = firstDirectoryFromPickerResult(selected);
      if (dir) {
        setCustomParent(dir);
      }
    } catch {
      setSubmitError(
        '无法打开系统文件夹对话框。请在 DS Pick 桌面版重试，或手动填入「自定义根目录」绝对路径。',
      );
    } finally {
      setPickBusy(false);
    }
  };

  const submitCreate = async (e: FormEvent) => {
    e.preventDefault();
    const name = skillName.trim();
    if (!name) {
      setSubmitError('请填写技能目录名');
      return;
    }
    setSubmitting(true);
    setSubmitError(null);
    try {
      await createSkill({
        name,
        scope,
        ...(customParent.trim() ? { parent_directory: customParent.trim() } : {}),
      });
      setSkillName('');
      setCustomParent('');
      await onSkillCreated();
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : String(err));
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="space-y-3">
      {showCreate && (
        <form
          onSubmit={(ev) => void submitCreate(ev)}
          className="rounded-lg border border-card-border bg-canvas-alt p-3 space-y-2 mb-2"
        >
          <div className="text-[11px] font-medium text-t-text">新建技能</div>
          <p className="text-[10px] text-t-text-muted leading-relaxed">
            将在选定目录下创建 <span className="font-mono">{'<名称>/SKILL.md'}</span>，对应运行时{' '}
            <span className="font-mono">POST /v1/skills</span>。目录名仅允许字母、数字、<span className="font-mono">._-</span>。
          </p>
          <label className="text-[10px] text-t-text-muted flex flex-col gap-0.5">
            技能目录名
            <input
              type="text"
              value={skillName}
              onChange={(ev) => setSkillName(ev.target.value)}
              placeholder="例如 my-skill"
              disabled={submitting}
              className="rounded-md border border-card-border bg-canvas px-2 py-1 text-xs text-t-text font-mono placeholder:text-t-text-muted/60"
            />
          </label>
          <div className="text-[10px] text-t-text-muted flex flex-col gap-1">
            <span className="text-t-text">写入位置</span>
            <label className="inline-flex items-center gap-1.5 cursor-pointer">
              <input
                type="radio"
                name="skill-scope"
                checked={scope === 'workspace'}
                onChange={() => setScope('workspace')}
                disabled={submitting}
              />
              工作区（<span className="font-mono">.agents/skills</span> 或 <span className="font-mono">skills/</span>；若均不存在会创建{' '}
              <span className="font-mono">.agents/skills</span>）
            </label>
            <label className="inline-flex items-center gap-1.5 cursor-pointer">
              <input
                type="radio"
                name="skill-scope"
                checked={scope === 'global'}
                onChange={() => setScope('global')}
                disabled={submitting}
              />
              全局（配置中的 skills 目录，一般为 <span className="font-mono">~/.deepseek/skills</span>）
            </label>
          </div>
          <div className="space-y-1">
            <div className="text-[10px] text-t-text-muted">自定义技能根目录（可选，设置后将优先使用该路径）</div>
            <div className="flex flex-wrap gap-2 items-center">
              <input
                type="text"
                value={customParent}
                onChange={(ev) => setCustomParent(ev.target.value)}
                placeholder="绝对路径，留空则以上方「写入位置」为准"
                disabled={submitting}
                className="flex-1 min-w-[160px] rounded-md border border-card-border bg-canvas px-2 py-1 text-[10px] text-t-text font-mono placeholder:text-t-text-muted/60"
              />
              <button
                type="button"
                onClick={() => void pickSkillsRoot()}
                disabled={submitting || pickBusy}
                className="px-2 py-1 text-[10px] rounded-md border border-card-border bg-canvas hover:bg-hover text-t-text disabled:opacity-50"
              >
                {pickBusy ? '选择中…' : '选择文件夹…'}
              </button>
              {customParent ? (
                <button
                  type="button"
                  onClick={() => setCustomParent('')}
                  disabled={submitting}
                  className="text-[10px] text-accent hover:underline disabled:opacity-50"
                >
                  清除
                </button>
              ) : null}
            </div>
            <p className="text-[10px] text-t-text-muted leading-relaxed">
              自定义路径须为<strong>已存在</strong>且已登记的技能根目录（与上方列表目录或全局/工作区约定之一一致）。
            </p>
          </div>
          {submitError ? <p className="text-[10px] text-t-error break-all">{submitError}</p> : null}
          <div className="flex justify-end pt-1">
            <button
              type="submit"
              disabled={submitting || !skillName.trim()}
              className="px-3 py-1.5 text-xs font-medium rounded-md bg-accent text-white hover:opacity-90 disabled:opacity-40"
            >
              {submitting ? '创建中…' : '创建'}
            </button>
          </div>
        </form>
      )}
      <div className="rounded-lg border border-card-border bg-canvas-alt p-3 space-y-2">
        <p className="text-[10px] text-t-text-muted leading-relaxed">
          技能由运行时扫描目录下的 <span className="font-mono">SKILL.md</span> 发现，当前列表目录：
        </p>
        {skillsDirectory ? (
          <p className="text-[10px] font-mono text-t-text break-all">{skillsDirectory}</p>
        ) : (
          <p className="text-[10px] text-t-text-muted">（连接后刷新可见）</p>
        )}
        <p className="text-[10px] text-t-text-muted leading-relaxed">
          可用「新建技能」或 <span className="font-mono">POST /v1/skills</span> 写入模板；社区包仍在终端 TUI 使用{' '}
          <span className="font-mono">/skill install …</span>。若新建位置与上方面板目录不一致，刷新后可能不会出现在列表中（与工作区优先级有关）。
        </p>
        <button
          type="button"
          onClick={onRefresh}
          disabled={loading}
          className="text-[10px] text-accent hover:underline disabled:opacity-50"
        >
          重新加载列表
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
        <p className="text-xs text-t-text-muted text-center py-4">未扫描到技能。</p>
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
