import {
  useCallback,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
} from 'react';
import { useT } from '../i18n';
import PanelEdgeSeam from './PanelEdgeSeam';
import ChecklistPanel from './ChecklistPanel';
import AuditScratchpadPanel from './AuditScratchpadPanel';
import LongHorizonPanel from './LongHorizonPanel';
import AgentPanel from './AgentPanel';
import type { AgentState } from '../types/agent';
import type { RuntimeConnectionState } from '../api/client';

const GRID_WIDTH_KEY = 'deepseek-desktop-audit-grid-width';
const GRID_MIN_PX = 400;
const GRID_DEFAULT_PX = 560;

function clampGridWidth(px: number): number {
  if (typeof window === 'undefined') {
    return Math.max(GRID_MIN_PX, Math.round(px));
  }
  const cap = Math.min(1600, Math.floor(window.innerWidth * 0.55));
  return Math.min(cap, Math.max(GRID_MIN_PX, Math.round(px)));
}

function readStoredGridWidth(): number {
  try {
    const n = parseInt(localStorage.getItem(GRID_WIDTH_KEY) ?? '', 10);
    if (Number.isFinite(n)) {
      return clampGridWidth(n);
    }
  } catch {
    /* ignore */
  }
  return GRID_DEFAULT_PX;
}

export interface AuditGridPanelProps {
  workspaceRoot: string;
  resumedThreadId: string;
  streaming: boolean;
  runtimeConn: RuntimeConnectionState;
  runtimeSessionEstablished: boolean;
  agentStates: AgentState[];
  subagentActiveCount: number;
  narrativeSpawnSuspected: boolean;
  openWorkspaceFile: (relPath: string, title?: string) => Promise<void>;
  onDismiss: () => void;
}

function GridCell({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-card">
      <header className="shrink-0 border-b border-divider bg-canvas-alt px-3 py-2">
        <h3 className="truncate text-xs font-semibold tracking-wide text-t-text-muted uppercase">{title}</h3>
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>
    </section>
  );
}

export default function AuditGridPanel({
  workspaceRoot,
  resumedThreadId,
  streaming,
  runtimeConn,
  runtimeSessionEstablished,
  agentStates,
  subagentActiveCount,
  narrativeSpawnSuspected,
  openWorkspaceFile,
  onDismiss,
}: AuditGridPanelProps) {
  const { t } = useT();
  const [panelWidth, setPanelWidth] = useState(readStoredGridWidth);
  const [panelResizing, setPanelResizing] = useState(false);
  const livePanelWidthRef = useRef(panelWidth);
  const resizeDragRef = useRef<{
    pointerId: number;
    startX: number;
    startW: number;
  } | null>(null);

  const endPanelResize = useCallback((e: PointerEvent<HTMLDivElement>) => {
    const d = resizeDragRef.current;
    if (!d || e.pointerId !== d.pointerId) {
      return;
    }
    resizeDragRef.current = null;
    setPanelResizing(false);
    try {
      localStorage.setItem(GRID_WIDTH_KEY, String(livePanelWidthRef.current));
    } catch {
      /* ignore */
    }
    e.currentTarget.releasePointerCapture(e.pointerId);
  }, []);

  const onResizePointerDown = useCallback(
    (e: PointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) {
        return;
      }
      resizeDragRef.current = {
        pointerId: e.pointerId,
        startX: e.clientX,
        startW: panelWidth,
      };
      setPanelResizing(true);
      e.currentTarget.setPointerCapture(e.pointerId);
    },
    [panelWidth],
  );

  const onResizePointerMove = useCallback((e: PointerEvent<HTMLDivElement>) => {
    const d = resizeDragRef.current;
    if (!d || e.pointerId !== d.pointerId) {
      return;
    }
    const next = clampGridWidth(d.startW - (e.clientX - d.startX));
    livePanelWidthRef.current = next;
    setPanelWidth(next);
  }, []);

  const onResizeKeyDown = useCallback((e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 32 : 16;
    if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault();
      const delta = e.key === 'ArrowLeft' ? step : -step;
      setPanelWidth((w) => {
        const n = clampGridWidth(w + delta);
        livePanelWidthRef.current = n;
        try {
          localStorage.setItem(GRID_WIDTH_KEY, String(n));
        } catch {
          /* ignore */
        }
        return n;
      });
    }
  }, []);

  return (
    <div className="flex h-full shrink-0">
      <PanelEdgeSeam
        side="right"
        seamClass="chrome-seam-l"
        resizing={panelResizing}
        ariaResize={t('auditGrid.resizeWidth')}
        collapseTitle={t('auditGrid.hide')}
        onCollapse={onDismiss}
        onPointerDown={onResizePointerDown}
        onPointerMove={onResizePointerMove}
        onPointerUp={endPanelResize}
        onPointerCancel={endPanelResize}
        onKeyDown={onResizeKeyDown}
      />
      <aside
        role="complementary"
        aria-label={t('auditGrid.panelAria')}
        className="flex min-w-0 shrink-0 flex-col overflow-hidden border-t border-divider bg-canvas"
        style={{ width: panelWidth }}
      >
        <div className="grid min-h-0 flex-1 grid-cols-2 grid-rows-2 gap-px bg-divider p-px">
          <GridCell title={t('sidebar.checklist')}>
            <ChecklistPanel threadId={resumedThreadId} pollFast={streaming} />
          </GridCell>
          <GridCell title={t('sidebar.audit')}>
            <AuditScratchpadPanel
              threadId={resumedThreadId}
              workspaceRoot={workspaceRoot}
              pollFast={streaming}
              onOpenWorkspacePath={(rel) => {
                void openWorkspaceFile(rel);
              }}
              subagentActiveCount={subagentActiveCount}
              narrativeSpawnSuspected={narrativeSpawnSuspected}
            />
          </GridCell>
          <GridCell title={t('auditGrid.longHorizon')}>
            <LongHorizonPanel threadId={resumedThreadId} pollFast={streaming} />
          </GridCell>
          <GridCell title={t('sidebar.agents')}>
            <AgentPanel
              agents={agentStates}
              workspaceRoot={workspaceRoot}
              runtimeConn={runtimeConn}
              streaming={streaming}
              runtimeSessionEstablished={runtimeSessionEstablished}
            />
          </GridCell>
        </div>
      </aside>
    </div>
  );
}
