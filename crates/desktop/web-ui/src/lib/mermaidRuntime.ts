import mermaid from 'mermaid';
import { sanitizeMermaidSvg } from './sanitizeHtml';
import { mountMermaidSvgIframe, patchMermaidSvgForWebView2 } from './mermaidSvgPostProcess';
import {
  assertMermaidSvgSafe,
  isMermaidSvgThreatError,
  type MermaidSvgThreatError,
} from './mermaidSvgSecurity';
import type { Theme } from './appPreferences';

/** Bumped when `mermaid.initialize` options change (forces re-init after app update). */
const MERMAID_INIT_REV = '18';

let configuredInitKey: string | null = null;

/** Labels passed by callers so preview + panel rendering share translated strings. */
export interface MermaidLabels {
  rendering: string;
  renderError: (message: string) => string;
  retry: string;
  retrying: string;
  securityBlocked: string;
  suspiciousContent: (reason: string) => string;
  renderAnyway: string;
}

const DEFAULT_LABELS: MermaidLabels = {
  rendering: 'Rendering…',
  renderError: (msg: string) => `Mermaid render failed: ${msg}`,
  retry: 'Retry',
  retrying: '…',
  securityBlocked: 'Preview blocked',
  suspiciousContent: (reason: string) =>
    `Suspicious content detected in diagram (${reason}). Preview was blocked.`,
  renderAnyway: 'Preview anyway',
};

/** Show the "rendering…" placeholder only if the render takes longer than the flash threshold. */
export const RENDER_FLASH_THRESHOLD_MS = 50;

/** Bump when SVG post-process changes so cached blocks re-render after app update. */
const MERMAID_RENDER_VERSION = MERMAID_INIT_REV;

export type MermaidTrustMode = 'trusted' | 'sanitized';

export interface RenderMermaidOptions {
  /** Workspace preview / Mermaid panel — skip DOMPurify (Cursor-like). */
  trust?: MermaidTrustMode;
  /** After user confirms on a security block. */
  bypassThreatScan?: boolean;
}

/** Keep a single Mermaid config in sync with app light/dark theme. */
export function ensureMermaidInitialized(theme: Theme): void {
  const initKey = `${theme}:${MERMAID_INIT_REV}`;
  if (configuredInitKey === initKey) {
    return;
  }
  mermaid.initialize({
    startOnLoad: false,
    theme: theme === 'dark' ? 'dark' : 'default',
    // Trusted paths skip DOMPurify; loose avoids Mermaid's internal second sanitize pass.
    securityLevel: 'loose',
    // Match Cursor/GitHub: HTML labels in foreignObject (layout correct). Black-box guard in post-process.
    htmlLabels: true,
  });
  configuredInitKey = initKey;
}

/** Reset cached theme after tests or hot reload (optional). */
export function resetMermaidRuntimeForTests(): void {
  configuredInitKey = null;
}

/** SVG-native labels use `\n`; docs often use `<br/>` for Cursor/GitHub previews. */
export function normalizeMermaidSourceForSvgLabels(code: string): string {
  return code.replace(/<br\s*\/?>/gi, '\n');
}

export function blockDigest(code: string): string {
  let hash = 0;
  for (let i = 0; i < code.length; i++) {
    hash = ((hash << 5) - hash) + code.charCodeAt(i);
    hash |= 0;
  }
  return Math.abs(hash).toString(36);
}

function resolveMermaidLabels(labels?: Partial<MermaidLabels>): MermaidLabels {
  return { ...DEFAULT_LABELS, ...labels };
}

function finalizeTrustedSvg(svg: string, options: RenderMermaidOptions): string {
  const patched = patchMermaidSvgForWebView2(svg);
  if (!options.bypassThreatScan) {
    assertMermaidSvgSafe(patched);
  }
  return patched;
}

export async function renderMermaidToSvg(
  code: string,
  renderId: string,
  theme: Theme,
  options: RenderMermaidOptions = {},
): Promise<string> {
  const trust = options.trust ?? 'trusted';
  ensureMermaidInitialized(theme);
  const normalized = normalizeMermaidSourceForSvgLabels(code);
  const { svg } = await mermaid.render(renderId, normalized);
  if (trust === 'sanitized') {
    return sanitizeMermaidSvg(svg);
  }
  return finalizeTrustedSvg(svg, options);
}

function clearMermaidMount(mount: HTMLElement): void {
  mount.innerHTML = '';
  mount.textContent = '';
  mount.classList.remove('text-xs', 'text-red-400', 'text-amber-300', 'p-3');
  mount.classList.add('text-xs', 'text-t-text-muted');
}

function mountMermaidError(
  block: HTMLElement,
  mount: HTMLElement,
  message: string,
  labels: MermaidLabels,
  onRetry: () => void,
): void {
  mount.innerHTML = '';
  mount.classList.remove('text-t-text-muted');
  mount.classList.add('text-xs', 'text-red-400', 'p-3', 'space-y-2');

  const msgEl = document.createElement('p');
  msgEl.textContent = labels.renderError(message);
  mount.appendChild(msgEl);

  const btn = document.createElement('button');
  btn.type = 'button';
  btn.className =
    'rounded-md border border-red-500/40 px-2 py-1 text-[11px] font-medium ' +
    'text-red-300 hover:bg-red-500/10';
  btn.textContent = labels.retry;
  btn.addEventListener('click', () => {
    delete block.dataset.mermaidRendered;
    delete block.dataset.mermaidTheme;
    btn.disabled = true;
    btn.textContent = labels.retrying;
    onRetry();
  });
  mount.appendChild(btn);
  block.dataset.mermaidRendered = 'error';
}

function mountMermaidThreatBlocked(
  block: HTMLElement,
  mount: HTMLElement,
  err: MermaidSvgThreatError,
  labels: MermaidLabels,
  onRenderAnyway: () => void,
): void {
  mount.innerHTML = '';
  mount.classList.remove('text-t-text-muted');
  mount.classList.add('text-xs', 'text-amber-300', 'p-3', 'space-y-2');

  const title = document.createElement('p');
  title.className = 'font-medium text-amber-200/90';
  title.textContent = labels.securityBlocked;
  mount.appendChild(title);

  const msgEl = document.createElement('p');
  msgEl.textContent = labels.suspiciousContent(err.reason);
  mount.appendChild(msgEl);

  const btn = document.createElement('button');
  btn.type = 'button';
  btn.className =
    'rounded-md border border-amber-500/40 px-2 py-1 text-[11px] font-medium ' +
    'text-amber-200 hover:bg-amber-500/10';
  btn.textContent = labels.renderAnyway;
  btn.addEventListener('click', () => {
    delete block.dataset.mermaidRendered;
    delete block.dataset.mermaidTheme;
    btn.disabled = true;
    btn.textContent = labels.retrying;
    onRenderAnyway();
  });
  mount.appendChild(btn);
  block.dataset.mermaidRendered = 'blocked';
}

export interface RenderMermaidBlockOptions {
  bypassThreatScan?: boolean;
}

/** Render one `.ds-mermaid-block` placeholder (Markdown file preview). */
export async function renderMermaidBlock(
  block: HTMLElement,
  blockIndex: number,
  theme: Theme,
  labels?: Partial<MermaidLabels>,
  options?: RenderMermaidBlockOptions,
): Promise<void> {
  const L = resolveMermaidLabels(labels);
  const source = block.querySelector('.ds-mermaid-source')?.textContent?.trim();
  const mount = block.querySelector<HTMLElement>('.ds-mermaid-mount');
  if (!source || !mount) {
    return;
  }

  if (block.dataset.mermaidVersion !== MERMAID_RENDER_VERSION) {
    delete block.dataset.mermaidRendered;
    delete block.dataset.mermaidTheme;
  }

  if (
    block.dataset.mermaidRendered === '1'
    && block.dataset.mermaidTheme === theme
  ) {
    return;
  }

  const renderId = `md-mermaid-${blockDigest(source)}-${blockIndex}`;
  clearMermaidMount(mount);

  let flashTimer: ReturnType<typeof setTimeout> | null = setTimeout(() => {
    flashTimer = null;
    if (block.dataset.mermaidRendered !== '1') {
      mount.textContent = L.rendering;
    }
  }, RENDER_FLASH_THRESHOLD_MS);

  const retry = () => {
    void renderMermaidBlock(block, blockIndex, theme, labels, options);
  };

  const renderAnyway = () => {
    void renderMermaidBlock(block, blockIndex, theme, labels, { bypassThreatScan: true });
  };

  try {
    const svg = await renderMermaidToSvg(source, renderId, theme, {
      trust: 'trusted',
      bypassThreatScan: options?.bypassThreatScan,
    });
    if (flashTimer != null) {
      clearTimeout(flashTimer);
      flashTimer = null;
    }
    mountMermaidSvgIframe(mount, svg);
    block.dataset.mermaidRendered = '1';
    block.dataset.mermaidTheme = theme;
    block.dataset.mermaidVersion = MERMAID_RENDER_VERSION;
    mount.classList.remove('text-xs', 'text-t-text-muted');
  } catch (e) {
    if (flashTimer != null) {
      clearTimeout(flashTimer);
      flashTimer = null;
    }
    if (isMermaidSvgThreatError(e)) {
      mountMermaidThreatBlocked(block, mount, e, L, renderAnyway);
      return;
    }
    const msg = (e as Error).message || String(e);
    mountMermaidError(block, mount, msg, L, retry);
  }
}

/** Find `.ds-mermaid-block` placeholders and render diagrams inline. */
export async function renderMermaidBlocksInContainer(
  container: HTMLElement,
  theme: Theme,
  labels?: Partial<MermaidLabels>,
): Promise<void> {
  const blocks = container.querySelectorAll<HTMLElement>('.ds-mermaid-block');
  for (let i = 0; i < blocks.length; i++) {
    await renderMermaidBlock(blocks[i], i, theme, labels);
  }
}
