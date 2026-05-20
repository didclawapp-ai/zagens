import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../i18n';
import type { RightPanelView } from './RightPanel';
import type { RuntimeConnectionState } from '../api/client';
import PanelEdgeSeam from './PanelEdgeSeam';

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
  /** Whether sidebar is collapsed. When true, the parent should render a toggle strip instead. */
  collapsed: boolean;
  /** Called when collapse button clicked. */
  onToggleCollapse: () => void;
  /** Office task sessions hide code-only inspector tabs. */
  officeSession?: boolean;
}

const SIDEBAR_WIDTH_KEY = 'deepseek-desktop-sidebar-width';
const SIDEBAR_MIN_PX = 180;
const SIDEBAR_DEFAULT_PX = 240;

function clampSidebarWidth(px: number): number {
  if (typeof window === 'undefined') {
    return Math.max(SIDEBAR_MIN_PX, Math.round(px));
  }
  const cap = Math.min(560, Math.floor(window.innerWidth * 0.45));
  return Math.min(cap, Math.max(SIDEBAR_MIN_PX, Math.round(px)));
}

function readStoredSidebarWidth(): number {
  try {
    const n = parseInt(localStorage.getItem(SIDEBAR_WIDTH_KEY) ?? '', 10);
    if (Number.isFinite(n)) {
      return clampSidebarWidth(n);
    }
  } catch {
    /* ignore */
  }
  return SIDEBAR_DEFAULT_PX;
}

const navBtn = (active: boolean) =>
  `w-full text-left px-3 py-2 rounded-lg text-sm transition-colors ${
    active
      ? 'bg-hover-strong font-medium text-accent'
      : 'text-t-text hover:bg-hover'
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
  collapsed,
  onToggleCollapse,
  officeSession = false,
}: Props) {
  const { t } = useT();
  const [sidebarWidth, setSidebarWidth] = useState(readStoredSidebarWidth);
  const resizeDragRef = useRef<{ pointerId: number; startX: number; startW: number } | null>(null);
  const liveSidebarWidthRef = useRef(sidebarWidth);
  const [sidebarResizing, setSidebarResizing] = useState(false);

  useEffect(() => {
    liveSidebarWidthRef.current = sidebarWidth;
  }, [sidebarWidth]);

  useEffect(() => {
    const onResize = () => {
      setSidebarWidth((w) => clampSidebarWidth(w));
    };
    window.addEventListener('resize', onResize);
    return () => window.removeEventListener('resize', onResize);
  }, []);

  const endSidebarResize = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    const d = resizeDragRef.current;
    if (!d || e.pointerId !== d.pointerId) {
      return;
    }
    resizeDragRef.current = null;
    setSidebarResizing(false);
    if (el.hasPointerCapture(e.pointerId)) {
      el.releasePointerCapture(e.pointerId);
    }
    const finalW =
      e.type === 'pointerup'
        ? clampSidebarWidth(d.startW + (e.clientX - d.startX))
        : liveSidebarWidthRef.current;
    setSidebarWidth(finalW);
    try {
      localStorage.setItem(SIDEBAR_WIDTH_KEY, String(finalW));
    } catch {
      /* ignore */
    }
  }, []);

  const onResizePointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) {
        return;
      }
      e.preventDefault();
      resizeDragRef.current = { pointerId: e.pointerId, startX: e.clientX, startW: sidebarWidth };
      setSidebarResizing(true);
      e.currentTarget.setPointerCapture(e.pointerId);
    },
    [sidebarWidth],
  );

  const onResizePointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    const d = resizeDragRef.current;
    if (!d || e.pointerId !== d.pointerId) {
      return;
    }
    const next = clampSidebarWidth(d.startW + (e.clientX - d.startX));
    liveSidebarWidthRef.current = next;
    setSidebarWidth(next);
  }, []);

  return (
    <>
    <aside
      className={`flex shrink-0 flex-col bg-canvas overflow-hidden ${
        sidebarResizing ? '' : 'transition-[width] duration-150'
      }`}
      style={{ width: collapsed ? 0 : sidebarWidth }}
      aria-label="会话与导航"
    >
      <div className="shrink-0 px-4 pt-5 pb-2">
        <div className="mb-4 flex min-w-0 items-center gap-2.5 pl-0.5">
          <img
            src="/app-icon.png"
            alt=""
            className="size-6 shrink-0 rounded-md object-cover"
            width={24}
            height={24}
          />
          <span className="truncate text-base font-semibold text-accent">DS Pick</span>
        </div>
      </div>

      <div className="flex flex-col gap-0.5 px-3 py-1">
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
        {!officeSession && (
          <button
            type="button"
            className={navBtn(activeInspector === 'checklist')}
            onClick={() => onInspectorChange('checklist')}
            aria-label={t('sidebar.checklist')}
          >
            <svg viewBox="0 0 24 24" className="inline w-4 h-4 mr-2 stroke-current align-text-bottom" style={{ fill: 'none', strokeWidth: 1.6 }}>
              <path d="M9 6h11M9 12h11M9 18h11M5 6h.01M5 12h.01M5 18h.01" strokeLinecap="round" />
            </svg>
            {t('sidebar.checklist')}
          </button>
        )}
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
                isActive ? 'bg-msg-user' : 'hover:bg-hover'
              }`}
            >
              <button
                type="button"
                onClick={() => onSelectSession?.(s.id)}
                className={`flex-1 min-w-0 px-3 py-2 text-sm text-left truncate ${
                  isActive ? 'font-medium text-t-text' : 'text-t-text'
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
        {desktopHost && apiKeyConfigured === false && (
          <p className="px-1 text-[10px] text-amber-text/90 leading-snug">未配置 API Key</p>
        )}
        <div
          className="flex items-center gap-2 px-1 py-1 text-xs text-t-text-muted"
          title="与本地 deepseek-tui 运行时 (127.0.0.1:7878) 的连接状态"
        >
          <span
            className={`shrink-0 inline-block w-2 h-2 rounded-full ${
              runtimeConn === 'connected' ? 'bg-emerald-500' : 'bg-red-500'
            }`}
          />
          <span className="truncate">
            {runtimeConn === 'connected'
              ? t('common.connectionNormal')
              : t('common.connectionDisconnected')}
          </span>
        </div>
      </div>

    </aside>
    {!collapsed && (
      <PanelEdgeSeam
        side="left"
        seamClass="chrome-seam-r"
        resizing={sidebarResizing}
        ariaResize={t('sidebar.resizeWidth')}
        collapseTitle={t('sidebar.collapse')}
        onCollapse={onToggleCollapse}
        onPointerDown={onResizePointerDown}
        onPointerMove={onResizePointerMove}
        onPointerUp={endSidebarResize}
        onPointerCancel={endSidebarResize}
        onKeyDown={(e) => {
          const step = e.shiftKey ? 32 : 16;
          if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
            e.preventDefault();
            const delta = e.key === 'ArrowRight' ? step : -step;
            setSidebarWidth((w) => {
              const n = clampSidebarWidth(w + delta);
              try {
                localStorage.setItem(SIDEBAR_WIDTH_KEY, String(n));
              } catch {
                /* ignore */
              }
              return n;
            });
          }
        }}
      />
    )}
    </>
  );
}

/* ------------------------------------------------------------------ */
/*  Settings accordion: expands sub-nav items below the 设置 toggle   */
/* ------------------------------------------------------------------ */

type SettingsTab =
  | 'api-key'
  | 'mcp'
  | 'usage'
  | 'tasks-skills'
  | 'agents'
  | 'routing'
  | 'system'
  | 'index'
  | 'about';

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
  const { t } = useT();
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
    { tab: 'about', label: t('sidebar.about'), show: true },
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
