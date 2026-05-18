/** Inject copy-to-clipboard controls on fenced code blocks in chat markdown HTML. */

const COPY_ICON = `<svg class="ds-code-copy-icon" viewBox="0 0 24 24" aria-hidden="true"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;
const CHECK_ICON = `<svg class="ds-code-copy-icon" viewBox="0 0 24 24" aria-hidden="true"><path d="M20 6L9 17l-5-5"/></svg>`;

export interface CodeBlockCopyLabels {
  copy: string;
  copied: string;
}

async function copyPlainText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const ta = document.createElement('textarea');
      ta.value = text;
      ta.style.position = 'fixed';
      ta.style.left = '-9999px';
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand('copy');
      document.body.removeChild(ta);
      return ok;
    } catch {
      return false;
    }
  }
}

function codeTextFromPre(pre: HTMLPreElement): string {
  const code = pre.querySelector('code');
  return (code?.textContent ?? pre.textContent ?? '').replace(/\n$/, '');
}

function languageLabel(pre: HTMLPreElement): string | null {
  const code = pre.querySelector('code');
  if (!code) {
    return null;
  }
  for (const cls of code.classList) {
    const m = /^language-(.+)$/.exec(cls);
    if (m?.[1]) {
      return m[1];
    }
  }
  return null;
}

function ensureCopyButton(
  shell: HTMLDivElement,
  pre: HTMLPreElement,
  labels: CodeBlockCopyLabels,
): HTMLButtonElement {
  let btn = shell.querySelector<HTMLButtonElement>('button.ds-code-copy-btn');
  if (!btn) {
    btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'ds-code-copy-btn';
    btn.innerHTML = COPY_ICON;
    shell.insertBefore(btn, pre);

    btn.addEventListener('click', async (e) => {
      e.preventDefault();
      e.stopPropagation();
      const text = codeTextFromPre(pre);
      if (!text) {
        return;
      }
      const ok = await copyPlainText(text);
      if (!ok) {
        return;
      }
      btn!.classList.add('ds-code-copy-btn--copied');
      btn!.setAttribute('aria-label', labels.copied);
      btn!.title = labels.copied;
      btn!.innerHTML = CHECK_ICON;
      window.setTimeout(() => {
        btn!.classList.remove('ds-code-copy-btn--copied');
        btn!.setAttribute('aria-label', labels.copy);
        btn!.title = labels.copy;
        btn!.innerHTML = COPY_ICON;
      }, 2000);
    });
  }
  btn.setAttribute('aria-label', labels.copy);
  btn.title = labels.copy;
  return btn;
}

/** Wrap each `<pre>` under `root` with a shell + copy button (idempotent). */
export function enhanceChatCodeBlocks(
  root: HTMLElement | null,
  labels: CodeBlockCopyLabels,
): void {
  if (!root) {
    return;
  }

  root.querySelectorAll('pre').forEach((preEl) => {
    const pre = preEl as HTMLPreElement;
    let shell = pre.parentElement;
    if (!shell?.classList.contains('ds-code-block-shell')) {
      shell = document.createElement('div');
      shell.className = 'ds-code-block-shell';
      const parent = pre.parentNode;
      if (!parent) {
        return;
      }
      parent.insertBefore(shell, pre);
      shell.appendChild(pre);

      const lang = languageLabel(pre);
      if (lang) {
        const tag = document.createElement('span');
        tag.className = 'ds-code-block-lang';
        tag.textContent = lang;
        shell.insertBefore(tag, pre);
      }
    }

    ensureCopyButton(shell as HTMLDivElement, pre, labels);
  });
}
