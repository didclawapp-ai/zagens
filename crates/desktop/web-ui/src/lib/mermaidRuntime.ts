import mermaid from 'mermaid';
import { sanitizeMermaidSvg } from './sanitizeHtml';
import type { Theme } from './appPreferences';

let configuredTheme: Theme | null = null;

/** Keep a single Mermaid config in sync with app light/dark theme. */
export function ensureMermaidInitialized(theme: Theme): void {
  if (configuredTheme === theme) {
    return;
  }
  mermaid.initialize({
    startOnLoad: false,
    theme: theme === 'dark' ? 'dark' : 'default',
    securityLevel: 'strict',
  });
  configuredTheme = theme;
}

/** Reset cached theme after tests or hot reload (optional). */
export function resetMermaidRuntimeForTests(): void {
  configuredTheme = null;
}

function blockDigest(code: string): string {
  let hash = 0;
  for (let i = 0; i < code.length; i++) {
    hash = ((hash << 5) - hash) + code.charCodeAt(i);
    hash |= 0;
  }
  return Math.abs(hash).toString(36);
}

export async function renderMermaidToSvg(
  code: string,
  renderId: string,
  theme: Theme,
): Promise<string> {
  ensureMermaidInitialized(theme);
  const { svg } = await mermaid.render(renderId, code);
  return sanitizeMermaidSvg(svg);
}

/** Find `.ds-mermaid-block` placeholders and render diagrams inline. */
export async function renderMermaidBlocksInContainer(
  container: HTMLElement,
  theme: Theme,
): Promise<void> {
  const blocks = container.querySelectorAll<HTMLElement>('.ds-mermaid-block');
  for (let i = 0; i < blocks.length; i++) {
    const block = blocks[i];
    if (block.dataset.mermaidRendered === '1') {
      continue;
    }
    const source = block.querySelector('.ds-mermaid-source')?.textContent?.trim();
    const mount = block.querySelector<HTMLElement>('.ds-mermaid-mount');
    if (!source || !mount) {
      continue;
    }
    const renderId = `md-mermaid-${blockDigest(source)}-${i}`;
    mount.textContent = '渲染中…';
    try {
      const svg = await renderMermaidToSvg(source, renderId, theme);
      mount.innerHTML = svg;
      block.dataset.mermaidRendered = '1';
      mount.classList.remove('text-xs', 'text-t-text-muted');
    } catch (e) {
      const msg = (e as Error).message || String(e);
      mount.innerHTML = '';
      mount.classList.add('text-xs', 'text-red-400', 'p-3');
      mount.textContent = `Mermaid 渲染失败：${msg}`;
      block.dataset.mermaidRendered = 'error';
    }
  }
}
