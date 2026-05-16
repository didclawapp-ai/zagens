// ---------------------------------------------------------------------------
// MarkdownRenderer — full markdown-it pipeline with DOMPurify sanitization.
// Extracted from the original `MarkdownPreview.tsx` (which now re-exports).
// ---------------------------------------------------------------------------

import { useCallback, useEffect, useRef, useState, type MouseEvent } from 'react';
import MarkdownIt from 'markdown-it';
import DOMPurify from 'dompurify';
import { resolveMarkdownLinkToWorkspaceRel } from '../../../lib/resolveMarkdownWorkspaceLink';
import hljs from 'highlight.js/lib/core';
import plaintext from 'highlight.js/lib/languages/plaintext';
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import rust from 'highlight.js/lib/languages/rust';
import bash from 'highlight.js/lib/languages/bash';
import json from 'highlight.js/lib/languages/json';
import markdown from 'highlight.js/lib/languages/markdown';

import 'highlight.js/styles/github.css';

import type { RendererProps } from '../types';

// ---- hljs setup (minimal — code blocks inside markdown) --------------------

hljs.registerLanguage('plaintext', plaintext);
hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('js', javascript);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('ts', typescript);
hljs.registerLanguage('tsx', typescript);
hljs.registerLanguage('jsx', javascript);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('rs', rust);
hljs.registerLanguage('bash', bash);
hljs.registerLanguage('sh', bash);
hljs.registerLanguage('shell', bash);
hljs.registerLanguage('zsh', bash);
hljs.registerLanguage('json', json);
hljs.registerLanguage('markdown', markdown);
hljs.registerLanguage('md', markdown);

// ---- helpers ---------------------------------------------------------------

function escapeFallback(str: string): string {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function highlightFence(raw: string, langRaw?: string): string {
  const key = langRaw?.trim().split(/\s+/)[0]?.toLowerCase() ?? '';
  if (key && hljs.getLanguage(key)) {
    try {
      return hljs.highlight(raw, { language: key }).value;
    } catch {
      /* fall through */
    }
  }
  try {
    return hljs.highlight(raw, { language: 'plaintext', ignoreIllegals: true })
      .value;
  } catch {
    return escapeFallback(raw);
  }
}

// ---- markdown-it instance --------------------------------------------------

const md = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: true,
  highlight(str: string, langRaw: string): string {
    return highlightFence(str, langRaw ?? '');
  },
});

/** Relative paths in Markdown links; blocks javascript:/data: and keeps repo-relative hrefs. */
const URI_ALLOW =
  /^(?:(?:https?|ftp|mailto|tel):|(?![a-z][a-z0-9+.-]*:)(?:[\w./-]+))$/i;

// ---- copy-button injection ------------------------------------------------

/** Clip-path checkmark SVG (16×16) shown for ~1.5 s after a successful copy. */
const CHECK_SVG =
  '<svg class="w-4 h-4" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2">' +
  '<path d="M2 8l4 4 8-8" stroke-linecap="round" stroke-linejoin="round"/>' +
  '</svg>';

/** Clipboard outline SVG (16×16) — shown when the button is idle. */
const COPY_SVG =
  '<svg class="w-4 h-4" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5">' +
  '<rect x="5" y="1" width="9" height="13.5" rx="1.5" />' +
  '<path d="M3 4H2.5A1.5 1.5 0 001 5.5v9A1.5 1.5 0 002.5 16h7a1.5 1.5 0 001.5-1.5V13" />' +
  '</svg>';

interface CodeBlockEntry {
  /** The <pre> element we wrapped. */
  pre: HTMLPreElement;
  /** The outer wrapper div so we can clean up later. */
  wrapper: HTMLDivElement;
}

function attachCopyButtons(container: HTMLElement): void {
  const pres = container.querySelectorAll('pre');
  for (const pre of pres) {
    // Skip if already wrapped
    if (
      pre.parentElement &&
      pre.parentElement.classList.contains('ds-code-block-wrap')
    ) {
      continue;
    }

    const code = pre.querySelector('code');
    const text = code?.textContent ?? pre.textContent ?? '';

    // Build wrapper
    const wrapper = document.createElement('div');
    wrapper.className = 'ds-code-block-wrap relative group';

    // Copy button
    const btn = document.createElement('button');
    btn.className =
      'ds-copy-btn absolute top-2 right-2 z-10 flex items-center gap-1 ' +
      'rounded-md px-2 py-1 text-xs font-medium ' +
      'opacity-0 group-hover:opacity-100 transition-opacity duration-150 ' +
      'bg-canvas-alt/80 hover:bg-canvas-alt text-t-text-muted hover:text-t-text ' +
      'border border-card-border';
    btn.title = 'Copy code';
    btn.innerHTML = COPY_SVG;

    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      try {
        await navigator.clipboard.writeText(text);
        btn.innerHTML = CHECK_SVG;
        btn.classList.add('text-green-600', 'border-green-500/50');
        setTimeout(() => {
          btn.innerHTML = COPY_SVG;
          btn.classList.remove('text-green-600', 'border-green-500/50');
        }, 1500);
      } catch {
        // Clipboard write failed — silently ignore (e.g. insecure context)
      }
    });

    // Insert wrapper before pre, then move pre into wrapper
    pre.parentNode?.insertBefore(wrapper, pre);
    wrapper.appendChild(pre);
    wrapper.appendChild(btn);
  }
}

// ---- component -------------------------------------------------------------

export function MarkdownRenderer({ state, onOpenWorkspaceRelativePath }: RendererProps) {
  const { content } = state;
  const [rendered, setRendered] = useState('');
  const containerRef = useRef<HTMLDivElement>(null);

  const renderSafe = useCallback((raw: string) => {
    if (!raw) return '';
    return DOMPurify.sanitize(md.render(raw), {
      ALLOWED_URI_REGEXP: URI_ALLOW,
    });
  }, []);

  useEffect(() => {
    setRendered(renderSafe(content));
  }, [content, renderSafe]);

  // After every render, inject copy buttons into <pre> blocks
  useEffect(() => {
    if (containerRef.current) {
      attachCopyButtons(containerRef.current);
    }
  }, [rendered]);

  const onClickCapture = useCallback(
    (e: MouseEvent<HTMLDivElement>) => {
      const el = e.target as HTMLElement | null;
      if (!el) {
        return;
      }
      const a = el.closest('a[href]') as HTMLAnchorElement | null;
      if (!a) {
        return;
      }
      const hrefRaw = a.getAttribute('href');
      if (hrefRaw == null) {
        return;
      }
      const trimmed = hrefRaw.trim();
      if (!trimmed) {
        e.preventDefault();
        return;
      }

      if (trimmed.startsWith('#')) {
        e.preventDefault();
        const id = decodeURIComponent(trimmed.slice(1)).trim();
        if (id && typeof CSS !== 'undefined' && typeof CSS.escape === 'function') {
          try {
            const hit = e.currentTarget.querySelector(`#${CSS.escape(id)}`);
            hit?.scrollIntoView({ behavior: 'smooth', block: 'start' });
          } catch {
            /* ignore bad selectors */
          }
        }
        return;
      }

      if (/^https?:\/\//i.test(trimmed)) {
        e.preventDefault();
        window.open(trimmed, '_blank', 'noopener,noreferrer');
        return;
      }

      if (/^[a-z][a-z0-9+.-]*:/i.test(trimmed)) {
        e.preventDefault();
        window.open(trimmed, '_blank', 'noopener,noreferrer');
        return;
      }

      const resolved = resolveMarkdownLinkToWorkspaceRel(state.workspaceRelPath, trimmed);
      if (resolved && onOpenWorkspaceRelativePath) {
        e.preventDefault();
        void onOpenWorkspaceRelativePath(resolved);
        return;
      }

      e.preventDefault();
    },
    [state.workspaceRelPath, onOpenWorkspaceRelativePath],
  );

  if (!content) {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center text-sm text-t-text-muted">
        暂无预览内容。选择文件或生成内容后将在此显示。
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto p-5">
      <div
        ref={containerRef}
        className="prose prose-sm max-w-none font-display leading-relaxed text-t-text
          prose-headings:font-ui prose-headings:font-semibold prose-headings:text-t-text
          prose-h1:text-2xl prose-h2:text-lg prose-h3:text-base
          prose-p:text-t-text prose-p:leading-relaxed
          prose-code:font-mono prose-code:text-sm prose-code:bg-canvas-alt prose-code:px-1 prose-code:py-0.5 prose-code:rounded prose-code:text-t-text
          prose-pre:bg-canvas-alt prose-pre:border prose-pre:border-card-border prose-pre:rounded-lg
          prose-a:text-accent prose-a:no-underline hover:prose-a:underline
          prose-strong:text-t-text prose-li:text-t-text
          prose-blockquote:border-l-accent prose-blockquote:bg-accent-soft prose-blockquote:rounded-r-lg prose-blockquote:py-1 prose-blockquote:px-3
          prose-hr:border-divider
          dark:prose-invert
          [&_pre_code.hljs]:rounded-lg [&_pre_code.hljs]:p-4 [&_code.hljs]:bg-transparent [&_code.hljs]:p-0 [&_code.hljs]:text-sm
          [&_table]:my-4 [&_table]:w-full [&_table]:border-collapse [&_table]:text-sm
          [&_th]:border [&_th]:border-divider [&_th]:bg-canvas-alt/50 [&_th]:px-3 [&_th]:py-2 [&_th]:align-top
          [&_td]:border [&_td]:border-divider [&_td]:px-3 [&_td]:py-2 [&_td]:align-top"
        onClickCapture={onClickCapture}
        dangerouslySetInnerHTML={{ __html: rendered }}
      />
    </div>
  );
}
