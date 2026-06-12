import { useEffect, useState, useRef, useMemo, useCallback, useLayoutEffect } from 'react';
import { ensureMermaidInitialized, renderMermaidToSvg } from '../lib/mermaidRuntime';
import { mountMermaidSvgInline } from '../lib/mermaidSvgPostProcess';
import { isMermaidSvgThreatError } from '../lib/mermaidSvgSecurity';
import { useT } from '../i18n';

interface Message {
  id: string;
  role: string;
  content: string;
}

interface MermaidBlock {
  digest: string;
  code: string;
  sourceMessageId: string;
}

interface MermaidRenderEntry {
  svg: string;
  error?: string;
  threatBlocked?: { reason: string };
}

interface Props {
  messages: Message[];
  theme: 'light' | 'dark';
  onDetected?: () => void;
}

const ZOOM_MIN = 25;
const ZOOM_MAX = 300;
const ZOOM_STEP = 10;
const ZOOM_DEFAULT = 100;
const RESIZE_DEBOUNCE_MS = 150;

function clampZoom(v: number): number {
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, v));
}

/** Strip Mermaid max-width caps; keep vector crisp (no CSS bitmap scale). */
function prepareMermaidSvg(svgText: string): string {
  let s = svgText.replace(/max-width\s*:\s*[^;]+;?/gi, '');
  const crisp =
    'display:block;max-width:none;shape-rendering:geometricPrecision;text-rendering:geometricPrecision';
  if (!/\bstyle\s*=/i.test(s)) {
    return s.replace(/<svg/i, `<svg style="${crisp}"`);
  }
  return s.replace(/<svg([^>]*)\bstyle\s*=\s*"([^"]*)"/i, (_, attrs, style) => {
    const merged = style.includes('display:') ? style : `${crisp};${style}`;
    return `<svg${attrs}style="${merged.replace(/max-width\s*:[^;]+;?/gi, '')}"`;
  });
}

/**
 * Scale by setting SVG width/height from viewBox — stays vector-sharp.
 * CSS transform:scale() rasterizes first and looks blurry when enlarged.
 */
function applySvgZoom(svgText: string, zoomPct: number): string {
  const { w, h } = parseSvgDimensions(svgText);
  const factor = zoomPct / 100;
  const targetW = Math.max(1, Math.round(w * factor));
  const targetH = Math.max(1, Math.round(h * factor));
  let s = prepareMermaidSvg(svgText);
  s = s.replace(/(<svg[^>]*?)\s+width\s*=\s*"[^"]*"/i, '$1');
  s = s.replace(/(<svg[^>]*?)\s+height\s*=\s*"[^"]*"/i, '$1');
  return s.replace(/<svg/i, `<svg width="${targetW}" height="${targetH}"`);
}

function parseSvgDimensions(svgText: string): { w: number; h: number } {
  const viewBox = svgText.match(/viewBox\s*=\s*"([^"]+)"/i);
  if (viewBox) {
    const p = viewBox[1].trim().split(/[\s,]+/).map(Number);
    if (p.length === 4 && p[2] > 0 && p[3] > 0) {
      return { w: p[2], h: p[3] };
    }
  }
  const wMatch = svgText.match(/\bwidth\s*=\s*"([\d.]+)/i);
  const hMatch = svgText.match(/\bheight\s*=\s*"([\d.]+)/i);
  const w = wMatch ? parseFloat(wMatch[1]) : 0;
  const h = hMatch ? parseFloat(hMatch[1]) : 0;
  if (w > 0 && h > 0) return { w, h };
  return { w: 800, h: 500 };
}

function blockDigest(code: string): string {
  let hash = 0;
  for (let i = 0; i < code.length; i++) {
    const chr = code.charCodeAt(i);
    hash = ((hash << 5) - hash) + chr;
    hash |= 0;
  }
  return `mermaid-${Math.abs(hash).toString(36)}`;
}

function extractMermaidBlocks(messages: Message[]): MermaidBlock[] {
  const blocks: MermaidBlock[] = [];
  for (const msg of messages) {
    if (msg.role !== 'assistant') continue;
    const content = msg.content;
    if (!content) continue;
    const regex = /```mermaid\s*\n?([\s\S]*?)```/g;
    let match;
    while ((match = regex.exec(content)) !== null) {
      const code = match[1].trim();
      if (code) {
        blocks.push({
          digest: blockDigest(code),
          code,
          sourceMessageId: msg.id,
        });
      }
    }
  }
  return blocks;
}

function svgToPngDataUrl(svgText: string, scale: number, isDark: boolean): Promise<string> {
  return new Promise((resolve, reject) => {
    const canvas = document.createElement('canvas');
    const img = new Image();
    const blob = new Blob([svgText], { type: 'image/svg+xml;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    img.onload = () => {
      canvas.width = img.naturalWidth * scale;
      canvas.height = img.naturalHeight * scale;
      const ctx = canvas.getContext('2d');
      if (!ctx) {
        URL.revokeObjectURL(url);
        reject(new Error('Failed to get canvas context'));
        return;
      }
      const bg = isDark ? '#1e1e1e' : '#ffffff';
      ctx.fillStyle = bg;
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
      URL.revokeObjectURL(url);
      resolve(canvas.toDataURL('image/png'));
    };
    img.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error('Failed to load SVG'));
    };
    img.src = url;
  });
}

function triggerDownload(dataUrl: string, filename: string) {
  const a = document.createElement('a');
  a.href = dataUrl;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
}

// ── DiagramViewport — CSS transform zoom + pointer pan ───────────

function DiagramViewport({
  svg,
  zoom,
  digest,
  onZoom,
  viewResetKey,
}: {
  svg: string;
  zoom: number;
  digest: string;
  onZoom: (digest: string, zoom: number) => void;
  viewResetKey: number;
}) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const panRef = useRef({ x: 0, y: 0 });
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const dragRef = useRef<{ px: number; py: number; ox: number; oy: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const fitKeyRef = useRef('');
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const { w: svgW, h: svgH } = useMemo(() => parseSvgDimensions(svg), [svg]);
  const scaledSvg = useMemo(() => applySvgZoom(svg, zoom), [svg, zoom]);

  panRef.current = pan;

  useLayoutEffect(() => {
    const el = contentRef.current;
    if (!el) {
      return;
    }
    mountMermaidSvgInline(el, scaledSvg);
  }, [scaledSvg]);

  const centerAtZoom = useCallback(
    (zoomPct: number) => {
      const el = viewportRef.current;
      if (!el) return;
      const s = zoomPct / 100;
      setPan({
        x: Math.max(8, (el.clientWidth - svgW * s) / 2),
        y: Math.max(8, (el.clientHeight - svgH * s) / 2),
      });
    },
    [svgW, svgH],
  );

  const fitToViewport = useCallback(() => {
    const el = viewportRef.current;
    if (!el || svgW <= 0 || svgH <= 0) return;
    const pad = 32;
    const vw = Math.max(40, el.clientWidth - pad);
    const vh = Math.max(40, el.clientHeight - pad);
    const fitPct = clampZoom(Math.floor(Math.min(vw / svgW, vh / svgH, 1) * 100));
    onZoom(digest, fitPct);
    centerAtZoom(fitPct);
  }, [digest, svgW, svgH, onZoom, centerAtZoom]);

  useEffect(() => {
    fitKeyRef.current = '';
    setPan({ x: 0, y: 0 });
  }, [digest]);

  useLayoutEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const tryFit = () => {
      if (el.clientWidth < 20 || el.clientHeight < 20) return;
      const key = `${digest}:${viewResetKey}`;
      if (fitKeyRef.current === key) return;
      fitKeyRef.current = key;
      // Debounce resize refits to avoid jitter during panel drag
      if (debounceRef.current != null) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        debounceRef.current = null;
        fitToViewport();
      }, RESIZE_DEBOUNCE_MS);
    };
    tryFit();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(tryFit);
    ro.observe(el);
    return () => {
      ro.disconnect();
      if (debounceRef.current != null) {
        clearTimeout(debounceRef.current);
        debounceRef.current = null;
      }
    };
  }, [digest, viewResetKey, svg, fitToViewport]);

  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const delta = e.deltaY > 0 ? -ZOOM_STEP : ZOOM_STEP;
      const newZoom = clampZoom(zoom + delta);
      if (newZoom === zoom) return;
      const rect = el.getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const oldScale = zoom / 100;
      const newScale = newZoom / 100;
      const p = panRef.current;
      setPan({
        x: mx - ((mx - p.x) * newScale) / oldScale,
        y: my - ((my - p.y) * newScale) / oldScale,
      });
      onZoom(digest, newZoom);
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [zoom, digest, onZoom]);

  const onPointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    dragRef.current = { px: e.clientX, py: e.clientY, ox: panRef.current.x, oy: panRef.current.y };
    setDragging(true);
    e.preventDefault();
  }, []);

  const onPointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!dragRef.current) return;
    setPan({
      x: dragRef.current.ox + (e.clientX - dragRef.current.px),
      y: dragRef.current.oy + (e.clientY - dragRef.current.py),
    });
  }, []);

  const endDrag = useCallback(() => {
    dragRef.current = null;
    setDragging(false);
  }, []);

  return (
    <div
      ref={viewportRef}
      className="relative min-h-0 flex-1 touch-none select-none overflow-hidden rounded-b-lg bg-canvas-alt/30"
      style={{ cursor: dragging ? 'grabbing' : 'grab' }}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      role="img"
      aria-label="Mermaid diagram viewport"
    >
      <div
        ref={contentRef}
        className="absolute left-0 top-0 [&_svg]:block"
        style={{
          transform: `translate(${pan.x}px, ${pan.y}px)`,
        }}
      />
    </div>
  );
}

export default function MermaidPanel({ messages, theme, onDetected }: Props) {
  const { t } = useT();
  const [renderMap, setRenderMap] = useState<Record<string, MermaidRenderEntry>>({});
  const [busy, setBusy] = useState(false);
  const firedRef = useRef(false);
  const [zoomMap, setZoomMap] = useState<Record<string, number>>({});
  const [activeTab, setActiveTab] = useState(0);
  const [fullscreenBlock, setFullscreenBlock] = useState<string | null>(null);
  const [viewResetKey, setViewResetKey] = useState(0);
  const [confirmedThreats, setConfirmedThreats] = useState<Set<string>>(() => new Set());
  // Track blocks currently being retried after error
  const [retrying, setRetrying] = useState<Set<string>>(new Set());

  const blocks = useMemo(() => extractMermaidBlocks(messages), [messages]);

  // Clamp activeTab when blocks change
  useEffect(() => {
    if (blocks.length === 0) {
      setActiveTab(0);
    } else if (activeTab >= blocks.length) {
      setActiveTab(blocks.length - 1);
    }
  }, [blocks, activeTab]);

  // Reset when no blocks
  useEffect(() => {
    if (blocks.length === 0) {
      firedRef.current = false;
      setRenderMap({});
      return;
    }
    if (!firedRef.current) {
      firedRef.current = true;
      onDetected?.();
    }
  }, [blocks, onDetected]);

  useEffect(() => {
    ensureMermaidInitialized(theme);
    setRenderMap({});
    setConfirmedThreats(new Set());
  }, [theme]);

  const retryBlock = useCallback(async (digest: string, code: string) => {
    setRetrying((prev) => new Set(prev).add(digest));
    try {
      const svg = await renderMermaidToSvg(code, `mermaid-svg-${digest}`, theme, {
        trust: 'trusted',
      });
      setRenderMap((prev) => {
        const next = { ...prev };
        next[digest] = { svg };
        return next;
      });
      setConfirmedThreats((prev) => {
        if (!prev.has(digest)) {
          return prev;
        }
        const next = new Set(prev);
        next.delete(digest);
        return next;
      });
    } catch (e) {
      if (isMermaidSvgThreatError(e)) {
        setRenderMap((prev) => ({
          ...prev,
          [digest]: { svg: e.svg, threatBlocked: { reason: e.reason } },
        }));
        setConfirmedThreats((prev) => {
          const next = new Set(prev);
          next.delete(digest);
          return next;
        });
      } else {
        setRenderMap((prev) => {
          const next = { ...prev };
          next[digest] = {
            svg: '',
            error: (e as Error).message || String(e),
          };
          return next;
        });
      }
    } finally {
      setRetrying((prev) => {
        const next = new Set(prev);
        next.delete(digest);
        return next;
      });
    }
  }, [theme]);

  const confirmThreatRender = useCallback((digest: string) => {
    setConfirmedThreats((prev) => new Set(prev).add(digest));
  }, []);

  useEffect(() => {
    if (blocks.length === 0) {
      setRenderMap({});
      setBusy(false);
      return;
    }

    let cancelled = false;
    setBusy(true);

    const renderAll = async () => {
      const next: Record<string, MermaidRenderEntry> = {};
      for (const block of blocks) {
        if (cancelled) return;
        const existing = renderMap[block.digest];
        if (existing && !existing.error) {
          next[block.digest] = existing;
          continue;
        }
        try {
          const svg = await renderMermaidToSvg(
            block.code,
            `mermaid-svg-${block.digest}`,
            theme,
            { trust: 'trusted' },
          );
          if (!cancelled) {
            next[block.digest] = { svg };
          }
        } catch (e) {
          if (!cancelled) {
            if (isMermaidSvgThreatError(e)) {
              next[block.digest] = {
                svg: e.svg,
                threatBlocked: { reason: e.reason },
              };
            } else {
              next[block.digest] = {
                svg: '',
                error: (e as Error).message || String(e),
              };
            }
          }
        }
      }
      if (!cancelled) {
        setRenderMap(next);
        setBusy(false);
      }
    };

    renderAll();
    return () => { cancelled = true; };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [blocks, theme]);

  const setZoom = useCallback((digest: string, v: number) => {
    setZoomMap((prev) => ({ ...prev, [digest]: clampZoom(v) }));
  }, []);

  const zoomIn = useCallback((digest: string, current: number) => {
    setZoom(digest, current + ZOOM_STEP);
  }, [setZoom]);

  const zoomOut = useCallback((digest: string, current: number) => {
    setZoom(digest, current - ZOOM_STEP);
  }, [setZoom]);

  const resetZoom = useCallback((digest: string) => {
    setZoomMap((prev) => {
      const next = { ...prev };
      delete next[digest];
      return next;
    });
    setViewResetKey((k) => k + 1);
  }, []);

  const fitToWindow = useCallback(() => {
    setViewResetKey((k) => k + 1);
  }, []);

  const handleExport = useCallback((digest: string, svgText: string, blockIdx: number) => {
    svgToPngDataUrl(svgText, 2, theme === 'dark').then((dataUrl) => {
      const ts = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
      triggerDownload(dataUrl, `mermaid-${blockIdx + 1}-${ts}.png`);
    }).catch((e) => {
      console.error('Export failed:', e);
    });
  }, [theme]);

  // Fullscreen close on Escape
  useEffect(() => {
    if (!fullscreenBlock) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setFullscreenBlock(null);
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [fullscreenBlock]);

  // ── Loading state ────────────────────────────────────────────────

  if (busy && blocks.length > 0 && Object.keys(renderMap).length === 0) {
    return (
      <div className="p-4 text-sm text-t-text-muted flex items-center gap-2">
        <svg className="w-4 h-4 animate-spin text-accent" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <circle cx="12" cy="12" r="10" strokeDasharray="32" />
        </svg>
        {t('mermaid.loadingDiagrams')}
      </div>
    );
  }

  // ── Empty state ──────────────────────────────────────────────────

  if (blocks.length === 0) {
    return (
      <div className="p-4 text-sm text-t-text-muted">
        {t('mermaid.noDiagrams')}
      </div>
    );
  }

  // ── Tab bar ──────────────────────────────────────────────────────

  const safeActive = Math.min(activeTab, blocks.length - 1);
  const activeBlock = blocks[safeActive];
  const activeEntry = renderMap[activeBlock?.digest];
  const activeZoom = zoomMap[activeBlock?.digest] ?? ZOOM_DEFAULT;

  const renderToolbar = (
    digest: string,
    svgText: string,
    idx: number,
    zoom: number,
    isFullscreen: boolean,
  ) => (
    <div className={`flex shrink-0 items-center gap-1 ${isFullscreen ? 'px-3 py-2' : 'px-2 py-1.5'} border-b border-divider`}>
      <span className="text-[10px] text-t-text-muted mr-1 tabular-nums w-8">
        {zoom}%
      </span>
      <button
        type="button"
        className="px-1.5 py-0.5 rounded text-[10px] text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
        onClick={() => zoomOut(digest, zoom)}
        disabled={zoom <= ZOOM_MIN}
        title={t('mermaid.zoomOut')}
      >
        −
      </button>
      <button
        type="button"
        className="px-1.5 py-0.5 rounded text-[10px] text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
        onClick={() => zoomIn(digest, zoom)}
        disabled={zoom >= ZOOM_MAX}
        title={t('mermaid.zoomIn')}
      >
        +
      </button>
      <button
        type="button"
        className="px-1.5 py-0.5 rounded text-[10px] text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
        onClick={() => resetZoom(digest)}
        title={t('mermaid.resetView')}
      >
        ↺
      </button>
      <button
        type="button"
        className="px-1.5 py-0.5 rounded text-[10px] text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
        onClick={fitToWindow}
        title={t('mermaid.fitToWindow')}
      >
        ⊡
      </button>
      <span className="hidden sm:inline text-[10px] text-t-text-muted ml-0.5">
        {t('mermaid.wheelZoomHint')}
      </span>
      <div className="flex-1 min-w-1" />
      {!isFullscreen && (
        <button
          type="button"
          className="px-2 py-0.5 rounded text-[10px] text-t-text-muted hover:text-accent hover:bg-hover transition-colors flex items-center gap-1"
          onClick={() => setFullscreenBlock(digest)}
          title={t('mermaid.fullscreen')}
        >
          <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <polyline points="15,3 21,3 21,9" />
            <polyline points="9,21 3,21 3,15" />
            <line x1="21" y1="3" x2="14" y2="10" />
            <line x1="3" y1="21" x2="10" y2="14" />
          </svg>
          {t('mermaid.fullscreen')}
        </button>
      )}
      <button
        type="button"
        className="px-2 py-0.5 rounded text-[10px] text-t-text-muted hover:text-accent hover:bg-hover transition-colors flex items-center gap-1"
        onClick={() => handleExport(digest, svgText, idx)}
        title={t('mermaid.exportPng')}
      >
        <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
          <polyline points="7,10 12,15 17,10" />
          <line x1="12" y1="15" x2="12" y2="3" />
        </svg>
        {t('mermaid.exportPng')}
      </button>
    </div>
  );

  const renderDiagramError = (entry: MermaidRenderEntry, block: MermaidBlock) => {
    const isRetrying = retrying.has(block.digest);
    return (
      <div className="rounded-lg border border-red-500/30 bg-red-500/10">
        <div className="px-3 py-2 text-xs text-red-300/90 font-medium flex items-center justify-between">
          <span>{t('mermaid.syntaxError')}</span>
          <button
            type="button"
            className="px-2 py-0.5 rounded text-[10px] text-red-300/80 hover:text-red-200 hover:bg-red-500/20 transition-colors disabled:opacity-50"
            disabled={isRetrying}
            onClick={() => retryBlock(block.digest, block.code)}
          >
            {isRetrying ? t('mermaid.retrying') : t('mermaid.retry')}
          </button>
        </div>
        <pre className="px-3 py-2 text-[11px] text-t-text-muted whitespace-pre-wrap max-h-24 overflow-y-auto border-t border-red-500/20">
          {entry.error}
        </pre>
        <details className="px-3 py-2 border-t border-red-500/20">
          <summary className="text-[11px] text-t-text-muted cursor-pointer hover:text-t-text">
            {t('mermaid.viewSource')}
          </summary>
          <pre className="mt-2 text-[11px] text-t-text-muted whitespace-pre-wrap overflow-x-auto max-h-32">
            {block.code}
          </pre>
        </details>
      </div>
    );
  };

  const renderDiagramThreatBlocked = (entry: MermaidRenderEntry, block: MermaidBlock) => {
    const reason = entry.threatBlocked?.reason ?? 'unknown';
    return (
      <div className="rounded-lg border border-amber-500/30 bg-amber-500/10">
        <div className="px-3 py-2 text-xs text-amber-200/90 font-medium">
          {t('mermaid.securityBlocked')}
        </div>
        <p className="px-3 pb-2 text-[11px] text-amber-100/80">
          {t('mermaid.suspiciousContent', { reason })}
        </p>
        <div className="px-3 pb-3 flex gap-2 border-t border-amber-500/20 pt-2">
          <button
            type="button"
            className="px-2 py-0.5 rounded text-[10px] font-medium text-amber-100 border border-amber-500/40 hover:bg-amber-500/20 transition-colors"
            onClick={() => confirmThreatRender(block.digest)}
          >
            {t('mermaid.renderAnyway')}
          </button>
        </div>
        <details className="px-3 py-2 border-t border-amber-500/20">
          <summary className="text-[11px] text-t-text-muted cursor-pointer hover:text-t-text">
            {t('mermaid.viewSource')}
          </summary>
          <pre className="mt-2 text-[11px] text-t-text-muted whitespace-pre-wrap overflow-x-auto max-h-32">
            {block.code}
          </pre>
        </details>
      </div>
    );
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
      {/* Tab bar */}
      {blocks.length > 1 && (
        <div className="flex items-center overflow-x-auto border-b border-divider shrink-0 px-1">
          {blocks.map((block, idx) => {
            const isActive = idx === safeActive;
            const entry = renderMap[block.digest];
            const hasError = entry?.error != null;
            const hasThreat = entry?.threatBlocked != null && !confirmedThreats.has(block.digest);
            const isRendered = entry != null && !hasError && !hasThreat && entry.svg.length > 0;
            return (
              <button
                key={block.digest}
                type="button"
                className={`
                  px-3 py-1.5 text-xs whitespace-nowrap border-b-2 transition-colors
                  ${isActive
                    ? 'border-accent text-t-text font-medium'
                    : 'border-transparent text-t-text-muted hover:text-t-text hover:border-divider'
                  }
                `}
                onClick={() => setActiveTab(idx)}
                title={hasError ? t('mermaid.syntaxError') : hasThreat ? t('mermaid.securityBlocked') : undefined}
              >
                <span className="flex items-center gap-1.5">
                  {!isRendered && !hasError && !hasThreat && (
                    <svg className="w-3 h-3 animate-spin text-t-text-muted" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <circle cx="12" cy="12" r="10" strokeDasharray="32" />
                    </svg>
                  )}
                  {hasError && (
                    <svg className="w-3 h-3 text-red-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <circle cx="12" cy="12" r="10" />
                      <line x1="15" y1="9" x2="9" y2="15" />
                      <line x1="9" y1="9" x2="15" y2="15" />
                    </svg>
                  )}
                  {hasThreat && (
                    <svg className="w-3 h-3 text-amber-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <path d="M12 9v4" />
                      <path d="M12 17h.01" />
                      <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
                    </svg>
                  )}
                  {isRendered && (
                    <svg className="w-3 h-3 text-green-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <polyline points="20,6 9,17 4,12" />
                    </svg>
                  )}
                  {t('mermaid.diagramN', { n: String(idx + 1) })}
                </span>
              </button>
            );
          })}
          <span className="ml-auto mr-2 text-[10px] text-t-text-muted tabular-nums shrink-0">
            {safeActive + 1}/{blocks.length}
          </span>
        </div>
      )}

      {/* Active tab content */}
      {activeBlock && activeEntry && (
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          {activeEntry.threatBlocked && !confirmedThreats.has(activeBlock.digest) ? (
            <div className="p-4 overflow-y-auto">
              {renderDiagramThreatBlocked(activeEntry, activeBlock)}
            </div>
          ) : activeEntry.error ? (
            <div className="p-4 overflow-y-auto">
              {renderDiagramError(activeEntry, activeBlock)}
            </div>
          ) : (
            <>
              {renderToolbar(activeBlock.digest, activeEntry.svg, safeActive, activeZoom, false)}
              <DiagramViewport
                svg={activeEntry.svg}
                zoom={activeZoom}
                digest={activeBlock.digest}
                onZoom={setZoom}
                viewResetKey={viewResetKey}
              />
            </>
          )}
        </div>
      )}

      {/* Waiting for render */}
      {activeBlock && !activeEntry && (
        <div className="p-4">
          <div className="rounded-lg border border-divider p-3 text-xs text-t-text-muted">
            {t('mermaid.rendering')}
          </div>
        </div>
      )}

      {/* Fullscreen modal */}
      {fullscreenBlock && renderMap[fullscreenBlock] && !renderMap[fullscreenBlock]?.error
        && !(renderMap[fullscreenBlock]?.threatBlocked && !confirmedThreats.has(fullscreenBlock)) && (() => {
        const fsEntry = renderMap[fullscreenBlock];
        const fsBlockIdx = blocks.findIndex(b => b.digest === fullscreenBlock);
        const fsZoom = zoomMap[fullscreenBlock] ?? ZOOM_DEFAULT;
        return (
          <div
            className="fixed inset-0 z-[100] flex flex-col"
            style={{ backgroundColor: theme === 'dark' ? 'rgba(0,0,0,0.92)' : 'rgba(255,255,255,0.92)' }}
          >
            {/* Fullscreen toolbar */}
            <div className="flex items-center shrink-0" style={{ backgroundColor: theme === 'dark' ? '#1e1e1e' : '#f5f5f5' }}>
              <span className="ml-3 text-xs text-t-text-muted tabular-nums">
                {t('mermaid.diagramOf', { n: String(fsBlockIdx + 1), total: String(blocks.length) })}
              </span>
              {renderToolbar(fullscreenBlock, fsEntry.svg, fsBlockIdx, fsZoom, true)}
              <button
                type="button"
                className="px-3 py-2 text-xs text-t-text-muted hover:text-t-text hover:bg-hover transition-colors flex items-center gap-1 border-l border-divider"
                onClick={() => setFullscreenBlock(null)}
                title="Esc"
              >
                <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
                {t('mermaid.close')}
              </button>
            </div>
            {/* Fullscreen diagram */}
            <DiagramViewport
              svg={fsEntry.svg}
              zoom={fsZoom}
              digest={fullscreenBlock}
              onZoom={setZoom}
              viewResetKey={viewResetKey}
            />

            {/* Fullscreen prev/next navigation */}
            {blocks.length > 1 && (
              <>
                {(() => {
                  const prevIdx = fsBlockIdx > 0 ? fsBlockIdx - 1 : -1;
                  if (prevIdx >= 0 && renderMap[blocks[prevIdx].digest] && !renderMap[blocks[prevIdx].digest]?.error) {
                    return (
                      <button
                        type="button"
                        className="absolute left-2 top-1/2 -translate-y-1/2 w-10 h-10 rounded-full flex items-center justify-center text-t-text-muted hover:text-t-text hover:bg-hover/30 transition-colors"
                        style={{ backgroundColor: theme === 'dark' ? 'rgba(0,0,0,0.5)' : 'rgba(255,255,255,0.5)' }}
                        onClick={() => setFullscreenBlock(blocks[prevIdx].digest)}
                        title={t('mermaid.diagramN', { n: String(prevIdx + 1) })}
                      >
                        <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <polyline points="15,18 9,12 15,6" />
                        </svg>
                      </button>
                    );
                  }
                  return null;
                })()}
                {(() => {
                  const nextIdx = fsBlockIdx < blocks.length - 1 ? fsBlockIdx + 1 : -1;
                  if (nextIdx >= 0 && renderMap[blocks[nextIdx].digest] && !renderMap[blocks[nextIdx].digest]?.error) {
                    return (
                      <button
                        type="button"
                        className="absolute right-2 top-1/2 -translate-y-1/2 w-10 h-10 rounded-full flex items-center justify-center text-t-text-muted hover:text-t-text hover:bg-hover/30 transition-colors"
                        style={{ backgroundColor: theme === 'dark' ? 'rgba(0,0,0,0.5)' : 'rgba(255,255,255,0.5)' }}
                        onClick={() => setFullscreenBlock(blocks[nextIdx].digest)}
                        title={t('mermaid.diagramN', { n: String(nextIdx + 1) })}
                      >
                        <svg className="w-5 h-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                          <polyline points="9,18 15,12 9,6" />
                        </svg>
                      </button>
                    );
                  }
                  return null;
                })()}
              </>
            )}
          </div>
        );
      })()}
    </div>
  );
}
