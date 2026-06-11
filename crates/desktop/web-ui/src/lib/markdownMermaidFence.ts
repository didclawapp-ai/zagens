import type MarkdownIt from 'markdown-it';

function escapeHtmlText(raw: string): string {
  return raw
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
}

/** Render ```mermaid fences as inline diagram placeholders (filled after mount). */
export function applyMermaidFenceRule(md: MarkdownIt): void {
  const defaultFence = md.renderer.rules.fence;
  if (!defaultFence) {
    return;
  }

  md.renderer.rules.fence = (tokens, idx, options, env, self) => {
    const token = tokens[idx];
    const lang = token.info.trim().split(/\s+/)[0]?.toLowerCase() ?? '';
    if (lang !== 'mermaid') {
      return defaultFence(tokens, idx, options, env, self);
    }

    const escaped = escapeHtmlText(token.content);
    return (
      '<div class="ds-mermaid-block my-4 rounded-lg border border-card-border bg-canvas-alt/30 overflow-x-auto">' +
      `<pre class="ds-mermaid-source" hidden aria-hidden="true">${escaped}</pre>` +
      '<div class="ds-mermaid-mount flex items-center justify-center min-h-[4rem] p-4 text-xs text-t-text-muted">渲染中…</div>' +
      '</div>\n'
    );
  };
}
