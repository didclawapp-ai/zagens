import { useState } from 'react';
import type { RightPanelView } from './RightPanel';
import type { RuntimeConnectionState } from '../api/client';

interface SessionInfo {
  id: string;
  name: string;
  created_at?: number;
  updated_at?: number;
}

type Theme = 'light' | 'dark';

interface Props {
  sessions: SessionInfo[];
  activeSessionId: string | null;
  onNewSession: () => void;
  onSelectSession?: (id: string) => void;
  onDeleteSession?: (id: string) => void;
  desktopHost: boolean;
  runtimeConn: RuntimeConnectionState;
  apiKeyConfigured: boolean | null;
  activeInspector: RightPanelView;
  onInspectorChange: (view: RightPanelView) => void;
  theme: Theme;
  onToggleTheme: () => void;
}

const navBtn = (active: boolean) =>
  `w-full text-left px-3 py-2 rounded-lg text-sm transition-colors ${
    active
      ? 'bg-hover-strong text-accent border border-accent/20'
      : 'text-t-text-secondary hover:bg-hover hover:text-t-text'
  }`;

export default function Sidebar({
  sessions,
  activeSessionId,
  onNewSession,
  onSelectSession,
  onDeleteSession,
  desktopHost,
  runtimeConn,
  apiKeyConfigured,
  activeInspector,
  onInspectorChange,
  theme,
  onToggleTheme,
}: Props) {
  return (
    <aside
      className="flex w-60 shrink-0 flex-col border-r border-rail-edge bg-canvas"
      aria-label="会话与导航"
    >
      <div className="shrink-0 border-b border-divider/60 px-3.5 py-3.5">
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-2 flex-1 px-2.5 py-2 rounded-lg bg-hover">
            <span className="flex size-[22px] items-center justify-center rounded-md bg-gradient-to-br from-blue-300 to-blue-600 text-[11px] text-white">
              ✦
            </span>
            <span className="text-sm font-semibold text-t-text">
              DS<span className="opacity-70 font-medium"> Pick</span>
            </span>
          </div>
        </div>
      </div>

      <div className="flex flex-col gap-0.5 px-2 py-2">
        <button
          type="button"
          onClick={onNewSession}
          className="nav-item"
        >
          <svg viewBox="0 0 24 24">
            <path d="M12 5v14M5 12h14" />
          </svg>
          新对话
        </button>
        <button
          type="button"
          className={navBtn(activeInspector === 'workspace')}
          onClick={() => onInspectorChange('workspace')}
          aria-label="工作台"
        >
          <svg viewBox="0 0 24 24" className="inline w-4 h-4 mr-2 stroke-current align-text-bottom" style={{ fill: 'none', strokeWidth: 1.6 }}>
            <path d="M4 6h16v12H4z M8 6V4h8v2" />
          </svg>
          工作台
        </button>
        {/* 设置 accordion — sub-items expand/collapse on click */}
        <SettingsAccordion
          activeInspector={activeInspector}
          onInspectorChange={onInspectorChange}
          desktopHost={desktopHost}
        />
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto px-2 py-2">
        <p className="px-2.5 py-2 text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
          会话
        </p>
        {sessions.length === 0 && (
          <p className="text-xs text-t-text-muted px-2.5 py-4 text-center">暂无会话</p>
        )}
        {sessions.map((s) => {
          const isActive = activeSessionId != null && s.id === activeSessionId;
          return (
            <div
              key={s.id}
              className={`flex items-center gap-1 rounded-lg group ${
                isActive ? 'bg-accent-soft ring-1 ring-accent/30' : 'hover:bg-hover'
              }`}
            >
              <button
                type="button"
                onClick={() => onSelectSession?.(s.id)}
                className={`flex-1 min-w-0 px-3 py-2 text-sm text-left truncate ${
                  isActive ? 'text-accent font-medium' : 'text-t-text'
                }`}
              >
                {s.name || s.id.slice(0, 8)}
              </button>
              {onDeleteSession && (
                <button
                  type="button"
                  title="删除会话"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDeleteSession(s.id);
                  }}
                  className="shrink-0 px-2 py-2 text-t-text-muted hover:text-t-error opacity-0 group-hover:opacity-100 transition-opacity"
                >
                  ×
                </button>
              )}
            </div>
          );
        })}
      </div>

      <div className="shrink-0 border-t border-divider/60 px-3 py-2.5 space-y-2">
        <div className="flex items-center gap-2 px-1 py-1 text-xs text-t-text-muted"
          title="与本地 deepseek-tui 运行时 (127.0.0.1:7878) 的连接状态">
          <span
            className={`shrink-0 inline-block w-2 h-2 rounded-full ${
              runtimeConn === 'connected'
                ? 'bg-emerald-500'
                : runtimeConn === 'auth_mismatch'
                  ? 'bg-amber-400'
                  : runtimeConn === 'offline'
                    ? 'bg-red-500'
                    : 'bg-gray-400'
            }`}
          />
          <span className="truncate">
            {runtimeConn === 'checking' && '检测运行时…'}
            {runtimeConn === 'connected' && '运行时就绪'}
            {runtimeConn === 'offline' && '离线'}
            {runtimeConn === 'auth_mismatch' && '令牌不一致'}
          </span>
        </div>
        {desktopHost && apiKeyConfigured === false && (
          <p className="px-1 text-[10px] text-amber-text/90 leading-snug">未配置 API Key</p>
        )}
        <button
          type="button"
          onClick={onToggleTheme}
          className="flex items-center gap-2 w-full px-2 py-1.5 rounded-lg text-xs text-t-text-secondary hover:bg-hover transition-colors"
          title={theme === 'light' ? '切换到暗色模式' : '切换到浅色模式'}
        >
          <svg viewBox="0 0 24 24" className="w-4 h-4 stroke-current" style={{ fill: 'none', strokeWidth: 1.6 }}>
            {theme === 'light' ? (
              <path d="M21 12.79A9 9 0 1111.21 3 7 7 0 0021 12.79z" />
            ) : (
              <circle cx="12" cy="12" r="5" />
            )}
            {theme === 'light' ? null : (
              <path d="M12 1v2M12 21v2M4.22 4.22l1.42 1.42M18.36 18.36l1.42 1.42M1 12h2M21 12h2M4.22 19.78l1.42-1.42M18.36 5.64l1.42-1.42" />
            )}
          </svg>
          {theme === 'light' ? '暗色模式' : '浅色模式'}
        </button>
      </div>

      <div className="shrink-0 px-3.5 py-2.5 border-t border-divider/60 space-y-0.5">
        <p className="text-[10px] text-t-text-muted">DS Pick v0.2.1</p>
        <p className="text-[10px] text-t-text-muted/80 leading-snug">
          基于 DeepSeek TUI 运行时（<code className="font-mono">deepseek</code> CLI）
        </p>
      </div>
    </aside>
  );
}

/* ------------------------------------------------------------------ */
/*  Settings accordion: expands sub-nav items below the 设置 toggle   */
/* ------------------------------------------------------------------ */

type SettingsTab = 'api-key' | 'mcp' | 'usage' | 'tasks-skills' | 'agents' | 'routing';

function subNavBtn(active: boolean) {
  return `w-full text-left pl-7 pr-3 py-2 rounded-lg text-xs transition-colors ${
    active
      ? 'bg-hover-strong text-accent border border-accent/20'
      : 'text-t-text-muted hover:bg-hover hover:text-t-text-secondary'
  }`;
}

function SettingsAccordion({
  activeInspector,
  onInspectorChange,
  desktopHost,
}: {
  activeInspector: RightPanelView;
  onInspectorChange: (v: RightPanelView) => void;
  desktopHost: boolean;
}) {
  const [open, setOpen] = useState(false);

  const isSubActive = (tab: SettingsTab) => activeInspector === tab;

  const handleSubClick = (tab: SettingsTab) => {
    setOpen(true);
    onInspectorChange(tab);
  };

  const subItems: { tab: SettingsTab; label: string; show: boolean }[] = [
    { tab: 'api-key', label: 'API Key', show: desktopHost },
    { tab: 'mcp', label: 'MCP 服务器', show: true },
    { tab: 'usage', label: '用量仪表盘', show: true },
    /** 右栏仍为 `automation` view id；仅含任务 + 技能，定时自动化不展示 */
    { tab: 'tasks-skills', label: '任务与技能', show: true },
    { tab: 'agents', label: '子代理', show: true },
    { tab: 'routing', label: '模型路由', show: true },
  ];

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className={navBtn(activeInspector === 'settings' || isSubActive('api-key') || isSubActive('mcp') || isSubActive('usage') || isSubActive('tasks-skills') || isSubActive('agents') || isSubActive('routing'))}
      >
        <svg viewBox="0 0 24 24" className="inline w-4 h-4 mr-2 stroke-current align-text-bottom" style={{ fill: 'none', strokeWidth: 1.6 }}>
          <path d="M4 14l4-4 4 4 8-8" />
          <path d="M4 20h16" />
        </svg>
        设置
        <svg
          viewBox="0 0 24 24"
          className={`ml-auto w-3.5 h-3.5 stroke-current transition-transform ${open ? 'rotate-90' : ''}`}
          style={{ fill: 'none', strokeWidth: 2 }}
        >
          <path d="M9 5l7 7-7 7" />
        </svg>
      </button>

      <div
        className={`overflow-hidden transition-[max-height] duration-200 ${open ? 'max-h-80' : 'max-h-0'}`}
      >
        <div className="flex flex-col gap-0.5 pt-0.5 pb-1">
          {subItems
            .filter((it) => it.show)
            .map((it) => (
              <button
                key={it.tab}
                type="button"
                className={subNavBtn(isSubActive(it.tab))}
                onClick={() => handleSubClick(it.tab)}
              >
                {it.label}
              </button>
            ))}
        </div>
      </div>
    </>
  );
}
