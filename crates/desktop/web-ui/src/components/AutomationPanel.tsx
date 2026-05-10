import { useCallback, useEffect, useState } from 'react';
import { fetchTasks, fetchSkills, type RuntimeConnectionState } from '../api/client';
import type { TaskSummary, SkillEntry } from '../types/automation';

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
  pending: '等待中',
  running: '运行中',
  paused: '已暂停',
  completed: '已完成',
  failed: '失败',
  canceled: '已取消',
};

const TASK_STATUS_COLOR: Record<string, string> = {
  pending: 'text-t-text-muted',
  running: 'text-amber-text',
  paused: 'text-t-text-muted',
  completed: 'text-success',
  failed: 'text-t-error',
  canceled: 'text-t-text-muted',
};

export default function AutomationPanel({ runtimeConn }: { runtimeConn: RuntimeConnectionState }) {
  const [tab, setTab] = useState<TabId>('tasks');
  const [tasks, setTasks] = useState<TaskSummary[]>([]);
  const [skills, setSkills] = useState<SkillEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [t, s] = await Promise.all([fetchTasks(), fetchSkills()]);
      setTasks(t);
      setSkills(s.skills);
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

  if (runtimeConn !== 'connected') {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center space-y-2">
        <p>等待运行时连接…</p>
        <p className="text-[10px]">任务与技能将在运行时就绪后自动加载。</p>
      </div>
    );
  }

  if (loading && tasks.length === 0 && skills.length === 0) {
    return <div className="p-4 text-xs text-t-text-muted text-center">正在加载…</div>;
  }

  if (error && tasks.length === 0 && skills.length === 0) {
    return (
      <div className="p-4 space-y-2">
        <p className="text-xs text-t-error">加载失败：{error}</p>
        <button type="button" onClick={reload} className="text-xs text-accent hover:underline">
          重试
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <div className="flex items-center gap-1 px-3 py-2 border-b border-divider shrink-0">
        {(Object.keys(TAB_LABELS) as TabId[]).map((k) => (
          <button key={k} type="button" onClick={() => setTab(k)} className={tabBtn(tab === k)}>
            {TAB_LABELS[k]}
          </button>
        ))}
      </div>

      <div className="overflow-y-auto px-3 py-2">
        {tab === 'tasks' && <TasksList tasks={tasks} />}
        {tab === 'skills' && <SkillsList skills={skills} />}
      </div>
    </div>
  );
}

function TasksList({ tasks }: { tasks: TaskSummary[] }) {
  if (tasks.length === 0) {
    return <p className="text-xs text-t-text-muted text-center py-6">暂无任务。</p>;
  }
  return (
    <div className="space-y-2">
      {tasks.map((t) => (
        <div key={t.id} className="rounded-lg border border-card-border bg-canvas-alt p-3">
          <div className="flex items-center gap-2">
            <span className="font-mono text-[10px] text-t-text-muted">{t.id.slice(0, 10)}</span>
            <span className="text-xs text-t-text truncate flex-1">{t.prompt_summary}</span>
            <span className={`text-[10px] font-medium ${TASK_STATUS_COLOR[t.status] ?? 'text-t-text-muted'}`}>
              {TASK_STATUS_LABEL[t.status] ?? t.status}
            </span>
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

function SkillsList({ skills }: { skills: SkillEntry[] }) {
  if (skills.length === 0) {
    return <p className="text-xs text-t-text-muted text-center py-6">未安装技能。</p>;
  }
  return (
    <div className="space-y-2">
      {skills.map((s) => (
        <div key={s.name} className="rounded-lg border border-card-border bg-canvas-alt p-3">
          <div className="text-xs font-semibold text-t-text">{s.name}</div>
          <div className="mt-0.5 text-[10px] text-t-text-muted">{s.description}</div>
        </div>
      ))}
    </div>
  );
}
