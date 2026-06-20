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
  ComposerModelId,
  DesktopRouteIntentOption,
  DesktopRunModeId,
  DesktopTaskTypePreference,
  DesktopTaskTypeResolved,
} from '../types/desktop';
import {
  appendWorkspaceMentionToText,
  formatWorkspaceMention,
} from '../lib/composerWorkspaceMention';
import {
  composerModelLabel,
  composerModelShortLabel,
  isPresetComposerModel,
  normalizeComposerModel,
} from '../lib/composerModels';
import { runModesForSession } from '../lib/taskTypeSession';
import { ComposerContextMeter } from './composer/ComposerContextMeter';
import ComposerOverflowMenu from './composer/ComposerOverflowMenu';
import { IconArrowUp } from './icons/FlatIcons';
import { clipboardHtmlToPlainText } from '../lib/sanitizeHtml';
import {
  extractPastedUrl,
  urlAttachmentFromPaste,
} from '../lib/composerUrlAttachment';
import { composerAutoApproveToggleEnabled } from '../lib/approvalPolicy';
import { toast } from '../lib/toast';

const COMPOSER_ERROR_TAG = 'composer-error';
const COMPOSER_TRANSCRIBING_TAG = 'composer-transcribing';

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
  kind?: 'text' | 'image' | 'url';
  /** data:image/...;base64,... when `kind === 'image'` and within size limits. */
  imageDataUrl?: string;
  /** Canonical http(s) URL when `kind === 'url'`. */
  url?: string;
}

function shortenPath(p: string, currentDirLabel: string): string {
  if (p === '.' || p === './') return currentDirLabel;
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

type TranslateFn = (key: string, params?: Record<string, string>) => string;

async function fileToAttached(file: File, t: TranslateFn): Promise<AttachedFile> {
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
        omitReason: t('composer.imageTooLarge', { size: formatSize(MAX_IMAGE_BYTES) }),
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
        omitReason: t('composer.imageReadError'),
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
      omitReason: t('composer.binaryOrUnsupported'),
    };
  }

  if (mimeImpliesBinary(file.type)) {
    return {
      name,
      content: '',
      truncated: false,
      size,
      inlined: false,
      omitReason: t('composer.mimeNotText'),
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
        omitReason: t('composer.magicBinary'),
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
      omitReason: t('composer.readFailed'),
    };
  }
}

/** Close CDATA safely if file content contains the terminator sequence. */
function toCdata(payload: string): string {
  return payload.replace(/\]\]>/g, ']]]]><![CDATA[>');
}

/** Model-facing prompt: user text + note for omitted files + inlined XML excerpts. */
function buildApiPrompt(userText: string, files: AttachedFile[], t: TranslateFn): string {
  const trimmedUser = userText.trim();
  const urlAtts = files.filter((f): f is AttachedFile & { kind: 'url'; url: string } => f.kind === 'url' && Boolean(f.url));
  const fileStack = files.filter((f) => f.kind !== 'image' && f.kind !== 'url');
  const inlined = fileStack.filter((f) => f.inlined);
  const omitted = fileStack.filter((f) => !f.inlined);

  const parts: string[] = [];

  if (trimmedUser) {
    parts.push(trimmedUser);
  }

  if (urlAtts.length > 0) {
    const lines = urlAtts.map((f) => `- ${f.url}`);
    parts.push([t('composerAttachment.urlHeader'), ...lines].join('\n'));
  }

  if (omitted.length > 0) {
    const lines = omitted.map((f) =>
      t('composerAttachment.omittedLine', {
        name: f.name,
        size: formatSize(f.size),
        reason: f.omitReason ? `：${f.omitReason}` : '',
      }),
    );
    parts.push([t('composer.attachmentSummary'), ...lines].join('\n'));
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
function buildDisplayContent(userText: string, files: AttachedFile[], t: TranslateFn): string {
  const trimmedUser = userText.trim();
  const lines: string[] = [];
  if (trimmedUser) lines.push(trimmedUser);

  if (files.length > 0) {
    const attLines = files.map((f) => {
      const sz = formatSize(f.size);
      if (f.kind === 'url') {
        return `• ${f.name}${t('composerAttachment.displayUrl')}`;
      }
      if (f.kind === 'image') {
        if (f.imageDataUrl) {
          return `• ${f.name} · ${sz}${t('composerAttachment.displayImageBridged')}`;
        }
        return `• ${f.name} · ${sz}（${f.omitReason ?? t('composer.cannotSend')}）`;
      }
      if (!f.inlined) {
        return `• ${f.name} · ${sz}${t('composerAttachment.displayNotInlined')}`;
      }
      return `• ${f.name} · ${sz}${f.truncated ? t('composer.truncated') : ''}${t('composerAttachment.displayInlinedModelOnly')}`;
    });
    lines.push([t('composerAttachment.header'), ...attLines].join('\n'));
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
  /** From system settings — non-`auto` policies lock the Composer toggle off. */
  approvalPolicy: string;
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
  onExportTraceReport: () => void;
  onExportTraceCompare: () => void;
  model: ComposerModelId;
  onModelChange: (model: ComposerModelId) => void;
  /** Presets + models from config.toml + current selection. */
  modelOptions: string[];
  /** Opens ModelParamsDialog (temperature / top_p / max_tokens). */
  onOpenModelParams?: () => void;
  workspace: string;
  onWorkspaceChange: (ws: string) => void | Promise<void>;
  /** Session is bound to a restored runtime thread; workspace commits via PATCH when changed */
  resumedThreadActive?: boolean;
  /** Estimated context fill percentage (runtime snapshot or transcript fallback). */
  contextUsagePct: number;
  contextUsedTokens: number;
  contextWindowTokens: number;
  /** `engine` | `store` when runtime `/context` succeeded. */
  contextSource?: string;
  /** Active compaction threshold from runtime (tokens). */
  compactionThresholdTokens?: number;
  /** Last API round `input_tokens` from provider (when available). */
  lastApiInputTokens?: number | null;
  /** Output tokens from the last completed turn (Claude-style hint). */
  lastTurnOutputTokens?: number | null;
  /** Prefix cache hit % from the last completed turn (DeepSeek telemetry). */
  lastCacheHitPercent?: number | null;
  /** Long-horizon harness status chip (nudge / blocked / context warning). */
  lhtChip?: import('../lib/lhtChip').LhtChipState | null;
  /** Office task session — hides Plan/Yolo and code-only chrome. */
  officeSession?: boolean;
  /** Files panel「添加至对话」— bump `nonce` to append `@path` to the input. */
  workspaceMention?: { relPath: string; isDirectory?: boolean; nonce: number };
  /** Backtrack fork — replace composer text when `nonce` bumps. */
  composerPrefill?: { text: string; nonce: number };
}

export default function Composer({
  onSend,
  onCancel,
  disabled,
  autoApprove,
  approvalPolicy,
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
  onExportTraceReport,
  onExportTraceCompare,
  model,
  onModelChange,
  modelOptions,
  onOpenModelParams,
  workspace,
  onWorkspaceChange,
  resumedThreadActive = false,
  contextUsagePct,
  contextUsedTokens,
  contextWindowTokens,
  contextSource,
  compactionThresholdTokens,
  lastApiInputTokens = null,
  lastTurnOutputTokens = null,
  lastCacheHitPercent = null,
  lhtChip = null,
  officeSession = false,
  workspaceMention,
  composerPrefill,
}: Props) {
  const { t } = useT();
  const [text, setText] = useState('');
  const [attachments, setAttachments] = useState<AttachedFile[]>([]);
  const [transcribing, setTranscribing] = useState(false);
  const [modelOpen, setModelOpen] = useState(false);
  const [customModelDraft, setCustomModelDraft] = useState('');
  const [overflowOpen, setOverflowOpen] = useState(false);
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
  const workspaceTriggerWrapRef = useRef<HTMLDivElement>(null);
  const workspacePopoverPanelRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!workspaceMention?.nonce || !workspaceMention.relPath.trim()) return;
    const token = formatWorkspaceMention(
      workspaceMention.relPath,
      Boolean(workspaceMention.isDirectory),
    );
    if (!token) return;
    setText((prev) => appendWorkspaceMentionToText(prev, token));
    requestAnimationFrame(() => {
      const el = textareaRef.current;
      if (!el) return;
      el.focus();
      const len = el.value.length;
      el.setSelectionRange(len, len);
    });
  }, [workspaceMention?.nonce, workspaceMention?.relPath, workspaceMention?.isDirectory]);

  useEffect(() => {
    if (!composerPrefill?.nonce || !composerPrefill.text.trim()) return;
    setText(composerPrefill.text);
    requestAnimationFrame(() => {
      const el = textareaRef.current;
      if (!el) return;
      el.focus();
      const len = el.value.length;
      el.setSelectionRange(len, len);
    });
  }, [composerPrefill?.nonce, composerPrefill?.text]);

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
    if (!modelOpen && !workspaceOpen && !overflowOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setModelOpen(false);
        setWorkspaceOpen(false);
        setOverflowOpen(false);
      }
    };
    document.addEventListener('keydown', handler);
    return () => document.removeEventListener('keydown', handler);
  }, [modelOpen, workspaceOpen, overflowOpen]);

  const handleSend = async () => {
    if ((!text.trim() && attachments.length === 0) || disabled || transcribing) return;

    const badImages = attachments.filter((a) => a.kind === 'image' && !a.imageDataUrl);
    if (badImages.length > 0) {
      toast.error(t('composer.badImagesError'), { tag: COMPOSER_ERROR_TAG });
      return;
    }
    toast.dismissByTag(COMPOSER_ERROR_TAG);

    const imageAtt = attachments.filter(
      (a): a is AttachedFile & { kind: 'image'; imageDataUrl: string } =>
        a.kind === 'image' && Boolean(a.imageDataUrl),
    );
    const textOnlyAtt = attachments.filter((a) => a.kind !== 'image');

    let apiPrompt = buildApiPrompt(text, textOnlyAtt, t);

    if (imageAtt.length > 0) {
      setTranscribing(true);
      toast.info(t('composer.transcribing'), { tag: COMPOSER_TRANSCRIBING_TAG, duration: 0 });
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const preamble = `${t('composerAttachment.visionBridgePreamble')}\n`;
        const chunks: string[] = [];
        for (let i = 0; i < imageAtt.length; i++) {
          const transcription = await invoke<string>('vision_transcribe_image', {
            dataUrl: imageAtt[i].imageDataUrl,
          });
          const body = typeof transcription === 'string' ? transcription.trim() : String(transcription).trim();
          if (!body) {
            throw new Error(t('composer.visionBridgeEmpty'));
          }
          chunks.push(`${t('composerAttachment.visionBridgeImageSection', { index: String(i + 1) })}\n\n${body}`);
        }
        const bridge = `${preamble}\n${chunks.join('\n\n---\n\n')}`;
        apiPrompt =
          apiPrompt.trim().length > 0 ? `${bridge}\n\n---\n\n${apiPrompt}` : bridge;
      } catch (err) {
        toast.error(err instanceof Error ? err.message : String(err), { tag: COMPOSER_ERROR_TAG });
        return;
      } finally {
        setTranscribing(false);
        toast.dismissByTag(COMPOSER_TRANSCRIBING_TAG);
      }
    }

    if (!apiPrompt.trim()) return;

    const displayContent =
      buildDisplayContent(text, attachments, t) +
      (imageAtt.length > 0 ? `\n\n${t('composer.bridgedNotice')}` : '');

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
      toast.dismissByTag(COMPOSER_ERROR_TAG);
      void (async () => {
        const newAtts: AttachedFile[] = [];
        for (const it of imageItems) {
          const f = it.getAsFile();
          if (!f) continue;
          newAtts.push(await fileToAttached(f, t));
        }
        if (newAtts.length === 0) return;
        setAttachments((prev) => [...prev, ...newAtts].slice(0, MAX_ATTACHMENTS));
      })();
      return;
    }

    const plain = cd.getData('text/plain');
    const html = cd.getData('text/html');
    const pastedUrl = extractPastedUrl(plain, html);
    if (pastedUrl) {
      e.preventDefault();
      toast.dismissByTag(COMPOSER_ERROR_TAG);
      addUrlAttachment(pastedUrl);
      return;
    }

    if (html) {
      e.preventDefault();

      let pasted = clipboardHtmlToPlainText(html);

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

  const selectModel = useCallback(
    (m: string) => {
      const normalized = normalizeComposerModel(m);
      if (!normalized) return;
      onModelChange(normalized);
      setCustomModelDraft(normalized);
      setModelOpen(false);
    },
    [onModelChange],
  );

  const applyCustomModel = useCallback(() => {
    selectModel(customModelDraft);
  }, [customModelDraft, selectModel]);

  useEffect(() => {
    if (modelOpen) {
      setCustomModelDraft(model);
    }
  }, [modelOpen, model]);

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
          results.push(await fileToAttached(file, t));
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

  const addUrlAttachment = useCallback((rawUrl: string) => {
    const att = urlAttachmentFromPaste(rawUrl);
    setAttachments((prev) => {
      if (prev.length >= MAX_ATTACHMENTS) return prev;
      if (prev.some((a) => a.kind === 'url' && a.url === att.url)) return prev;
      return [...prev, att];
    });
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
          <p className="mb-3 text-[11px] leading-snug text-t-text-secondary">{t('composer.threadWorkspaceNotice')}</p>
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

  const routingActive = routeIntent !== 'off';
  const availableRunModes = runModesForSession(officeSession);
  const runModePickerDisabled = availableRunModes.length <= 1;
  const showAutoApprove = officeSession || runMode === 'agent';
  const autoApproveToggleEnabled = composerAutoApproveToggleEnabled(approvalPolicy);
  const ctxPct = contextUsagePct ?? 0;
  const ctxFillClass = ctxPct >= 85 ? 'danger' : ctxPct >= 65 ? 'warn' : '';
  const contextTooltipKey =
    contextSource === 'engine' || contextSource === 'store'
      ? 'composer.contextTooltipRuntime'
      : 'composer.contextTooltip';
  const contextTooltipExtra =
    compactionThresholdTokens != null && compactionThresholdTokens > 0
      ? t('composer.contextCompactHint', {
          threshold: compactionThresholdTokens.toLocaleString(),
        })
      : '';
  const lastApiTooltip =
    lastApiInputTokens != null && lastApiInputTokens > 0
      ? t('composer.lastApiInputTokensTitle', {
          count: lastApiInputTokens.toLocaleString(),
        })
      : '';
  const contextMeterTooltip = [
    t(contextTooltipKey, {
      pct: ctxPct.toFixed(1),
      used: Math.round(contextUsedTokens).toLocaleString(),
      max: contextWindowTokens.toLocaleString(),
    }),
    lastApiTooltip,
    lastTurnOutputTokens != null && lastTurnOutputTokens > 0
      ? `${t('composer.lastTurnTokensTitle')}: ${t('composer.lastTurnTokens', {
          count: lastTurnOutputTokens.toLocaleString(),
        })}`
      : '',
    lastCacheHitPercent != null
      ? `${t('composer.lastCacheHitTitle')}: ${lastCacheHitPercent.toFixed(0)}%`
      : '',
    contextTooltipExtra,
  ]
    .filter(Boolean)
    .join('\n');
  const modelPickerTitle = routingActive
    ? `${composerModelLabel(model)} — ${t('composer.modelFallback')}`
    : composerModelLabel(model);
  const sendReady =
    Boolean(text.trim() || attachments.length > 0) && !disabled && !transcribing;

  return (
    <>
      <div className="composer-dock shrink-0 px-4 py-3">
        <div className="mx-auto max-w-3xl">
          <div className="composer-shell flex flex-col overflow-visible">
          {officeSession ? (
            <p className="px-3 pt-2 pb-1 text-[10px] text-t-text-muted border-b border-divider/40">
              {t('composer.officeStatusBar')}
            </p>
          ) : null}
          {attachments.length > 0 && (
            <div className="flex flex-wrap gap-1.5 px-3 pt-3 pb-0">
              {attachments.map((f, i) => (
                <span
                  key={`${f.name}-${i}`}
                  className={`inline-flex items-center gap-1 rounded-md border px-2 py-1 text-[11px] ${
                    f.kind === 'url'
                      ? 'border-accent/30 bg-accent/10 text-accent'
                      : 'border-card-border bg-canvas-alt text-t-text-secondary'
                  }`}
                  title={
                    f.kind === 'url'
                      ? `${f.url ?? f.name}${t('composerAttachment.attachTitleUrl')}`
                      : `${f.name} · ${formatSize(f.size)}${f.kind === 'image' ? t('composerAttachment.attachTitleImageBridged') : ''}${!f.inlined && f.kind !== 'image' ? t('composerAttachment.attachTitleNotEmbedded') : ''}${f.truncated ? t('composerAttachment.attachTitleTruncated') : ''}${f.omitReason ? `\n${f.omitReason}` : ''}`
                  }
                >
                  {f.kind === 'image' && f.imageDataUrl ? (
                    <img
                      src={f.imageDataUrl}
                      alt=""
                      className="h-7 w-7 shrink-0 rounded border border-card-border object-cover"
                    />
                  ) : f.kind === 'url' ? (
                    <svg viewBox="0 0 24 24" className="size-3 shrink-0 stroke-current" style={{ fill: 'none', strokeWidth: 1.8 }}>
                      <path d="M10 13a5 5 0 007.54.54l3-3a5 5 0 00-7.07-7.07l-1.72 1.71" strokeLinecap="round" />
                      <path d="M14 11a5 5 0 00-7.54-.54l-3 3a5 5 0 007.07 7.07l1.71-1.71" strokeLinecap="round" />
                    </svg>
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
                  {f.kind !== 'image' && f.kind !== 'url' && !f.inlined && (
                    <span className="text-[10px] text-amber-text" title={f.omitReason}>
                      {t('composer.onlyReference')}
                    </span>
                  )}
                  {f.kind !== 'image' && f.kind !== 'url' && f.inlined && f.truncated && (
                    <span className="text-amber-text">⧉</span>
                  )}
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
            id="composer-input"
            ref={textareaRef}
            value={text}
            onChange={(e) => {
              toast.dismissByTag(COMPOSER_ERROR_TAG);
              setText(e.target.value);
            }}
            onKeyDown={handleKeyDown}
            onPaste={handlePaste}
            aria-label={t('composer.inputMessage')}
            aria-keyshortcuts="Enter Send Shift+Enter Newline"
            placeholder={t('composer.placeholder')}
            disabled={disabled || transcribing}
            rows={2}
            className="composer-input w-full resize-none border-none bg-transparent px-4 py-3 text-[15px] text-t-text placeholder-t-text-muted focus:outline-none disabled:opacity-50"
            style={{ minHeight: '52px', lineHeight: 1.55 }}
          />
          <div
            className="flex items-center gap-1.5 bg-canvas-alt/30 px-2.5 py-2"
            role="toolbar"
            aria-label={t('a11y.composerActionsToolbar')}
          >
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
              aria-label={t('composer.attach')}
              disabled={disabled || transcribing || attachments.length >= MAX_ATTACHMENTS}
              onClick={handleAttachClick}
            >
              <svg viewBox="0 0 24 24" aria-hidden>
                <path d="M21.44 11.05l-9.19 9.19a6 6 0 01-8.49-8.49l9.19-9.19a4 4 0 015.66 5.66l-9.2 9.19a2 2 0 01-2.83-2.83l8.49-8.48" />
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
            <ComposerOverflowMenu
              open={overflowOpen}
              onOpenChange={setOverflowOpen}
              disabled={disabled}
              officeSession={Boolean(officeSession)}
              showAutoApprove={showAutoApprove}
              autoApprove={autoApprove}
              autoApproveToggleEnabled={autoApproveToggleEnabled}
              approvalPolicy={approvalPolicy}
              onAutoApproveChange={onAutoApproveChange}
              runMode={runMode}
              availableRunModes={availableRunModes}
              runModePickerDisabled={runModePickerDisabled}
              onRunModeChange={onRunModeChange}
              taskTypePreference={taskTypePreference}
              lockedThreadTaskType={lockedThreadTaskType}
              onTaskTypePreferenceChange={onTaskTypePreferenceChange}
              lhtChip={lhtChip}
              sessionExportEnabled={sessionExportEnabled}
              threadExportEnabled={threadExportEnabled}
              onExportSessionJson={onExportSessionJson}
              onExportThreadJson={onExportThreadJson}
              onExportTraceReport={onExportTraceReport}
              onExportTraceCompare={onExportTraceCompare}
              onOpenRouting={onOpenRouting}
            />
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
                {composerModelShortLabel(model)}
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
                  className="absolute bottom-full left-0 z-[10040] mb-1 w-72 max-w-[min(18rem,calc(100vw-2rem))] rounded-lg border border-card-border bg-card p-1.5 shadow-lg ring-1 ring-black/[0.06] dark:ring-white/[0.08]"
                  role="listbox"
                  aria-label={t('composer.selectModel')}
                >
                  {modelOptions.map((id) => (
                    <button
                      key={id}
                      type="button"
                      role="option"
                      aria-selected={id === model}
                      onClick={() => selectModel(id)}
                      className={`flex w-full items-center justify-between gap-2 rounded-md px-3 py-2 text-left text-sm transition-colors ${
                        id === model
                          ? 'bg-accent-soft text-accent font-medium'
                          : 'text-t-text hover:bg-hover'
                      }`}
                    >
                      <span className="min-w-0 truncate" title={id}>
                        {composerModelLabel(id)}
                        {!isPresetComposerModel(id) ? (
                          <span className="ml-1 text-[10px] font-normal text-t-text-muted">
                            ({t('composer.modelFromConfig')})
                          </span>
                        ) : null}
                      </span>
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
                  ))}
                  <div className="mt-1 border-t border-divider pt-1.5 px-1">
                    <label className="block text-[10px] font-medium text-t-text-muted mb-1">
                      {t('composer.customModel')}
                    </label>
                    <div className="flex gap-1">
                      <input
                        type="text"
                        value={customModelDraft}
                        onChange={(e) => setCustomModelDraft(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            applyCustomModel();
                          }
                        }}
                        placeholder={t('composer.customModelPlaceholder')}
                        className="min-w-0 flex-1 rounded-md border border-input-border bg-input-bg px-2 py-1.5 text-xs text-t-text outline-none focus:border-accent"
                      />
                      <button
                        type="button"
                        onClick={applyCustomModel}
                        className="shrink-0 rounded-md bg-accent px-2 py-1.5 text-xs font-medium text-accent-text hover:opacity-90"
                      >
                        {t('composer.customModelApply')}
                      </button>
                    </div>
                  </div>
                </div>
              )}
            </div>
            {onOpenModelParams ? (
              <button
                type="button"
                className="composer-chip shrink-0 px-2"
                disabled={disabled}
                onClick={onOpenModelParams}
                title={t('sidebar.modelParams')}
                aria-label={t('sidebar.modelParams')}
              >
                <svg viewBox="0 0 24 24" style={{ width: 14, height: 14 }} aria-hidden>
                  <path
                    fill="currentColor"
                    d="M12 8a4 4 0 1 1 0 8 4 4 0 0 1 0-8m8.94 5A8.994 8.994 0 0 0 13 20.94V22h-2v-1.06A8.994 8.994 0 0 0 3.06 13H2v-2h1.06A8.994 8.994 0 0 0 11 3.06V2h2v1.06A8.994 8.994 0 0 0 20.94 11H22v2h-1.06Z"
                  />
                </svg>
              </button>
            ) : null}
            <ComposerContextMeter
              percent={ctxPct}
              level={ctxFillClass}
              tooltip={contextMeterTooltip}
              ariaLabel={t('composer.contextMeterAria', { pct: String(Math.round(ctxPct)) })}
            />
            <button
              type="button"
              onClick={() => void handleSend()}
              disabled={disabled || transcribing || (!text.trim() && attachments.length === 0)}
              className={`composer-send-btn${sendReady ? ' composer-send-btn--ready' : ''}`}
              title={transcribing ? t('composer.transcribing') : t('composer.send')}
              aria-label={transcribing ? t('composer.transcribing') : t('composer.sendAria')}
            >
              <IconArrowUp />
            </button>
            {disabled && onCancel ? (
              <button
                type="button"
                onClick={onCancel}
                className="composer-icon-btn composer-stop-btn"
                aria-label={t('composer.stopAria')}
                title={t('composer.stop')}
              >
                <svg viewBox="0 0 24 24" aria-hidden>
                  <rect x="7" y="7" width="10" height="10" rx="1.5" fill="currentColor" stroke="none" />
                </svg>
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