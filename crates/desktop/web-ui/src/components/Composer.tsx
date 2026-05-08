import {
  useState,
  useRef,
  useEffect,
  useLayoutEffect,
  useCallback,
} from 'react';
import { createPortal } from 'react-dom';
import type { DesktopModelId, DesktopRunModeId } from '../types/desktop';
import { DESKTOP_MODEL_LABELS, DESKTOP_RUN_MODE_HINTS, DESKTOP_RUN_MODE_LABELS } from '../types/desktop';

const MAX_FILE_BYTES = 128 * 1024; // 128 KB per file
const MAX_ATTACHMENTS = 8;

export interface ComposerOutboundMessage {
  /** Rendered in the chat transcript (attachment names/summary only — no inlined file bodies). */
  displayContent: string;
  /** Full payload sent to the runtime / model (includes XML excerpts for inlined text attachments). */
  apiPrompt: string;
}

interface AttachedFile {
  name: string;
  /** Decoded UTF-8 text for inlined attachments; empty otherwise. */
  content: string;
  truncated: boolean;
  /** Original upload size before any truncation */
  size: number;
  /** When false, contents are never embedded — binary / unrecognized. */
  inlined: boolean;
  omitReason?: string;
}

function shortenPath(p: string): string {
  if (p === '.' || p === './') return '当前目录';
  const cleaned = p.replace(/\\/g, '/').replace(/\/+$/, '');
  const segments = cleaned.split('/');
  if (segments.length === 1) return segments[0];
  return segments[segments.length - 1];
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function escapeXmlAttr(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');
}

const BINARY_EXT =
  /\.(pdf|png|jpe?g|gif|bmp|webp|ico|tif|tiff|zip|rar|7z|gz|tar|wasm|exe|dll|so|dylib|bin|dmg|apk|woff2?|ttf|otf|eot|mp3|mp4|mpeg|mov|wmv|avi|sqlite3?)$/i;

function sniffBinaryHead(buf: ArrayBuffer): boolean {
  const u = new Uint8Array(buf);
  const n = u.length;
  if (
    n >= 5 &&
    u[0] === 0x25 &&
    u[1] === 0x50 &&
    u[2] === 0x44 &&
    u[3] === 0x46 &&
    u[4] === 0x2d
  ) {
    return true;
  }
  if (n >= 8 && u[0] === 0x89 && u[1] === 0x50 && u[2] === 0x4e && u[3] === 0x47 && u[4] === 0x0d && u[5] === 0x0a && u[6] === 0x1a && u[7] === 0x0a) {
    return true;
  }
  if (n >= 4 && u[0] === 0x42 && u[1] === 0x4d) {
    return true;
  }
  if (
    n >= 4 &&
    u[0] === 0x50 &&
    u[1] === 0x4b &&
    ((u[2] === 0x03 && u[3] === 0x04) || (u[2] === 0x05 && u[3] === 0x06) || (u[2] === 0x07 && u[3] === 0x08))
  ) {
    return true;
  }
  let nuls = 0;
  const scan = Math.min(n, 2048);
  for (let i = 0; i < scan; i++) if (u[i] === 0) nuls++;
  return scan >= 256 && nuls > Math.max(2, scan * 0.002);
}

function mimeImpliesBinary(mime: string): boolean {
  if (!mime.trim()) return false;
  const m = mime.toLowerCase();
  if (/^(image|audio|video)\//.test(m)) return true;
  if (m === 'application/pdf') return true;
  if (m === 'application/zip' || m === 'application/gzip' || m === 'application/x-gzip') return true;
  if (m === 'application/wasm') return true;
  if (m === 'application/octet-stream') return true;
  if (m.startsWith('application/vnd.openxmlformats') || m.startsWith('application/vnd.ms-')) return true;
  return false;
}

async function readHead(file: File, max: number): Promise<ArrayBuffer> {
  const slice = file.slice(0, Math.min(max, file.size));
  return slice.arrayBuffer();
}

async function readFullUtf8(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve((reader.result as string) ?? '');
    reader.onerror = () => reject(new Error('read failed'));
    reader.readAsText(file);
  });
}

async function fileToAttached(file: File): Promise<AttachedFile> {
  const size = file.size;
  const name = file.name;

  if (BINARY_EXT.test(name)) {
    return {
      name,
      content: '',
      truncated: false,
      size,
      inlined: false,
      omitReason: '一般为二进制或未提取格式（如 PDF / Office / 归档）',
    };
  }

  if (mimeImpliesBinary(file.type)) {
    return {
      name,
      content: '',
      truncated: false,
      size,
      inlined: false,
      omitReason: '浏览器 MIME 类型不适合 UTF-8 文本嵌入',
    };
  }

  if (size > 0) {
    const head = await readHead(file, 8192);
    if (sniffBinaryHead(head)) {
      return {
        name,
        content: '',
        truncated: false,
        size,
        inlined: false,
        omitReason: '魔数检测为二进制或含大量不可见字节',
      };
    }
  }

  try {
    const full = await readFullUtf8(file);
    const truncated = full.length > MAX_FILE_BYTES;
    const content = truncated ? full.slice(0, MAX_FILE_BYTES) : full;
    return { name, content, truncated, size, inlined: true };
  } catch {
    return {
      name,
      content: '',
      truncated: false,
      size,
      inlined: false,
      omitReason: '读取失败',
    };
  }
}

/** Close CDATA safely if file content contains the terminator sequence. */
function toCdata(payload: string): string {
  return payload.replace(/\]\]>/g, ']]]]><![CDATA[>');
}

/** Model-facing prompt: user text + note for omitted files + inlined XML excerpts. */
function buildApiPrompt(userText: string, files: AttachedFile[]): string {
  const trimmedUser = userText.trim();
  const inlined = files.filter((f) => f.inlined);
  const omitted = files.filter((f) => !f.inlined);

  const parts: string[] = [];

  if (trimmedUser) {
    parts.push(trimmedUser);
  }

  if (omitted.length > 0) {
    const lines = omitted.map(
      (f) => `- ${f.name}（${formatSize(f.size)}）${f.omitReason ? `：${f.omitReason}` : ''}`,
    );
    parts.push(
      [
        '下列附件未在消息中写入原始二进制内容。请让用户把文件放进当前线程「工作区」后用 read_file 读取，或由用户粘贴可复制纯文本。',
        ...lines,
      ].join('\n'),
    );
  }

  const fileBlocks =
    inlined.length > 0
      ? inlined
          .map((f) => {
            const truncated = f.truncated ? ' truncated="true"' : '';
            const safeName = escapeXmlAttr(f.name);
            return `<file name="${safeName}"${truncated}><![CDATA[${toCdata(f.content)}]]></file>`;
          })
          .join('\n\n')
      : '';

  if (fileBlocks) {
    if (parts.length > 0) {
      parts.push('---', 'Attached files:', fileBlocks);
    } else {
      parts.push('Attached files:', fileBlocks);
    }
  }

  return parts.join('\n\n').trimEnd();
}

/** Transcript-visible text — never exposes raw XML/file bodies from attachments. */
function buildDisplayContent(userText: string, files: AttachedFile[]): string {
  const trimmedUser = userText.trim();
  const lines: string[] = [];
  if (trimmedUser) lines.push(trimmedUser);

  if (files.length > 0) {
    const attLines = files.map((f) => {
      const sz = formatSize(f.size);
      if (!f.inlined) {
        return `• ${f.name} · ${sz}（不会在气泡中展开正文）`;
      }
      return `• ${f.name} · ${sz}${f.truncated ? '（发送至模型时已截断至 128 KB）' : ''}（正文不展示在气泡，仅发往模型）`;
    });
    lines.push(['[附件]', ...attLines].join('\n'));
  }

  return lines.join('\n\n');
}

function firstDirectoryFromPickerResult(selected: unknown): string | null {
  if (selected == null) return null;
  if (typeof selected === 'string' && selected.trim().length > 0) return selected;
  if (Array.isArray(selected) && typeof selected[0] === 'string' && selected[0].trim().length > 0) {
    return selected[0];
  }
  return null;
}

function workspacePickerDefaultPath(cwdHint: string): string | undefined {
  const s = cwdHint.trim();
  if (!s || s === '.' || s === './') return undefined;
  return s;
}

interface Props {
  onSend: (payload: ComposerOutboundMessage) => void;
  onCancel?: () => void;
  disabled: boolean;
  autoApprove: boolean;
  onAutoApproveChange: (value: boolean) => void;
  runMode: DesktopRunModeId;
  onRunModeChange: (mode: DesktopRunModeId) => void;
  model: DesktopModelId;
  onModelChange: (model: DesktopModelId) => void;
  workspace: string;
  onWorkspaceChange: (ws: string) => void | Promise<void>;
  /** Session is bound to a restored runtime thread; workspace commits via PATCH when changed */
  resumedThreadActive?: boolean;
}

export default function Composer({
  onSend,
  onCancel,
  disabled,
  autoApprove,
  onAutoApproveChange,
  runMode,
  onRunModeChange,
  model,
  onModelChange,
  workspace,
  onWorkspaceChange,
  resumedThreadActive = false,
}: Props) {
  const [text, setText] = useState('');
  const [attachments, setAttachments] = useState<AttachedFile[]>([]);
  const [modelOpen, setModelOpen] = useState(false);
  const [runModeOpen, setRunModeOpen] = useState(false);
  const [workspaceOpen, setWorkspaceOpen] = useState(false);
  const [workspaceInput, setWorkspaceInput] = useState(workspace);
  const [isPickingDir, setIsPickingDir] = useState(false);
  const [workspacePickError, setWorkspacePickError] = useState<string | null>(null);
  /** fixed viewport coordinates for workspace popover portal */
  const [workspacePopoverPos, setWorkspacePopoverPos] = useState<{
    top: number;
    left: number;
  }>({ top: 0, left: 8 });
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const modelMenuRef = useRef<HTMLDivElement>(null);
  const runModeMenuRef = useRef<HTMLDivElement>(null);
  const workspaceTriggerWrapRef = useRef<HTMLDivElement>(null);
  const workspacePopoverPanelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
      textareaRef.current.style.height =
        Math.min(textareaRef.current.scrollHeight, 220) + 'px';
    }
  }, [text]);

  useEffect(() => {
    setWorkspaceInput(workspace);
  }, [workspace]);

  useEffect(() => {
    if (workspaceOpen) {
      setWorkspacePickError(null);
    }
  }, [workspaceOpen]);

  const repositionWorkspacePopover = useCallback(() => {
    const trigger = workspaceTriggerWrapRef.current;
    const panel = workspacePopoverPanelRef.current;
    if (!trigger || !panel) return;
    const r = trigger.getBoundingClientRect();
    const margin = 8;
    let top = r.top - panel.offsetHeight - margin;
    if (top < margin) {
      top = Math.min(window.innerHeight - panel.offsetHeight - margin, r.bottom + margin);
    }
    top = Math.max(margin, top);
    let left = r.left;
    const pw = panel.offsetWidth;
    left = Math.max(margin, Math.min(left, window.innerWidth - pw - margin));
    setWorkspacePopoverPos({ top, left });
  }, []);

  useLayoutEffect(() => {
    if (!workspaceOpen) return;
    repositionWorkspacePopover();
  }, [
    workspaceOpen,
    repositionWorkspacePopover,
    resumedThreadActive,
    workspacePickError,
    attachments.length,
  ]);

  useEffect(() => {
    if (!workspaceOpen) return;
    const onResizeScroll = () => repositionWorkspacePopover();
    window.addEventListener('resize', onResizeScroll);
    window.addEventListener('scroll', onResizeScroll, true);
    return () => {
      window.removeEventListener('resize', onResizeScroll);
      window.removeEventListener('scroll', onResizeScroll, true);
    };
  }, [workspaceOpen, repositionWorkspacePopover]);

  useEffect(() => {
    if (!modelOpen) return;
    const handler = (e: MouseEvent) => {
      if (modelMenuRef.current && !modelMenuRef.current.contains(e.target as Node)) {
        setModelOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [modelOpen]);

  useEffect(() => {
    if (!runModeOpen) return;
    const handler = (e: MouseEvent) => {
      if (runModeMenuRef.current && !runModeMenuRef.current.contains(e.target as Node)) {
        setRunModeOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [runModeOpen]);

  useEffect(() => {
    if (!workspaceOpen) return;
    const handler = (e: MouseEvent) => {
      const node = e.target as Node | null;
      if (!node) return;
      const inTrigger = workspaceTriggerWrapRef.current?.contains(node);
      const inPopover = workspacePopoverPanelRef.current?.contains(node);
      if (!inTrigger && !inPopover) {
        setWorkspaceOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [workspaceOpen]);

  useEffect(() => {
    if (!modelOpen && !workspaceOpen && !runModeOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setModelOpen(false);
        setWorkspaceOpen(false);
        setRunModeOpen(false);
      }
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [modelOpen, workspaceOpen, runModeOpen]);

  const handleSend = () => {
    if ((!text.trim() && attachments.length === 0) || disabled) return;
    const displayContent = buildDisplayContent(text, attachments);
    const apiPrompt = buildApiPrompt(text, attachments);
    if (!apiPrompt.trim()) return;
    onSend({ displayContent, apiPrompt });
    setText('');
    setAttachments([]);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const selectRunMode = useCallback(
    (m: DesktopRunModeId) => {
      onRunModeChange(m);
      setRunModeOpen(false);
    },
    [onRunModeChange],
  );

  const selectModel = useCallback(
    (m: DesktopModelId) => {
      onModelChange(m);
      setModelOpen(false);
    },
    [onModelChange],
  );

  const pickDirectory = useCallback(async () => {
    setWorkspacePickError(null);
    setIsPickingDir(true);
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const defaultPath = workspacePickerDefaultPath(workspace);
      const selected = await open({
        directory: true,
        multiple: false,
        title: '选择工作区目录',
        ...(defaultPath ? ({ defaultPath } as Record<string, string>) : {}),
      });
      const dir = firstDirectoryFromPickerResult(selected);
      if (!dir) {
        return;
      }
      try {
        await Promise.resolve(onWorkspaceChange(dir));
        setWorkspaceOpen(false);
      } catch {
        /* Parent shows banner */
      }
    } catch {
      setWorkspacePickError(
        '无法唤起系统文件夹对话框（常见于浏览器或未集成 Tauri）。请直接使用下方输入路径，或在 DS Pick 桌面版中重试。',
      );
    } finally {
      setIsPickingDir(false);
      requestAnimationFrame(() => repositionWorkspacePopover());
    }
  }, [onWorkspaceChange, workspace, repositionWorkspacePopover]);

  const confirmWorkspaceInput = useCallback(async () => {
    const trimmed = workspaceInput.trim();
    if (!trimmed) return;
    try {
      await Promise.resolve(onWorkspaceChange(trimmed));
      setWorkspaceOpen(false);
    } catch {
      setWorkspaceInput(workspace);
    }
  }, [workspaceInput, onWorkspaceChange, workspace]);

  const handleWorkspaceKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      void confirmWorkspaceInput();
    }
  };

  const handleAttachClick = () => {
    fileInputRef.current?.click();
  };

  const handleFilesSelected = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const files = Array.from(e.target.files ?? []);
      if (files.length === 0) return;

      const remaining = MAX_ATTACHMENTS - attachments.length;
      if (remaining <= 0) {
        if (fileInputRef.current) fileInputRef.current.value = '';
        return;
      }

      const toRead = files.slice(0, remaining);
      void (async () => {
        const results: AttachedFile[] = [];
        for (const file of toRead) {
          results.push(await fileToAttached(file));
        }
        setAttachments((prev) => [...prev, ...results].slice(0, MAX_ATTACHMENTS));
        if (fileInputRef.current) fileInputRef.current.value = '';
      })();
    },
    [attachments.length],
  );

  const removeAttachment = useCallback((index: number) => {
    setAttachments((prev) => prev.filter((_, i) => i !== index));
  }, []);

  const workspacePopover =
    workspaceOpen &&
    typeof document !== 'undefined' &&
    createPortal(
      <div
        ref={workspacePopoverPanelRef}
        role="menu"
        aria-label="选择工作区"
        className="fixed z-[10050] w-72 max-h-[min(70vh,calc(100vh-24px))] overflow-y-auto rounded-lg border border-card-border bg-card p-3 shadow-lg ring-1 ring-black/[0.08] dark:ring-white/[0.12]"
        style={{ top: workspacePopoverPos.top, left: workspacePopoverPos.left }}
      >
        <p className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
          工作区目录
        </p>
        <div className="mb-2 rounded-md border border-card-border bg-canvas-alt px-2.5 py-1.5 font-mono text-[11px] text-t-text-secondary break-all">
          {workspace}
        </div>
        <button
          type="button"
          onClick={() => void pickDirectory()}
          disabled={isPickingDir}
          className="mb-3 flex w-full items-center justify-center gap-2 rounded-md border border-accent/30 bg-accent-soft px-2.5 py-2 text-sm font-medium text-accent transition-colors hover:brightness-[1.03] disabled:opacity-60"
        >
          <svg viewBox="0 0 24 24" className="size-4 stroke-current" style={{ fill: 'none', strokeWidth: 1.6 }}>
            <path d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
          </svg>
          {isPickingDir ? '正在打开文件夹对话框…' : '浏览文件夹…'}
        </button>
        {workspacePickError && (
          <p className="mb-3 text-[11px] leading-snug text-amber-text">{workspacePickError}</p>
        )}
        {resumedThreadActive && (
          <p className="mb-3 text-[11px] leading-snug text-t-text-secondary">
            已恢复运行时线程：<code className="font-mono">read_file</code> 使用服务端绑定的<strong>线程工作区</strong>；
            在此修改并经 PATCH 生效后才会切换绑定；回合进行中时服务端可能拒绝更改。
          </p>
        )}
        <div className="mb-1 text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
          或手动输入路径
        </div>
        <div className="flex gap-2">
          <input
            type="text"
            value={workspaceInput}
            onChange={(e) => setWorkspaceInput(e.target.value)}
            onKeyDown={handleWorkspaceKeyDown}
            placeholder="."
            className="flex-1 min-w-0 rounded-md border border-input-border bg-input-bg px-2.5 py-1.5 text-sm text-t-text placeholder:text-t-text-muted focus:border-accent focus:outline-none"
          />
          <button
            type="button"
            onClick={() => void confirmWorkspaceInput()}
            className="shrink-0 rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-accent-text hover:brightness-105"
          >
            确定
          </button>
        </div>
      </div>,
      document.body,
    );

  return (
    <>
      <div className="border-t border-divider px-4 py-3">
        <div className="mx-auto flex max-w-3xl flex-col gap-2">
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-xs text-t-text-muted">
            {runMode === 'agent' ? (
              <label className="inline-flex cursor-pointer select-none items-center gap-2">
                <input
                  type="checkbox"
                  checked={autoApprove}
                  onChange={(e) => onAutoApproveChange(e.target.checked)}
                  disabled={disabled}
                  className="rounded border-input-border bg-input-bg text-accent focus:ring-accent"
                />
                自动批准工具调用
              </label>
            ) : (
              <span className="max-w-xs leading-snug text-t-text-muted">
                {runMode === 'plan'
                  ? 'Plan：只读勘探、关闭 Shell（allow_shell=false），沙箱不向 WorkspaceWrite + 网络 升格。'
                  : 'YOLO：DangerFullAccess + trust_mode + auto_approve。'}
              </span>
            )}
            <div className="hidden h-4 w-px shrink-0 bg-divider sm:block" aria-hidden />
            <div className="relative" ref={runModeMenuRef}>
              <button
                type="button"
                disabled={disabled}
                onClick={() => setRunModeOpen((o) => !o)}
                aria-expanded={runModeOpen}
                aria-haspopup="listbox"
                title={DESKTOP_RUN_MODE_HINTS[runMode]}
                className="pill-btn font-medium text-t-text-secondary"
              >
                模式：<span className="text-accent">{DESKTOP_RUN_MODE_LABELS[runMode]}</span>
                <svg viewBox="0 0 24 24" style={{ width: 12, height: 12 }}>
                  <path d="M6 9l6 6 6-6" />
                </svg>
              </button>
              {runModeOpen && (
                <div
                  className="absolute bottom-full left-0 z-[10040] mb-1 w-[min(100vw-2rem,20rem)] max-w-[320px] rounded-lg border border-card-border bg-card p-1.5 shadow-lg ring-1 ring-black/[0.06] dark:ring-white/[0.08]"
                  role="listbox"
                  aria-label="选择运行模式"
                >
                  {(['plan', 'agent', 'yolo'] as DesktopRunModeId[]).map((id) => (
                    <button
                      key={id}
                      type="button"
                      role="option"
                      aria-selected={id === runMode}
                      title={DESKTOP_RUN_MODE_HINTS[id]}
                      onClick={() => selectRunMode(id)}
                      className={`flex w-full flex-col gap-0.5 rounded-md px-3 py-2 text-left text-sm transition-colors ${
                        id === runMode ? 'bg-accent-soft text-accent' : 'text-t-text hover:bg-hover'
                      }`}
                    >
                      <span className="font-medium">{DESKTOP_RUN_MODE_LABELS[id]}</span>
                      <span className="text-[11px] leading-snug text-t-text-muted">{DESKTOP_RUN_MODE_HINTS[id]}</span>
                    </button>
                  ))}
                </div>
              )}
            </div>
          </div>
        <div className="card overflow-visible">
          {attachments.length > 0 && (
            <div className="flex flex-wrap gap-1.5 px-3 pt-3 pb-0">
              {attachments.map((f, i) => (
                <span
                  key={`${f.name}-${i}`}
                  className="inline-flex items-center gap-1 rounded-md border border-card-border bg-canvas-alt px-2 py-1 text-[11px] text-t-text-secondary"
                  title={`${f.name} · ${formatSize(f.size)}${!f.inlined ? ' · 不按文本嵌入' : ''}${f.truncated ? ' · 已截断至 128 KB（发送模型时）' : ''}${f.omitReason ? `\n${f.omitReason}` : ''}`}
                >
                  <svg viewBox="0 0 24 24" className="size-3 stroke-current" style={{ fill: 'none', strokeWidth: 1.6 }}>
                    <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z" />
                    <path d="M14 2v6h6M16 13H8M16 17H8M10 9H8" />
                  </svg>
                  <span className="max-w-[200px] truncate">{f.name}</span>
                  {!f.inlined && (
                    <span className="text-[10px] text-amber-text" title={f.omitReason}>
                      仅引用
                    </span>
                  )}
                  {f.inlined && f.truncated && <span className="text-amber-text">⧉</span>}
                  <button
                    type="button"
                    onClick={() => removeAttachment(i)}
                    className="ml-0.5 text-t-text-muted hover:text-t-error"
                    title="移除"
                  >
                    ×
                  </button>
                </span>
              ))}
            </div>
          )}
          <textarea
            ref={textareaRef}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="今天需要什么帮助？"
            disabled={disabled}
            rows={2}
            className="w-full resize-none border-none bg-transparent px-4 py-3.5 text-sm text-t-text placeholder-t-text-muted focus:outline-none disabled:opacity-50"
            style={{ minHeight: '64px', lineHeight: 1.5 }}
          />
          <div className="flex items-center gap-2 border-t border-divider px-3 pb-3 pt-0">
            <input
              ref={fileInputRef}
              type="file"
              multiple
              className="hidden"
              onChange={handleFilesSelected}
              accept="text/*,application/json,application/xml,application/javascript,application/typescript,.rs,.py,.js,.ts,.tsx,.jsx,.css,.html,.json,.xml,.yaml,.yml,.toml,.md,.txt,.csv,.sh,.bash,.ps1,.sql,.env,.cfg,.ini,.conf,.log,.lock,.gradle,.proto,.graphql,.pdf"
            />
            <button
              type="button"
              className="pill-btn"
              title="附加文件（文本将嵌入发往模型；PDF/图片等为仅引用）"
              disabled={disabled || attachments.length >= MAX_ATTACHMENTS}
              onClick={handleAttachClick}
            >
              <svg viewBox="0 0 24 24">
                <path d="M12 5v14 M5 12h14" />
              </svg>
            </button>
            <div className="flex-1 min-w-[2rem]" />
            <div className="relative z-40" ref={workspaceTriggerWrapRef}>
              <button
                type="button"
                className="pill-btn"
                disabled={disabled}
                onClick={() => setWorkspaceOpen((o) => !o)}
                aria-expanded={workspaceOpen}
                aria-haspopup="menu"
                title={workspace}
              >
                <svg viewBox="0 0 24 24">
                  <path d="M4 6h16v12H4z M8 6V4h8v2" />
                </svg>
                <span className="max-w-[120px] truncate">{shortenPath(workspace)}</span>
                <svg viewBox="0 0 24 24" style={{ width: 12, height: 12 }}>
                  <path d="M6 9l6 6 6-6" />
                </svg>
              </button>
            </div>
            <div className="relative" ref={modelMenuRef}>
              <button
                type="button"
                className="pill-btn"
                disabled={disabled}
                onClick={() => setModelOpen((o) => !o)}
                aria-expanded={modelOpen}
                aria-haspopup="listbox"
              >
                {DESKTOP_MODEL_LABELS[model]}
                <svg
                  viewBox="0 0 24 24"
                  style={{ width: 12, height: 12 }}
                  className={modelOpen ? 'rotate-180' : ''}
                >
                  <path d="M6 9l6 6 6-6" />
                </svg>
              </button>
              {modelOpen && (
                <div
                  className="absolute bottom-full left-0 z-[10040] mb-1 w-48 rounded-lg border border-card-border bg-card p-1.5 shadow-lg ring-1 ring-black/[0.06] dark:ring-white/[0.08]"
                  role="listbox"
                  aria-label="选择模型"
                >
                  {(Object.entries(DESKTOP_MODEL_LABELS) as [DesktopModelId, string][]).map(
                    ([id, label]) => (
                      <button
                        key={id}
                        type="button"
                        role="option"
                        aria-selected={id === model}
                        onClick={() => selectModel(id)}
                        className={`flex w-full items-center justify-between rounded-md px-3 py-2 text-left text-sm transition-colors ${
                          id === model
                            ? 'bg-accent-soft text-accent font-medium'
                            : 'text-t-text hover:bg-hover'
                        }`}
                      >
                        <span>{label}</span>
                        {id === model && (
                          <svg
                            viewBox="0 0 24 24"
                            style={{
                              width: 14,
                              height: 14,
                              stroke: 'currentColor',
                              fill: 'none',
                              strokeWidth: 2,
                            }}
                          >
                            <path d="M5 13l4 4L19 7" />
                          </svg>
                        )}
                      </button>
                    ),
                  )}
                </div>
              )}
            </div>
            <button
              type="button"
              onClick={handleSend}
              disabled={disabled || (!text.trim() && attachments.length === 0)}
              className="grid h-10 w-10 flex-shrink-0 place-items-center rounded-full bg-accent text-accent-text shadow-md hover:brightness-105 disabled:opacity-40 disabled:shadow-none"
              title="发送"
            >
              <svg
                viewBox="0 0 24 24"
                className="size-[18px]"
                style={{ stroke: 'currentColor', fill: 'none', strokeWidth: 2 }}
              >
                <path d="M12 19V5M12 5l-6 6M12 5l6 6" />
              </svg>
            </button>
            {disabled && onCancel ? (
              <button
                type="button"
                onClick={onCancel}
                className="flex-shrink-0 rounded-lg bg-hover-strong px-4 py-2 text-sm font-medium text-t-text transition-colors hover:bg-hover"
              >
                停止
              </button>
            ) : null}
          </div>
        </div>
      </div>
    </div>
      {workspacePopover}
    </>
  );
}
