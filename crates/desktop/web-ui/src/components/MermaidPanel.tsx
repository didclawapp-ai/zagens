import { useEffect, useState, useRef, useMemo, useCallback, useLayoutEffect } from 'react';
import mermaid from 'mermaid';

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

interface Props {
  messages: Message[];
  theme: 'light' | 'dark';
  onDetected?: () => void;
}

const ZOOM_MIN = 25;
const ZOOM_MAX = 300;
const ZOOM_STEP = 10;
const ZOOM_DEFAULT = 100;

function clampZoom(v: number): number {
  return Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, v));
}

/**
 * SVG-native scaling: modify width/height attrs so the browser rasterises
 * at the target resolution instead of bitmap-scaling via CSS transform.
 * Also strips max-width constraints that would defeat the scaling.
 */
function scaleSvg(svgText: string, scale: number): string {
  if (scale === 1) return svgText;
  let result = svgText;
  // Scale width + height attributes (supports optional unit suffix like "px")
  result = result.replace(
    /(<svg[^>]*?)\bwidth\s*=\s*"([\d.]+)([^"]*?)"([^>]*?)\bheight\s*=\s*"([\d.]+)([^"]*?)"/i,
    (_, before, w, wUnit, mid, h, hUnit) => {
      const newW = parseFloat(w) * scale;
      const newH = parseFloat(h) * scale;
      return `${before}width="${newW}${wUnit}"${mid}height="${newH}${hUnit}"`;
    },
  );
  // Remove max-width constraint (Mermaid often adds max-width:XXXpx in style)
  result = result.replace(/max-width\s*:\s*[\d.]+\s*px\s*;?/gi, '');
  return result;
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
    const regex = /```mermaid\n([\s\S]*?)```/g;
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

const MERMAID_THEME_DEFAULT = 'default' as const;
const MERMAID_THEME_DARK = 'dark' as const;

// ── DiagramCanvas — wheel zoom + left-drag pan ──────────────────

function DiagramCanvas({
  svg,
  zoom,
  digest,
  onZoom,
  isFullscreen,
}: {
  svg: string;
  zoom: number;
  digest: string;
  onZoom: (digest: string, zoom: number) => void;
  isFullscreen: boolean;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const dragState = useRef<{ sx: number; sy: number; sl: number; st: number } | null>(null);
  const [grabbing, setGrabbing] = useState(false);
  const pendingScroll = useRef<{ left: number; top: number } | null>(null);

  // Apply pending scroll after zoom-triggered re-render (synchronous, before paint)
  useLayoutEffect(() => {
    if (pendingScroll.current && containerRef.current) {
      containerRef.current.scrollLeft = pendingScroll.current.left;
      containerRef.current.scrollTop = pendingScroll.current.top;
      pendingScroll.current = null;
    }
  });

  // Attach wheel listener with { passive: false } so we can preventDefault
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const delta = e.deltaY > 0 ? -ZOOM_STEP : ZOOM_STEP;
      const newZoom = clampZoom(zoom + delta);
      if (newZoom === zoom) return;
      const rect = el.getBoundingClientRect();
      const mx = e.clientX - rect.left;
      const my = e.clientY - rect.top;
      const ratio = newZoom / zoom;
      pendingScroll.current = {
        left: (mx + el.scrollLeft) * ratio - mx,
        top: (my + el.scrollTop) * ratio - my,
      };
      onZoom(digest, newZoom);
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [zoom, digest, onZoom]);

  // Drag-to-pan: mousedown starts tracking, mousemove adjusts scroll
  useEffect(() => {
    if (!grabbing) return;
    const onMove = (e: MouseEvent) => {
      if (!dragState.current || !containerRef.current) return;
      const dx = dragState.current.sx - e.clientX;
      const dy = dragState.current.sy - e.clientY;
      containerRef.current.scrollLeft = dragState.current.sl + dx;
      containerRef.current.scrollTop = dragState.current.st + dy;
    };
    const onUp = () => {
      dragState.current = null;
      setGrabbing(false);
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
    return () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
    };
  }, [grabbing]);

  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) return;
    const el = containerRef.current;
    if (!el) return;
    dragState.current = {
      sx: e.clientX,
      sy: e.clientY,
      sl: el.scrollLeft,
      st: el.scrollTop,
    };
    setGrabbing(true);
    e.preventDefault();
  }, []);

  const scaledSvg = useMemo(() => scaleSvg(svg, zoom / 100), [svg, zoom]);

  return (
    <div
      ref={containerRef}
      className="overflow-auto p-3 select-none"
      style={{
        cursor: grabbing ? 'grabbing' : 'grab',
        flex: 1,
        minHeight: 0,
      }}
      onMouseDown={handleMouseDown}
    >
      <div
        className="flex items-center justify-center"
        style={{ minWidth: '100%', minHeight: '100%' }}
      >
        <div
          dangerouslySetInnerHTML={{ __html: scaledSvg }}
          style={{ display: 'inline-block' }}
        />
      </div>
    </div>
  );
}

export default function MermaidPanel({ messages, theme, onDetected }: Props) {
  const [renderMap, setRenderMap] = useState<Record<string, { svg: string; error?: string }>>({});
  const [busy, setBusy] = useState(false);
  const firedRef = useRef(false);
  const [zoomMap, setZoomMap] = useState<Record<string, number>>({});
  const [activeTab, setActiveTab] = useState(0);
  const [fullscreenBlock, setFullscreenBlock] = useState<string | null>(null);

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
    mermaid.initialize({
      startOnLoad: false,
      theme: theme === 'dark' ? MERMAID_THEME_DARK : MERMAID_THEME_DEFAULT,
      securityLevel: 'strict',
    });
  }, [theme]);

  useEffect(() => {
    if (blocks.length === 0) {
      setRenderMap({});
      setBusy(false);
      return;
    }

    let cancelled = false;
    setBusy(true);

    const renderAll = async () => {
      const next: Record<string, { svg: string; error?: string }> = {};
      for (const block of blocks) {
        if (cancelled) return;
        const existing = renderMap[block.digest];
        if (existing && !existing.error) {
          next[block.digest] = existing;
          continue;
        }
        try {
          const { svg } = await mermaid.render(
            `mermaid-svg-${block.digest}`,
            block.code,
          );
          if (!cancelled) {
            next[block.digest] = { svg };
          }
        } catch (e) {
          if (!cancelled) {
            next[block.digest] = {
              svg: '',
              error: (e as Error).message || String(e),
            };
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
  }, [blocks]);

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
        渲染图表中…
      </div>
    );
  }

  // ── Empty state ──────────────────────────────────────────────────

  if (blocks.length === 0) {
    return (
      <div className="p-4 text-sm text-t-text-muted">
        暂未检测到 Mermaid 图表 — 模型尚未输出 mermaid 代码块
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
    <div className={`flex items-center gap-1 ${isFullscreen ? 'px-3 py-2' : 'px-2 py-1.5'} border-b border-divider`}>
      <span className="text-[10px] text-t-text-muted mr-1 tabular-nums w-8">
        {zoom}%
      </span>
      <button
        type="button"
        className="px-1.5 py-0.5 rounded text-[10px] text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
        onClick={() => zoomOut(digest, zoom)}
        disabled={zoom <= ZOOM_MIN}
        title="缩小"
      >
        −
      </button>
      <button
        type="button"
        className="px-1.5 py-0.5 rounded text-[10px] text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
        onClick={() => zoomIn(digest, zoom)}
        disabled={zoom >= ZOOM_MAX}
        title="放大"
      >
        +
      </button>
      <button
        type="button"
        className="px-1.5 py-0.5 rounded text-[10px] text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
        onClick={() => resetZoom(digest)}
        disabled={zoom === ZOOM_DEFAULT}
        title="重置缩放"
      >
        ↺
      </button>
      <div className="flex-1" />
      {!isFullscreen && (
        <button
          type="button"
          className="px-2 py-0.5 rounded text-[10px] text-t-text-muted hover:text-accent hover:bg-hover transition-colors flex items-center gap-1"
          onClick={() => setFullscreenBlock(digest)}
          title="全屏查看"
        >
          <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <polyline points="15,3 21,3 21,9" />
            <polyline points="9,21 3,21 3,15" />
            <line x1="21" y1="3" x2="14" y2="10" />
            <line x1="3" y1="21" x2="10" y2="14" />
          </svg>
          全屏
        </button>
      )}
      <button
        type="button"
        className="px-2 py-0.5 rounded text-[10px] text-t-text-muted hover:text-accent hover:bg-hover transition-colors flex items-center gap-1"
        onClick={() => handleExport(digest, svgText, idx)}
        title="导出为 PNG"
      >
        <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
          <polyline points="7,10 12,15 17,10" />
          <line x1="12" y1="15" x2="12" y2="3" />
        </svg>
        导出
      </button>
    </div>
  );

  const renderDiagramError = (entry: { svg: string; error?: string }, block: MermaidBlock) => (
    <div className="rounded-lg border border-red-500/30 bg-red-500/10">
      <div className="px-3 py-2 text-xs text-red-300/90 font-medium">
        Mermaid 语法错误
      </div>
      <pre className="px-3 py-2 text-[11px] text-t-text-muted whitespace-pre-wrap max-h-24 overflow-y-auto border-t border-red-500/20">
        {entry.error}
      </pre>
      <details className="px-3 py-2 border-t border-red-500/20">
        <summary className="text-[11px] text-t-text-muted cursor-pointer hover:text-t-text">
          查看原始代码
        </summary>
        <pre className="mt-2 text-[11px] text-t-text-muted whitespace-pre-wrap overflow-x-auto max-h-32">
          {block.code}
        </pre>
      </details>
    </div>
  );

  return (
    <div className="flex flex-col min-h-0">
      {/* Tab bar */}
      {blocks.length > 1 && (
        <div className="flex items-center overflow-x-auto border-b border-divider shrink-0 px-1">
          {blocks.map((block, idx) => {
            const isActive = idx === safeActive;
            const hasError = renderMap[block.digest]?.error != null;
            const isRendered = renderMap[block.digest] != null && !hasError;
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
                title={hasError ? '渲染错误' : undefined}
              >
                <span className="flex items-center gap-1.5">
                  {!isRendered && !hasError && (
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
                  {isRendered && (
                    <svg className="w-3 h-3 text-green-400" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <polyline points="20,6 9,17 4,12" />
                    </svg>
                  )}
                  图表 {idx + 1}
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
        <div className="flex flex-col min-h-0">
          {activeEntry.error ? (
            <div className="p-4">
              {renderDiagramError(activeEntry, activeBlock)}
            </div>
          ) : (
            <>
              {renderToolbar(activeBlock.digest, activeEntry.svg, safeActive, activeZoom, false)}
              <DiagramCanvas
                svg={activeEntry.svg}
                zoom={activeZoom}
                digest={activeBlock.digest}
                onZoom={setZoom}
                isFullscreen={false}
              />
            </>
          )}
        </div>
      )}

      {/* Waiting for render */}
      {activeBlock && !activeEntry && (
        <div className="p-4">
          <div className="rounded-lg border border-divider p-3 text-xs text-t-text-muted">
            渲染中…
          </div>
        </div>
      )}

      {/* Fullscreen modal */}
      {fullscreenBlock && renderMap[fullscreenBlock] && !renderMap[fullscreenBlock]?.error && (() => {
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
                图表 {fsBlockIdx + 1} / {blocks.length}
              </span>
              {renderToolbar(fullscreenBlock, fsEntry.svg, fsBlockIdx, fsZoom, true)}
              <button
                type="button"
                className="px-3 py-2 text-xs text-t-text-muted hover:text-t-text hover:bg-hover transition-colors flex items-center gap-1 border-l border-divider"
                onClick={() => setFullscreenBlock(null)}
                title="退出全屏 (Esc)"
              >
                <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
                关闭
              </button>
            </div>
            {/* Fullscreen diagram */}
            <DiagramCanvas
              svg={fsEntry.svg}
              zoom={fsZoom}
              digest={fullscreenBlock}
              onZoom={setZoom}
              isFullscreen={true}
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
                        title={`图表 ${prevIdx + 1}`}
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
                        title={`图表 ${nextIdx + 1}`}
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
