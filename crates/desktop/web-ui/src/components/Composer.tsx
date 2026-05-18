import {
  useState,
  useRef,
  useEffect,
  useLayoutEffect,
  useCallback,
} from 'react';
import { createPortal } from 'react-dom';
import { useT } from '../i18n';
import type {
  DesktopModelId,
  DesktopRouteIntentOption,
  DesktopRunModeId,
  DesktopTaskTypePreference,
  DesktopTaskTypeResolved,
} from '../types/desktop';
import {
  composerRoutingStatusLabel,
  DESKTOP_MODEL_LABELS,
  DESKTOP_MODEL_SHORT_LABELS,
  DESKTOP_RUN_MODE_HINTS,
  DESKTOP_RUN_MODE_LABELS,
  DESKTOP_TASK_TYPE_HINTS,
  DESKTOP_TASK_TYPE_LABELS,
  OFFICE_COMPOSER_RUN_MODE_HINT,
} from '../types/desktop';
import { runModesForSession } from '../lib/taskTypeSession';

const MAX_FILE_BYTES = 128 * 1024; // 128 KB per file
const MAX_IMAGE_BYTES = 20 * 1024 * 1024; // align with describe_image / vision_transcribe_image
const MAX_ATTACHMENTS = 8;
// Image compression before vision bridge: reduces 4K screenshots from ~20MB to ~hundreds of KB.
// Detail: "high" at the vision API means the model still gets adequate resolution after resize.
const COMPRESS_MAX_PX = 1920;
const COMPRESS_QUALITY = 0.85;

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
  /** Image attachments: transcribed via vision bridge before sending to the main model. */
  kind?: 'text' | 'image';
  /** data:image/...;base64,... when `kind === 'image'` and within size limits. */
  imageDataUrl?: string;
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

function isImageFile(file: File): boolean {
  const t = file.type.toLowerCase();
  if (t.startsWith('image/')) return true;
  return /\.(png|jpe?g|gif|webp|bmp)$/i.test(file.name);
}

function readFileAsDataUrl(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ''));
    reader.onerror = () => reject(new Error('read failed'));
    reader.readAsDataURL(file);
  });
}

/** Compress image via Canvas before base64 encoding for the vision bridge.
 *  JPEG output discards alpha (OK for OCR); GIF/BMP are re-encoded.
 *  Falls back to the original file if Canvas API is unavailable or fails.
 *  Typical reduction: 15-20 MB 4K PNG screenshot → 200-800 KB JPEG. */
async function compressImage(
  file: File,
  maxPx = COMPRESS_MAX_PX,
  quality = COMPRESS_QUALITY,
): Promise<Blob | File> {
  try {
    const bmp = await createImageBitmap(file);
    const scale = Math.min(1.0, maxPx / Math.max(bmp.width, bmp.height));
    // Already within size limits AND in a lossy format — skip re-encode.
    if (scale >= 1.0 && (file.type === 'image/jpeg' || file.type === 'image/webp')) {
      bmp.close();
      return file;
    }
    const canvas = document.createElement('canvas');
    canvas.width = Math.round(bmp.width * scale);
    canvas.height = Math.round(bmp.height * scale);
    const ctx = canvas.getContext('2d');
    if (!ctx) {
      bmp.close();
      return file;
    }
    ctx.drawImage(bmp, 0, 0, canvas.width, canvas.height);
    bmp.close();
    return new Promise((resolve) => {
      canvas.toBlob((blob) => resolve(blob ?? file), 'image/jpeg', quality);
    });
  } catch {
    return file; // fallback to original — vision bridge still works, just slower
  }
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

  if (isImageFile(file)) {
    if (size > MAX_IMAGE_BYTES) {
      return {
        name,
        content: '',
        truncated: false,
        size,
        inlined: false,
        kind: 'image',
        omitReason: `图片超过 ${formatSize(MAX_IMAGE_BYTES)}，无法发送`,
      };
    }
    try {
      // Compress to ~0.2-1 MB JPEG before base64 — 4K PNG screenshots
      // are the common case.  Falls back to original on failure.
      const compressed = await compressImage(file);
      const imageDataUrl = await readFileAsDataUrl(compressed);
      return {
        name,
        content: '',
        truncated: false,
        size,
        inlined: false,
        kind: 'image',
        imageDataUrl,
      };
    } catch {
      return {
        name,
        content: '',
        truncated: false,
        size,
        inlined: false,
        kind: 'image',
        omitReason: '读取图片失败',
      };
    }
  }

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
  const fileStack = files.filter((f) => f.kind !== 'image');
  const inlined = fileStack.filter((f) => f.inlined);
  const omitted = fileStack.filter((f) => !f.inlined);

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
      if (f.kind === 'image') {
        if (f.imageDataUrl) {
          return `• ${f.name} · ${sz}（发送前经视觉桥接转写）`;
        }
        return `• ${f.name} · ${sz}（${f.omitReason ?? '无法发送'}）`;
      }
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
  onSend: (payload: ComposerOutboundMessage) => void | Promise<void>;
  onCancel?: () => void;
  disabled: boolean;
  autoApprove: boolean;
  onAutoApproveChange: (value: boolean) => void;
  runMode: DesktopRunModeId;
  onRunModeChange: (mode: DesktopRunModeId) => void;
  taskTypePreference: DesktopTaskTypePreference;
  lockedThreadTaskType: DesktopTaskTypeResolved | null;
  onTaskTypePreferenceChange: (value: DesktopTaskTypePreference) => void;
  /** Read-only; strategy is edited in RoutingPanel. */
  routeIntent: DesktopRouteIntentOption;
  onOpenRouting?: () => void;
  sessionExportEnabled: boolean;
  threadExportEnabled: boolean;
  onExportSessionJson: () => void;
  onExportThreadJson: () => void;
  model: DesktopModelId;
  onModelChange: (model: DesktopModelId) => void;
  workspace: string;
  onWorkspaceChange: (ws: string) => void | Promise<void>;
  /** Session is bound to a restored runtime thread; workspace commits via PATCH when changed */
  resumedThreadActive?: boolean;
  /** Estimated context fill percentage (from transcript + system overhead). */
  contextUsagePct: number;
  contextUsedTokens: number;
  contextWindowTokens: number;
  /** Output tokens from the last completed turn (Claude-style hint). */
  lastTurnOutputTokens?: number | null;
  /** Office task session — hides Plan/Yolo and code-only chrome. */
  officeSession?: boolean;
}

export default function Composer({
  onSend,
  onCancel,
  disabled,
  autoApprove,
  onAutoApproveChange,
  runMode,
  onRunModeChange,
  taskTypePreference,
  lockedThreadTaskType,
  onTaskTypePreferenceChange,
  routeIntent,
  onOpenRouting,
  sessionExportEnabled,
  threadExportEnabled,
  onExportSessionJson,
  onExportThreadJson,
  model,
  onModelChange,
  workspace,
  onWorkspaceChange,
  resumedThreadActive = false,
  contextUsagePct,
  contextUsedTokens,
  contextWindowTokens,
  lastTurnOutputTokens = null,
  officeSession = false,
}: Props) {
  const { t } = useT();
  const [text, setText] = useState('');
  const [attachments, setAttachments] = useState<AttachedFile[]>([]);
  const [transcribing, setTranscribing] = useState(false);
  const [bridgeError, setBridgeError] = useState<string | null>(null);
  const [modelOpen, setModelOpen] = useState(false);
  const [runModeOpen, setRunModeOpen] = useState(false);
  const [taskTypeOpen, setTaskTypeOpen] = useState(false);
  const [moreMenuOpen, setMoreMenuOpen] = useState(false);
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
  const taskTypeMenuRef = useRef<HTMLDivElement>(null);
  const moreMenuRef = useRef<HTMLDivElement>(null);
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
    if (!taskTypeOpen) return;
    const handler = (e: MouseEvent) => {
      if (taskTypeMenuRef.current && !taskTypeMenuRef.current.contains(e.target as Node)) {
        setTaskTypeOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [taskTypeOpen]);

  useEffect(() => {
    if (!moreMenuOpen) return;
    const handler = (e: MouseEvent) => {
      if (moreMenuRef.current && !moreMenuRef.current.contains(e.target as Node)) {
        setMoreMenuOpen(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [moreMenuOpen]);

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
    if (!modelOpen && !workspaceOpen && !runModeOpen && !taskTypeOpen && !moreMenuOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setModelOpen(false);
        setWorkspaceOpen(false);
        setRunModeOpen(false);
        setTaskTypeOpen(false);
        setMoreMenuOpen(false);
      }
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [modelOpen, workspaceOpen, runModeOpen, taskTypeOpen, moreMenuOpen]);

  const handleSend = async () => {
    if ((!text.trim() && attachments.length === 0) || disabled || transcribing) return;

    const badImages = attachments.filter((a) => a.kind === 'image' && !a.imageDataUrl);
    if (badImages.length > 0) {
      setBridgeError(t('composer.badImagesError'));
      return;
    }
    setBridgeError(null);

    const imageAtt = attachments.filter(
      (a): a is AttachedFile & { kind: 'image'; imageDataUrl: string } =>
        a.kind === 'image' && Boolean(a.imageDataUrl),
    );
    const textOnlyAtt = attachments.filter((a) => a.kind !== 'image');

    let apiPrompt = buildApiPrompt(text, textOnlyAtt);

    if (imageAtt.length > 0) {
      setTranscribing(true);
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const preamble =
          '【系统说明】以下是「视觉桥接」模型从你粘贴/附带图片中识别的文字描述；这是消息内嵌内容，不等同于当前工作区内任何路径上的文件。\n请直接据此回答用户提问，不要把工作区文件名与上述插图混为一谈。\n';
        const chunks: string[] = [];
        for (let i = 0; i < imageAtt.length; i++) {
          const transcription = await invoke<string>('vision_transcribe_image', {
            dataUrl: imageAtt[i].imageDataUrl,
          });
          const body = typeof transcription === 'string' ? transcription.trim() : String(transcription).trim();
          if (!body) {
            throw new Error(t('composer.visionBridgeEmpty'));
          }
          chunks.push(`### 附图 ${i + 1}\n\n${body}`);
        }
        const bridge = `${preamble}\n${chunks.join('\n\n---\n\n')}`;
        apiPrompt =
          apiPrompt.trim().length > 0 ? `${bridge}\n\n---\n\n${apiPrompt}` : bridge;
      } catch (err) {
        setBridgeError(err instanceof Error ? err.message : String(err));
        return;
      } finally {
        setTranscribing(false);
      }
    }

    if (!apiPrompt.trim()) return;

    const displayContent =
      buildDisplayContent(text, attachments) +
      (imageAtt.length > 0 ? '\n\n[图片已先经视觉桥接转写并并入提示]' : '');

    await Promise.resolve(onSend({ displayContent, apiPrompt }));
    setText('');
    setAttachments([]);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      void handleSend();
    }
  };

  /** Paste images from clipboard, or smart-paste HTML as plain / code. */
  const handlePaste = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const cd = e.clipboardData;
    const imageItems: DataTransferItem[] = [];
    if (cd?.items) {
      for (let i = 0; i < cd.items.length; i++) {
        const it = cd.items[i];
        if (it.kind === 'file' && it.type.startsWith('image/')) {
          imageItems.push(it);
        }
      }
    }
    if (imageItems.length > 0) {
      e.preventDefault();
      setBridgeError(null);
      void (async () => {
        const newAtts: AttachedFile[] = [];
        for (const it of imageItems) {
          const f = it.getAsFile();
          if (!f) continue;
          newAtts.push(await fileToAttached(f));
        }
        if (newAtts.length === 0) return;
        setAttachments((prev) => [...prev, ...newAtts].slice(0, MAX_ATTACHMENTS));
      })();
      return;
    }

    const html = e.clipboardData.getData('text/html');
    if (html) {
      e.preventDefault();

      // Strip HTML → plain text
      const tmp = document.createElement('div');
      tmp.innerHTML = html;
      let pasted = tmp.textContent || tmp.innerText || '';

      // Guess if it looks like code (contains braces, indentation, semicolons, etc.)
      const looksLikeCode =
        /[{}\[\]()]/.test(pasted) &&
        (pasted.includes('\n') || pasted.includes(';') || /^\s{2,}/m.test(pasted));

      if (looksLikeCode) {
        // Auto-wrap in Markdown code fence
        const lang = guessCodeLanguage(pasted);
        pasted = `\`\`\`${lang}\n${pasted.trim()}\n\`\`\``;
      }

      // Insert at cursor position
      const ta = e.currentTarget;
      const start = ta.selectionStart;
      const end = ta.selectionEnd;
      const before = text.slice(0, start);
      const after = text.slice(end);
      setText(before + pasted + after);

      // Restore cursor after the pasted text
      requestAnimationFrame(() => {
        const newPos = start + pasted.length;
        ta.setSelectionRange(newPos, newPos);
      });
    }
  };

  const selectRunMode = useCallback(
    (m: DesktopRunModeId) => {
      onRunModeChange(m);
      setRunModeOpen(false);
    },
    [onRunModeChange],
  );

  const selectTaskType = useCallback(
    (value: DesktopTaskTypePreference) => {
      onTaskTypePreferenceChange(value);
      setTaskTypeOpen(false);
    },
    [onTaskTypePreferenceChange],
  );

  const taskTypeChipLabel =
    lockedThreadTaskType != null
      ? DESKTOP_TASK_TYPE_LABELS[lockedThreadTaskType]
      : DESKTOP_TASK_TYPE_LABELS[taskTypePreference];
  const taskTypeChipHint =
    lockedThreadTaskType != null
      ? `本会话固定为${DESKTOP_TASK_TYPE_LABELS[lockedThreadTaskType]}模式`
      : DESKTOP_TASK_TYPE_HINTS[taskTypePreference];

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
        title: t('composer.chooseWorkspace'),
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
      setWorkspacePickError(t('composer.pickError'));
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
        aria-label={t('composer.chooseWorkspace')}
        className="fixed z-[10050] w-72 max-h-[min(70vh,calc(100vh-24px))] overflow-y-auto rounded-lg border border-card-border bg-card p-3 shadow-lg ring-1 ring-black/[0.08] dark:ring-white/[0.12]"
        style={{ top: workspacePopoverPos.top, left: workspacePopoverPos.left }}
      >
        <p className="mb-2 text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
          {t('composer.workspaceLabel')}
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
          {isPickingDir ? t('composer.selectingFolder') : t('composer.browseFolder')}
        </button>
        {workspacePickError && (
          <p className="mb-3 text-[11px] leading-snug text-amber-text">{workspacePickError}</p>
        )}
        {resumedThreadActive && (
          <p className="mb-3 text-[11px] leading-snug text-t-text-secondary" dangerouslySetInnerHTML={{ __html: t('composer.threadWorkspaceNotice') }} />
        )}
        <div className="mb-1 text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
          {t('composer.manualPath')}
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
            {t('composer.confirm')}
          </button>
        </div>
      </div>,
      document.body,
    );

  const routingStatus = composerRoutingStatusLabel(routeIntent, runMode);
  const routingActive = routeIntent !== 'off';
  const availableRunModes = runModesForSession(officeSession);
  const runModePickerDisabled = availableRunModes.length <= 1;
  const showAutoApprove = officeSession || runMode === 'agent';
  const ctxPct = contextUsagePct ?? 0;
  const ctxFillClass = ctxPct >= 85 ? 'danger' : ctxPct >= 65 ? 'warn' : '';
  const modelPickerTitle = routingActive
    ? `${DESKTOP_MODEL_LABELS[model]} — ${t('composer.modelFallback')}`
    : DESKTOP_MODEL_LABELS[model];

  return (
    <>
      <div className="shrink-0 border-t border-divider px-4 py-2">
        <div className="mx-auto max-w-3xl">
          <div className="card overflow-visible">
            <div className="flex min-h-10 flex-wrap items-center gap-2 border-b border-divider px-3 py-2 text-xs">
            {showAutoApprove ? (
              <label className="inline-flex cursor-pointer select-none items-center gap-2">
                <input
                  type="checkbox"
                  checked={autoApprove}
                  onChange={(e) => onAutoApproveChange(e.target.checked)}
                  disabled={disabled}
                  className="rounded border-input-border bg-input-bg text-accent focus:ring-accent"
                />
                <span className="hidden sm:inline">{t('composer.autoApprove')}</span>
                <span className="sm:hidden">{t('composer.autoApproveShort')}</span>
              </label>
            ) : (
              <span className="max-w-[14rem] truncate leading-snug text-t-text-muted" title={runMode === 'plan' ? t('composer.planModeHint') : t('composer.yoloModeHint')}>
                {runMode === 'plan'
                  ? t('composer.planModeHint')
                  : t('composer.yoloModeHint')}
              </span>
            )}
            <div className="hidden h-4 w-px shrink-0 bg-divider sm:block" aria-hidden />
            <div className="relative" ref={runModeMenuRef}>
              <button
                type="button"
                disabled={disabled || runModePickerDisabled}
                onClick={() => !runModePickerDisabled && setRunModeOpen((o) => !o)}
                aria-expanded={runModeOpen}
                aria-haspopup={runModePickerDisabled ? undefined : 'listbox'}
                title={
                  officeSession ? OFFICE_COMPOSER_RUN_MODE_HINT : DESKTOP_RUN_MODE_HINTS[runMode]
                }
                className={`composer-chip ${runModeOpen ? 'active' : ''} ${runModePickerDisabled ? 'cursor-default opacity-90' : ''}`}
              >
                {DESKTOP_RUN_MODE_LABELS[runMode]}
                {!runModePickerDisabled && (
                  <svg viewBox="0 0 24 24">
                    <path d="M6 9l6 6 6-6" />
                  </svg>
                )}
              </button>
              {runModeOpen && !runModePickerDisabled && (
                <div
                  className="absolute bottom-full left-0 z-[10040] mb-1 w-[min(100vw-2rem,20rem)] max-w-[320px] rounded-lg border border-card-border bg-card p-1.5 shadow-lg ring-1 ring-black/[0.06] dark:ring-white/[0.08]"
                  role="listbox"
                  aria-label={t('composer.selectMode')}
                >
                  {availableRunModes.map((id) => (
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
            <div className="relative" ref={taskTypeMenuRef}>
              <button
                type="button"
                disabled={disabled}
                onClick={() => setTaskTypeOpen((o) => !o)}
                aria-expanded={taskTypeOpen}
                aria-haspopup="listbox"
                title={taskTypeChipHint}
                className={`composer-chip ${taskTypeOpen ? 'active' : ''}`}
              >
                {taskTypeChipLabel}
                <svg viewBox="0 0 24 24">
                  <path d="M6 9l6 6 6-6" />
                </svg>
              </button>
              {taskTypeOpen && (
                <div
                  className="absolute bottom-full left-0 z-[10040] mb-1 w-[min(100vw-2rem,18rem)] max-w-[288px] rounded-lg border border-card-border bg-card p-1.5 shadow-lg ring-1 ring-black/[0.06] dark:ring-white/[0.08]"
                  role="listbox"
                  aria-label="任务类型"
                >
                  {(['auto', 'office', 'code'] as DesktopTaskTypePreference[]).map((id) => (
                    <button
                      key={id}
                      type="button"
                      role="option"
                      aria-selected={
                        lockedThreadTaskType == null
                          ? id === taskTypePreference
                          : id === lockedThreadTaskType
                      }
                      title={DESKTOP_TASK_TYPE_HINTS[id]}
                      onClick={() => selectTaskType(id)}
                      className={`flex w-full flex-col gap-0.5 rounded-md px-3 py-2 text-left text-sm transition-colors ${
                        (lockedThreadTaskType == null
                          ? id === taskTypePreference
                          : id === lockedThreadTaskType)
                          ? 'bg-accent-soft text-accent'
                          : 'text-t-text hover:bg-hover'
                      }`}
                    >
                      <span className="font-medium">{DESKTOP_TASK_TYPE_LABELS[id]}</span>
                      <span className="text-[11px] leading-snug text-t-text-muted">
                        {DESKTOP_TASK_TYPE_HINTS[id]}
                      </span>
                    </button>
                  ))}
                </div>
              )}
            </div>
            <div className="min-w-[0.5rem] flex-1" />
            {routingStatus && onOpenRouting && !officeSession ? (
              <button
                type="button"
                onClick={onOpenRouting}
                className="composer-chip max-w-[10rem] truncate text-accent"
                title={t('composer.openRouting')}
              >
                {routingStatus}
              </button>
            ) : null}
            <div className="relative" ref={moreMenuRef}>
              <button
                type="button"
                className="composer-icon-btn"
                disabled={disabled}
                onClick={() => setMoreMenuOpen((o) => !o)}
                aria-expanded={moreMenuOpen}
                aria-haspopup="menu"
                title={t('composer.moreMenu')}
              >
                <svg viewBox="0 0 24 24">
                  <circle cx="12" cy="6" r="1.5" fill="currentColor" stroke="none" />
                  <circle cx="12" cy="12" r="1.5" fill="currentColor" stroke="none" />
                  <circle cx="12" cy="18" r="1.5" fill="currentColor" stroke="none" />
                </svg>
              </button>
              {moreMenuOpen && (
                <div
                  className="absolute bottom-full right-0 z-[10040] mb-1 w-52 rounded-lg border border-card-border bg-card p-1 shadow-lg ring-1 ring-black/[0.06] dark:ring-white/[0.08]"
                  role="menu"
                >
                  <button
                    type="button"
                    role="menuitem"
                    disabled={!sessionExportEnabled}
                    onClick={() => {
                      setMoreMenuOpen(false);
                      onExportSessionJson();
                    }}
                    className="flex w-full rounded-md px-3 py-2 text-left text-sm text-t-text hover:bg-hover disabled:opacity-40"
                  >
                    {t('composer.exportSession')}
                  </button>
                  <button
                    type="button"
                    role="menuitem"
                    disabled={!threadExportEnabled}
                    onClick={() => {
                      setMoreMenuOpen(false);
                      onExportThreadJson();
                    }}
                    className="flex w-full rounded-md px-3 py-2 text-left text-sm text-t-text hover:bg-hover disabled:opacity-40"
                  >
                    {t('composer.exportThread')}
                  </button>
                  {onOpenRouting && !officeSession ? (
                    <>
                      <div className="my-1 h-px bg-divider" />
                      <button
                        type="button"
                        role="menuitem"
                        onClick={() => {
                          setMoreMenuOpen(false);
                          onOpenRouting();
                        }}
                        className="flex w-full rounded-md px-3 py-2 text-left text-sm text-t-text hover:bg-hover"
                      >
                        {t('composer.openRouting')}
                      </button>
                    </>
                  ) : null}
                </div>
              )}
            </div>
          </div>
          {bridgeError && (
            <p className="px-3 pt-3 text-xs text-error-text leading-relaxed">{bridgeError}</p>
          )}
          {transcribing && (
            <p className="px-3 pt-3 text-xs text-accent leading-relaxed">{t('composer.transcribing')}</p>
          )}
          {attachments.length > 0 && (
            <div className="flex flex-wrap gap-1.5 px-3 pt-3 pb-0">
              {attachments.map((f, i) => (
                <span
                  key={`${f.name}-${i}`}
                  className="inline-flex items-center gap-1 rounded-md border border-card-border bg-canvas-alt px-2 py-1 text-[11px] text-t-text-secondary"
                  title={`${f.name} · ${formatSize(f.size)}${f.kind === 'image' ? ' · 发送前经视觉桥接' : ''}${!f.inlined && f.kind !== 'image' ? ' · 不按文本嵌入' : ''}${f.truncated ? ' · 已截断至 128 KB（发送模型时）' : ''}${f.omitReason ? `\n${f.omitReason}` : ''}`}
                >
                  {f.kind === 'image' && f.imageDataUrl ? (
                    <img
                      src={f.imageDataUrl}
                      alt=""
                      className="h-7 w-7 shrink-0 rounded border border-card-border object-cover"
                    />
                  ) : (
                    <svg viewBox="0 0 24 24" className="size-3 stroke-current" style={{ fill: 'none', strokeWidth: 1.6 }}>
                      <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z" />
                      <path d="M14 2v6h6M16 13H8M16 17H8M10 9H8" />
                    </svg>
                  )}
                  <span className="max-w-[200px] truncate">{f.name}</span>
                  {f.kind === 'image' && !f.imageDataUrl && (
                    <span className="text-[10px] text-amber-text" title={f.omitReason}>
                      {t('composer.invalid')}
                    </span>
                  )}
                  {f.kind !== 'image' && !f.inlined && (
                    <span className="text-[10px] text-amber-text" title={f.omitReason}>
                      {t('composer.onlyReference')}
                    </span>
                  )}
                  {f.kind !== 'image' && f.inlined && f.truncated && <span className="text-amber-text">⧉</span>}
                  <button
                    type="button"
                    onClick={() => removeAttachment(i)}
                    className="ml-0.5 text-t-text-muted hover:text-t-error"
                    title={t('composer.removeAttachment')}
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
            onChange={(e) => {
              setBridgeError(null);
              setText(e.target.value);
            }}
            onKeyDown={handleKeyDown}
            onPaste={handlePaste}
            aria-label={t('composer.inputMessage')}
            placeholder={t('composer.placeholder')}
            disabled={disabled || transcribing}
            rows={2}
            className="w-full resize-none border-none bg-transparent px-4 py-3 text-sm text-t-text placeholder-t-text-muted focus:outline-none disabled:opacity-50"
            style={{ minHeight: '56px', lineHeight: 1.5 }}
          />
          <div className="flex items-center gap-1.5 border-t border-divider px-2.5 py-2">
            <input
              ref={fileInputRef}
              type="file"
              multiple
              className="hidden"
              onChange={handleFilesSelected}
              accept="image/*,text/*,application/json,application/xml,application/javascript,application/typescript,.rs,.py,.js,.ts,.tsx,.jsx,.css,.html,.json,.xml,.yaml,.yml,.toml,.md,.txt,.csv,.sh,.bash,.ps1,.sql,.env,.cfg,.ini,.conf,.log,.lock,.gradle,.proto,.graphql,.pdf"
            />
            <button
              type="button"
              className="composer-icon-btn"
              title={t('composer.attach')}
              disabled={disabled || transcribing || attachments.length >= MAX_ATTACHMENTS}
              onClick={handleAttachClick}
            >
              <svg viewBox="0 0 24 24">
                <path d="M12 5v14 M5 12h14" />
              </svg>
            </button>
            <div className="relative z-40" ref={workspaceTriggerWrapRef}>
              <button
                type="button"
                className="composer-icon-btn"
                disabled={disabled}
                onClick={() => setWorkspaceOpen((o) => !o)}
                aria-expanded={workspaceOpen}
                aria-haspopup="menu"
                title={workspace}
              >
                <svg viewBox="0 0 24 24">
                  <path d="M4 6h16v12H4z M8 6V4h8v2" />
                </svg>
              </button>
            </div>
            <div className="min-w-[0.5rem] flex-1" />
            <div className="relative" ref={modelMenuRef}>
              <button
                type="button"
                className={`composer-chip ${routingActive ? 'opacity-80' : ''}`}
                disabled={disabled}
                onClick={() => setModelOpen((o) => !o)}
                aria-expanded={modelOpen}
                aria-haspopup="listbox"
                title={modelPickerTitle}
              >
                {DESKTOP_MODEL_SHORT_LABELS[model]}
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
                  aria-label={t('composer.selectModel')}
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
            <div
              className="flex shrink-0 items-center gap-1.5"
              title={t('composer.contextTooltip', {
                pct: ctxPct.toFixed(1),
                used: Math.round(contextUsedTokens).toLocaleString(),
                max: contextWindowTokens.toLocaleString(),
              })}
            >
              <div className="composer-ctx-bar" aria-hidden>
                <div
                  className={`composer-ctx-fill ${ctxFillClass}`}
                  style={{ width: `${Math.min(100, ctxPct)}%` }}
                />
              </div>
              <span className="text-[11px] tabular-nums text-t-text-muted">
                ~{ctxPct.toFixed(1)}%
              </span>
              {lastTurnOutputTokens != null && lastTurnOutputTokens > 0 ? (
                <span
                  className="text-[10px] tabular-nums text-t-text-muted/80"
                  title={t('composer.lastTurnTokensTitle')}
                >
                  {t('composer.lastTurnTokens', { count: lastTurnOutputTokens.toLocaleString() })}
                </span>
              ) : null}
            </div>
            <button
              type="button"
              onClick={() => void handleSend()}
              disabled={disabled || transcribing || (!text.trim() && attachments.length === 0)}
              className="composer-send-pill"
              title={transcribing ? t('composer.transcribing') : t('composer.send')}
            >
              {t('composer.send')}
              <svg viewBox="0 0 24 24">
                <path d="M12 19V5M12 5l-6 6M12 5l6 6" />
              </svg>
            </button>
            {disabled && onCancel ? (
              <button
                type="button"
                onClick={onCancel}
                className="flex-shrink-0 rounded-lg bg-hover-strong px-4 py-2 text-sm font-medium text-t-text transition-colors hover:bg-hover"
              >
                {t('composer.stop')}
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

/** Guess a programming language from paste content for Markdown fence. */
function guessCodeLanguage(code: string): string {
  if (/^#include|^int main|->/.test(code)) return 'cpp';
  if (/^use |^fn |^let |^mut |^impl |^pub /.test(code) || /::/.test(code)) return 'rust';
  if (/^import |^from |^def |^class |^if __name__/.test(code)) return 'python';
  if (/^import |^export |^const |^function |^interface |^type /.test(code) || /=>/.test(code))
    return 'typescript';
  if (/<\/?[a-z]+/.test(code) || /style=/.test(code)) return 'html';
  if (/[{]/.test(code) && /[;]/.test(code) && /console\./.test(code)) return 'javascript';
  if (/^SELECT|^INSERT|^UPDATE|^CREATE TABLE/i.test(code)) return 'sql';
  if (/^#!\/bin\//.test(code) || /^echo /.test(code)) return 'bash';
  if (/^package |^import /.test(code) && /;/.test(code)) return 'java';
  if (/^module |^require /.test(code)) return 'ruby';
  return '';
}