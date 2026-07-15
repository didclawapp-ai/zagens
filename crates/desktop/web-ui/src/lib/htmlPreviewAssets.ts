/**
 * P0b: rewrite relative asset URLs in workspace HTML so srcDoc iframe preview
 * can load CSS / images (and script src for future allow-scripts) via data: URLs.
 */

import { invoke } from '@tauri-apps/api/core';
import {
  readComposerWorkspaceFile,
  readThreadWorkspaceFile,
} from '../api/client';
import { resolveMarkdownLinkToWorkspaceRel } from './resolveMarkdownWorkspaceLink';

/** Tags whose href/src are treated as embeddable preview assets (not navigation). */
const ASSET_OPEN_TAG_RE =
  /<(link|script|img|source|video|audio)\b([^>]*?)(\/?)>/gi;

const ATTR_RE = /\b(href|src|poster)\s*=\s*(["'])([\s\S]*?)\2/gi;

const TEXT_EXTS = new Set([
  '.css',
  '.js',
  '.mjs',
  '.cjs',
  '.svg',
  '.html',
  '.htm',
  '.txt',
  '.json',
  '.map',
]);

const MIME_BY_EXT: Record<string, string> = {
  '.css': 'text/css;charset=utf-8',
  '.js': 'text/javascript;charset=utf-8',
  '.mjs': 'text/javascript;charset=utf-8',
  '.cjs': 'text/javascript;charset=utf-8',
  '.svg': 'image/svg+xml;charset=utf-8',
  '.html': 'text/html;charset=utf-8',
  '.htm': 'text/html;charset=utf-8',
  '.txt': 'text/plain;charset=utf-8',
  '.json': 'application/json;charset=utf-8',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
  '.ico': 'image/x-icon',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
  '.ttf': 'font/ttf',
  '.otf': 'font/otf',
};

export type HtmlAssetRef = {
  /** Original attribute value from the HTML. */
  url: string;
  /** Attribute name (href | src | poster). */
  attr: string;
};

export function extensionOfPath(path: string): string {
  const base = path.split('/').pop() ?? path;
  const q = base.indexOf('?');
  const h = base.indexOf('#');
  let cut = base.length;
  if (q >= 0) cut = Math.min(cut, q);
  if (h >= 0) cut = Math.min(cut, h);
  const name = base.slice(0, cut);
  const dot = name.lastIndexOf('.');
  if (dot <= 0) return '';
  return name.slice(dot).toLowerCase();
}

export function mimeForAssetPath(relPath: string): string {
  const ext = extensionOfPath(relPath);
  return MIME_BY_EXT[ext] ?? 'application/octet-stream';
}

export function isTextAssetPath(relPath: string): boolean {
  return TEXT_EXTS.has(extensionOfPath(relPath));
}

/**
 * Resolve an HTML asset URL relative to the HTML file's workspace path.
 * Returns null for external schemes, escape above root, or empty results.
 */
export function resolveHtmlAssetToWorkspaceRel(
  htmlWorkspaceRel: string | undefined,
  url: string,
): string | null {
  const raw = url.trim();
  if (!raw || raw.startsWith('data:') || raw.startsWith('blob:')) {
    return null;
  }
  return resolveMarkdownLinkToWorkspaceRel(htmlWorkspaceRel, raw);
}

/** Collect embeddable href/src/poster URLs from HTML open tags. */
export function collectHtmlAssetRefs(html: string): HtmlAssetRef[] {
  const out: HtmlAssetRef[] = [];
  const seen = new Set<string>();
  ASSET_OPEN_TAG_RE.lastIndex = 0;
  let tagMatch: RegExpExecArray | null;
  while ((tagMatch = ASSET_OPEN_TAG_RE.exec(html)) !== null) {
    const attrs = tagMatch[2] ?? '';
    ATTR_RE.lastIndex = 0;
    let attrMatch: RegExpExecArray | null;
    while ((attrMatch = ATTR_RE.exec(attrs)) !== null) {
      const attr = (attrMatch[1] ?? '').toLowerCase();
      const url = attrMatch[3] ?? '';
      if (!url || seen.has(url)) continue;
      seen.add(url);
      out.push({ url, attr });
    }
  }
  return out;
}

/**
 * Replace asset attribute values when `url → dataUrl` is present in `dataByUrl`.
 * Leaves unmatched URLs unchanged.
 */
export function rewriteHtmlAssetUrls(
  html: string,
  dataByUrl: ReadonlyMap<string, string>,
): string {
  if (dataByUrl.size === 0) return html;
  return html.replace(
    ASSET_OPEN_TAG_RE,
    (_full, tag: string, attrs: string, slash: string) => {
      const nextAttrs = attrs.replace(
        ATTR_RE,
        (attrFull: string, name: string, quote: string, value: string) => {
          const dataUrl = dataByUrl.get(value);
          if (!dataUrl) return attrFull;
          return `${name}=${quote}${dataUrl}${quote}`;
        },
      );
      return `<${tag}${nextAttrs}${slash}>`;
    },
  );
}

export function textToDataUrl(mime: string, text: string): string {
  // Prefer URL-encoding for CSS/JS so UTF-8 survives without utf-8 base64 pitfalls.
  return `data:${mime},${encodeURIComponent(text)}`;
}

export function base64ToDataUrl(mime: string, base64: string): string {
  return `data:${mime};base64,${base64}`;
}

export type HtmlPreviewAssetReadCtx = {
  workspaceRelPath: string;
  workspaceRoot?: string;
  threadId?: string | null;
  desktopHost?: boolean;
};

type BinaryAssetPayload = {
  mime_type: string;
  base64: string;
  size: number;
  truncated: boolean;
};

async function readTextAsset(
  ctx: HtmlPreviewAssetReadCtx,
  relPath: string,
): Promise<string | null> {
  try {
    if (ctx.threadId) {
      const file = await readThreadWorkspaceFile(ctx.threadId, relPath);
      return file.content;
    }
    const root = ctx.workspaceRoot?.trim();
    if (!root) return null;
    const file = await readComposerWorkspaceFile(root, relPath);
    return file.content;
  } catch {
    return null;
  }
}

async function readBinaryAsset(
  ctx: HtmlPreviewAssetReadCtx,
  relPath: string,
): Promise<BinaryAssetPayload | null> {
  if (!ctx.desktopHost) return null;
  try {
    if (ctx.threadId) {
      return await invoke<BinaryAssetPayload>('read_thread_workspace_binary', {
        threadId: ctx.threadId,
        relativePath: relPath,
      });
    }
    const root = ctx.workspaceRoot?.trim();
    if (!root) return null;
    return await invoke<BinaryAssetPayload>('read_workspace_binary_at_root', {
      workspaceRoot: root,
      relativePath: relPath,
    });
  } catch {
    return null;
  }
}

/**
 * Load relative assets referenced by `html` and return a rewritten document
 * with successful loads inlined as data: URLs. On total failure returns the original html.
 */
export async function loadRewrittenHtmlPreviewDoc(
  html: string,
  ctx: HtmlPreviewAssetReadCtx,
): Promise<string> {
  const refs = collectHtmlAssetRefs(html);
  if (refs.length === 0) return html;

  const dataByUrl = new Map<string, string>();

  await Promise.all(
    refs.map(async (ref) => {
      const rel = resolveHtmlAssetToWorkspaceRel(ctx.workspaceRelPath, ref.url);
      if (!rel) return;

      if (isTextAssetPath(rel)) {
        const text = await readTextAsset(ctx, rel);
        if (text == null) return;
        dataByUrl.set(ref.url, textToDataUrl(mimeForAssetPath(rel), text));
        return;
      }

      const bin = await readBinaryAsset(ctx, rel);
      if (!bin || !bin.base64) return;
      // Prefer sniff mime from shell when present; fall back to extension table.
      const mime =
        bin.mime_type && bin.mime_type !== 'application/octet-stream'
          ? bin.mime_type
          : mimeForAssetPath(rel);
      dataByUrl.set(ref.url, base64ToDataUrl(mime, bin.base64));
    }),
  );

  if (dataByUrl.size === 0) return html;
  return rewriteHtmlAssetUrls(html, dataByUrl);
}
