import { useEffect, useState, useRef, useMemo, useCallback } from 'react';
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

export default function MermaidPanel({ messages, theme, onDetected }: Props) {
  const [renderMap, setRenderMap] = useState<Record<string, { svg: string; error?: string }>>({});
  const [busy, setBusy] = useState(false);
  const firedRef = useRef(false);
  const [zoomMap, setZoomMap] = useState<Record<string, number>>({});

  const blocks = useMemo(() => extractMermaidBlocks(messages), [messages]);

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

  if (blocks.length === 0) {
    return (
      <div className="p-4 text-sm text-t-text-muted">
        暂未检测到 Mermaid 图表 — 模型尚未输出 mermaid 代码块
      </div>
    );
  }

  return (
    <div className="flex flex-col min-h-0 overflow-y-auto p-4 gap-4">
      {blocks.map((block, idx) => {
        const entry = renderMap[block.digest];
        if (!entry) {
          return (
            <div
              key={block.digest}
              className="rounded-lg border border-divider p-3 text-xs text-t-text-muted"
            >
              渲染中…
            </div>
          );
        }
        if (entry.error) {
          return (
            <div
              key={block.digest}
              className="rounded-lg border border-red-500/30 bg-red-500/10"
            >
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
        }

        const z = zoomMap[block.digest] ?? ZOOM_DEFAULT;

        return (
          <div key={block.digest} className="rounded-lg border border-divider bg-canvas-alt/30">
            <div className="flex items-center gap-1 px-2 py-1.5 border-b border-divider">
              <span className="text-[10px] text-t-text-muted mr-1 tabular-nums w-8">
                {z}%
              </span>
              <button
                type="button"
                className="px-1.5 py-0.5 rounded text-[10px] text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
                onClick={() => zoomOut(block.digest, z)}
                disabled={z <= ZOOM_MIN}
                title="缩小"
              >
                −
              </button>
              <button
                type="button"
                className="px-1.5 py-0.5 rounded text-[10px] text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
                onClick={() => zoomIn(block.digest, z)}
                disabled={z >= ZOOM_MAX}
                title="放大"
              >
                +
              </button>
              <button
                type="button"
                className="px-1.5 py-0.5 rounded text-[10px] text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
                onClick={() => resetZoom(block.digest)}
                disabled={z === ZOOM_DEFAULT}
                title="重置缩放"
              >
                ↺
              </button>
              <div className="flex-1" />
              <button
                type="button"
                className="px-2 py-0.5 rounded text-[10px] text-t-text-muted hover:text-accent hover:bg-hover transition-colors flex items-center gap-1"
                onClick={() => handleExport(block.digest, entry.svg, idx)}
                title="导出为 PNG"
              >
                <svg className="w-3 h-3" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4" />
                  <polyline points="7,10 12,15 17,10" />
                  <line x1="12" y1="15" x2="12" y2="3" />
                </svg>
                导出 PNG
              </button>
            </div>
            <div
              className="overflow-auto p-3"
              style={{ maxHeight: z > 100 ? undefined : '60vh' }}
            >
              <div
                dangerouslySetInnerHTML={{ __html: entry.svg }}
                style={{
                  transform: `scale(${z / 100})`,
                  transformOrigin: 'top left',
                  display: 'inline-block',
                }}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}
