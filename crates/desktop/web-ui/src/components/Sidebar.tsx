import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../i18n';
import type { RightPanelView } from './RightPanel';
import type { RuntimeConnectionState } from '../api/client';
import {
  runtimeConnIndicatorClass,
  runtimeConnStatusLabel,
} from '../lib/runtimeReachable';
import PanelEdgeSeam from './PanelEdgeSeam';
import InspectorActivityDot from './InspectorActivityDot';
import type { InspectorNavActivity } from '../lib/inspectorUnread';
import { usePrefersReducedMotion } from '../lib/usePrefersReducedMotion';

interface SessionInfo {
  id: string;
  name: string;
  created_at?: number;
  updated_at?: number;
}

interface Props {
  sessions: SessionInfo[];
  showAllSessions?: boolean;
  onToggleShowAllSessions?: () => void;
  activeSessionId: string | null;
  onNewSession: () => void;
  onSelectSession?: (id: string) => void;
  onDeleteSession?: (id: string) => void;
  desktopHost: boolean;
  runtimeConn: RuntimeConnectionState;
  /** Active model turn — probes may show degraded while APIs still work. */
  streaming?: boolean;
  runtimeSessionEstablished?: boolean;
  apiKeyConfigured: boolean | null;
  activeInspector: RightPanelView;
  onInspectorChange: (view: RightPanelView) => void;
  /** Whether sidebar is collapsed. When true, the parent should render a toggle strip instead. */
  collapsed: boolean;
  /** Called when collapse button clicked. */
  onToggleCollapse: () => void;
  /** Office task sessions hide code-only inspector tabs. */
  officeSession?: boolean;
  checklistActivity?: InspectorNavActivity;
  auditActivity?: InspectorNavActivity;
  taskActivity?: InspectorNavActivity;
  agentActivity?: InspectorNavActivity;
}

const SIDEBAR_WIDTH_KEY = 'zagens-desktop-sidebar-width';
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
  showAllSessions = false,
  onToggleShowAllSessions,
  activeSessionId,
  onNewSession,
  onSelectSession,
  onDeleteSession,
  desktopHost,
  runtimeConn,
  streaming = false,
  runtimeSessionEstablished = false,
  apiKeyConfigured,
  activeInspector,
  onInspectorChange,
  collapsed,
  onToggleCollapse,
  officeSession = false,
  checklistActivity = { active: false, pulse: false },
  auditActivity = { active: false, pulse: false },
  taskActivity = { active: false, pulse: false },
  agentActivity = { active: false, pulse: false },
}: Props) {
  const { t } = useT();
  const prefersReducedMotion = usePrefersReducedMotion();
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
        sidebarResizing || prefersReducedMotion ? '' : 'transition-[width] duration-150'
      }`}
      style={{ width: collapsed ? 0 : sidebarWidth }}
      aria-label={t('a11y.sidebarNav')}
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
          <div className="min-w-0">
            <span className="block truncate text-base font-semibold text-accent">{t('app.title')}</span>
            <span className="block truncate text-[10px] leading-tight text-t-text-muted">{t('app.subtitle')}</span>
          </div>
        </div>
      </div>

      <div className="flex flex-col gap-0.5 px-3 py-1" role="navigation" aria-label={t('a11y.sidebarNav')}>
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
            className={`${navBtn(activeInspector === 'checklist')} flex items-center`}
            onClick={() => onInspectorChange('checklist')}
            aria-label={t('sidebar.checklist')}
          >
            <svg viewBox="0 0 24 24" className="inline w-4 h-4 mr-2 stroke-current align-text-bottom shrink-0" style={{ fill: 'none', strokeWidth: 1.6 }}>
              <path d="M9 6h11M9 12h11M9 18h11M5 6h.01M5 12h.01M5 18h.01" strokeLinecap="round" />
            </svg>
            <span className="truncate">{t('sidebar.checklist')}</span>
            <InspectorActivityDot activity={checklistActivity} />
          </button>
        )}
        {!officeSession && (
          <button
            type="button"
            className={`${navBtn(activeInspector === 'audit')} flex items-center`}
            onClick={() => onInspectorChange('audit')}
            aria-label={t('sidebar.audit')}
          >
            <svg
              viewBox="0 0 24 24"
              className="inline w-4 h-4 mr-2 stroke-current align-text-bottom shrink-0"
              style={{ fill: 'none', strokeWidth: 1.6 }}
            >
              <path d="M4 6h16v12H4z M8 6V4h8v2 M9 10h6M9 14h4" strokeLinecap="round" />
            </svg>
            <span className="truncate">{t('sidebar.audit')}</span>
            <InspectorActivityDot activity={auditActivity} />
          </button>
        )}
        <button
          type="button"
          className={navBtn(activeInspector === 'usage')}
          onClick={() => onInspectorChange('usage')}
          aria-label={t('sidebar.usage')}
        >
          <svg
            viewBox="0 0 24 24"
            className="inline w-4 h-4 mr-2 stroke-current align-text-bottom shrink-0"
            style={{ fill: 'none', strokeWidth: 1.6 }}
          >
            <path d="M4 19h16M6 16l3-5 3 3 4-7 4 9" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          {t('sidebar.usage')}
        </button>
        <button
          type="button"
          className={`${navBtn(activeInspector === 'tasks')} flex items-center gap-0`}
          onClick={() => onInspectorChange('tasks')}
          aria-label={t('sidebar.tasks')}
        >
          <svg
            viewBox="0 0 24 24"
            className="inline w-4 h-4 mr-2 stroke-current align-text-bottom shrink-0"
            style={{ fill: 'none', strokeWidth: 1.6 }}
          >
            <path d="M9 6h11M9 12h11M9 18h7M5 6h.01M5 12h.01M5 18h.01" strokeLinecap="round" />
          </svg>
          <span className="truncate">{t('sidebar.tasks')}</span>
          <InspectorActivityDot activity={taskActivity} />
        </button>
        {!officeSession && (
          <button
            type="button"
            className={`${navBtn(activeInspector === 'agents')} flex items-center gap-0`}
            onClick={() => onInspectorChange('agents')}
            aria-label={t('sidebar.agents')}
          >
            <svg
              viewBox="0 0 24 24"
              className="inline w-4 h-4 mr-2 stroke-current align-text-bottom shrink-0"
              style={{ fill: 'none', strokeWidth: 1.6 }}
            >
              <path d="M12 3a4 4 0 0 1 4 4v1h2a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2h2V7a4 4 0 0 1 4-4z" />
            </svg>
            <span className="truncate">{t('sidebar.agents')}</span>
            <InspectorActivityDot activity={agentActivity} />
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
        <div className="flex items-center justify-between gap-1 px-2.5 py-2">
          <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
            {t('common.sessions')}
          </p>
          {onToggleShowAllSessions && (
            <button
              type="button"
              onClick={onToggleShowAllSessions}
              className="text-[10px] text-t-text-muted hover:text-accent transition-colors"
              title={t('sidebar.showAllSessionsHint')}
            >
              {showAllSessions ? '✓ ' : ''}
              {t('sidebar.showAllSessions')}
            </button>
          )}
        </div>
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
                  title={t('sidebar.deleteSessionTitle')}
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
          <p className="px-1 text-[10px] text-amber-text/90 leading-snug">{t('sidebar.apiKeyNotConfigured')}</p>
        )}
        <div
          className="flex items-center gap-2 px-1 py-1 text-xs text-t-text-muted"
          title={t('sidebar.runtimeConnectionTitle')}
        >
          <span
            className={`shrink-0 inline-block w-2 h-2 rounded-full ${runtimeConnIndicatorClass(
              runtimeConn,
              { streaming, sessionEstablished: runtimeSessionEstablished },
            )}`}
          />
          <span className="truncate">
            {runtimeConnStatusLabel(runtimeConn, { streaming, sessionEstablished: runtimeSessionEstablished }, {
              connected: t('common.connectionNormal'),
              disconnected: t('common.connectionDisconnected'),
              busy: t('common.connectionBusy'),
              authMismatch: t('common.runtimeAuthMismatch'),
              checking: t('common.runtimeChecking'),
            })}
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
  | 'skills'
  | 'routing'
  | 'topic-memory'
  | 'system'
  | 'lht-settings'
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
    { tab: 'api-key', label: t('sidebar.apiKey'), show: desktopHost },
    { tab: 'mcp', label: t('panels.mcp'), show: true },
    { tab: 'skills', label: t('sidebar.skills'), show: true },
    { tab: 'routing', label: t('panels.routing'), show: !officeSession },
    { tab: 'topic-memory', label: t('sidebar.topicMemory'), show: !officeSession },
    { tab: 'index', label: t('panels.index'), show: !officeSession },
    { tab: 'system', label: t('panels.system'), show: true },
    { tab: 'lht-settings', label: t('panels.lhtSettings'), show: !officeSession },
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
        {t('sidebar.settings')}
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
