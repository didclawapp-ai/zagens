import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../i18n';
import type { RightPanelView } from './RightPanel';
import type { RuntimeConnectionState } from '../api/client';

interface SessionInfo {
  id: string;
  name: string;
  created_at?: number;
  updated_at?: number;
}

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
  /** Current sidebar width in px. */
  sidebarWidth: number;
  /** Called on drag resize. */
  onSidebarWidthChange: (px: number) => void;
  /** Whether sidebar is collapsed. When true, the parent should render a toggle strip instead. */
  collapsed: boolean;
  /** Called when collapse button clicked. */
  onToggleCollapse: () => void;
  /** Office task sessions hide code-only inspector tabs. */
  officeSession?: boolean;
}

const SIDEBAR_MIN_PX = 180;
const SIDEBAR_MAX_PX = 480;

function clampSidebarWidth(px: number): number {
  return Math.min(SIDEBAR_MAX_PX, Math.max(SIDEBAR_MIN_PX, Math.round(px)));
}

const navBtn = (active: boolean) =>
  `w-full text-left px-3 py-2 rounded-lg text-sm transition-colors ${
    active
      ? 'bg-hover-strong text-accent border border-accent/14'
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
  sidebarWidth,
  onSidebarWidthChange,
  collapsed,
  onToggleCollapse,
  officeSession = false,
}: Props) {
  const { t } = useT();

  // ---- resize handle -------------------------------------------------------
  const draggingRef = useRef(false);
  const startXRef = useRef(0);
  const startWRef = useRef(0);

  const onPointerDown = useCallback(
    (e: React.PointerEvent) => {
      draggingRef.current = true;
      startXRef.current = e.clientX;
      startWRef.current = sidebarWidth;
      (e.target as HTMLElement).setPointerCapture(e.pointerId);
    },
    [sidebarWidth],
  );

  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      if (!draggingRef.current) return;
      const delta = e.clientX - startXRef.current;
      const next = clampSidebarWidth(startWRef.current + delta);
      onSidebarWidthChange(next);
    },
    [onSidebarWidthChange],
  );

  const onPointerUp = useCallback(
    (e: React.PointerEvent) => {
      if (!draggingRef.current) return;
      draggingRef.current = false;
      const delta = e.clientX - startXRef.current;
      const next = clampSidebarWidth(startWRef.current + delta);
      onSidebarWidthChange(next);
      (e.target as HTMLElement).releasePointerCapture(e.pointerId);
    },
    [onSidebarWidthChange],
  );

  return (
    <aside
      className="flex shrink-0 flex-col border-r border-rail-edge bg-canvas overflow-hidden transition-[width] duration-150"
      style={{ width: collapsed ? 0 : sidebarWidth }}
      aria-label="会话与导航"
    >
      <div className="shrink-0 border-b border-divider px-3.5 py-3.5">
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-2 flex-1 px-2.5 py-2 rounded-lg bg-hover">
            <span className="flex size-[22px] items-center justify-center rounded-md bg-gradient-to-br from-blue-300 to-blue-600 text-[11px] text-white">
              ✦
            </span>
            <span className="text-sm font-semibold text-t-text">
              DS<span className="opacity-70 font-medium"> Pick</span>
            </span>
          </div>
          <button
            type="button"
            onClick={onToggleCollapse}
            className="p-1 rounded text-t-text-muted hover:text-t-text hover:bg-hover transition-colors shrink-0"
            title="收起侧边栏"
          >
            <svg className="w-4 h-4" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
              <path d="M11 4l-6 4 6 4" strokeLinecap="round" strokeLinejoin="round"/>
            </svg>
          </button>
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
          {t('sidebar.newSession')}
        </button>
        <button
          type="button"
          className={navBtn(activeInspector === 'workspace')}
          onClick={() => onInspectorChange('workspace')}
          aria-label={t('sidebar.workspace')}
        >
          <svg viewBox="0 0 24 24" className="inline w-4 h-4 mr-2 stroke-current align-text-bottom" style={{ fill: 'none', strokeWidth: 1.6 }}>
            <path d="M4 6h16v12H4z M8 6V4h8v2" />
          </svg>
          {t('sidebar.workspace')}
        </button>
        <SettingsAccordion
          activeInspector={activeInspector}
          onInspectorChange={onInspectorChange}
          desktopHost={desktopHost}
          officeSession={officeSession}
        />
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto px-2 py-2">
        <p className="px-2.5 py-2 text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
          Sessions
        </p>
        {sessions.length === 0 && (
          <p className="text-xs text-t-text-muted px-2.5 py-4 text-center">{t('common.noSessions')}</p>
        )}
        {sessions.map((s) => {
          const isActive = activeSessionId != null && s.id === activeSessionId;
          return (
            <div
              key={s.id}
              className={`flex items-center gap-1 rounded-lg group ${
                isActive ? 'bg-accent-soft ring-1 ring-accent/22' : 'hover:bg-hover'
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

      <div className="shrink-0 border-t border-divider px-3 py-2.5 space-y-2">
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
            {runtimeConn === 'checking' && t('common.runtimeChecking')}
            {runtimeConn === 'connected' && t('common.runtimeReady')}
            {runtimeConn === 'offline' && t('common.runtimeOffline')}
            {runtimeConn === 'auth_mismatch' && t('common.runtimeAuthMismatch')}
          </span>
        </div>
        {desktopHost && apiKeyConfigured === false && (
          <p className="px-1 text-[10px] text-amber-text/90 leading-snug">未配置 API Key</p>
        )}
      </div>

      <div className="shrink-0 px-3.5 py-2 border-t border-divider space-y-1.5">
        <p className="text-[10px] text-t-text-muted">DS Pick v0.2.2</p>
        <p className="text-[10px] text-t-text-muted/80 leading-snug">
          基于 DeepSeek TUI 运行时（<code className="font-mono">deepseek</code> CLI）
        </p>
      </div>

      {/* Resize handle — right edge */}
      {!collapsed && (
        <div
          className="absolute top-0 right-0 w-1 h-full cursor-col-resize hover:bg-accent/30 transition-colors z-10"
          style={{ background: draggingRef.current ? 'var(--tw-color-accent) / 0.25' : undefined }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
        />
      )}
    </aside>
  );
}

/* ------------------------------------------------------------------ */
/*  Settings accordion: expands sub-nav items below the 设置 toggle   */
/* ------------------------------------------------------------------ */

type SettingsTab = 'api-key' | 'mcp' | 'usage' | 'tasks-skills' | 'agents' | 'routing' | 'system' | 'index';

function subNavBtn(active: boolean) {
  return `w-full text-left pl-7 pr-3 py-2 rounded-lg text-xs transition-colors ${
    active
      ? 'bg-hover-strong text-accent border border-accent/14'
      : 'text-t-text-muted hover:bg-hover hover:text-t-text-secondary'
  }`;
}

function SettingsAccordion({
  activeInspector,
  onInspectorChange,
  desktopHost,
  officeSession,
}: {
  activeInspector: RightPanelView;
  onInspectorChange: (v: RightPanelView) => void;
  desktopHost: boolean;
  officeSession: boolean;
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
    { tab: 'tasks-skills', label: '任务与技能', show: true },
    { tab: 'agents', label: '子代理', show: !officeSession },
    { tab: 'routing', label: '模型路由', show: !officeSession },
    { tab: 'index', label: '索引', show: !officeSession },
    { tab: 'system', label: '系统设置', show: true },
  ];

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className={navBtn(activeInspector === 'settings' || subItems.some(({ tab }) => isSubActive(tab)))}
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
