import { useCallback, useEffect, useMemo, useState } from 'react';
import DOMPurify from 'dompurify';
import 'highlight.js/styles/github.css';
import { chatMarkdownIt } from '../lib/markdownChatCore';
import { isSafeRelativeWorkspaceHref, isWorkspacePathlike } from '../lib/workspacePathLike';
import { normalizeWorkspaceRelPath } from '../lib/openWorkspaceFile';

const URI_ALLOW =
  /^(?:(?:https?|ftp|mailto|tel):|(?![a-z][a-z0-9+.-]*:)(?:[\w./-]+))$/i;

function enhanceWorkspacePathTargets(html: string): string {
  if (typeof window === 'undefined' || !html) {
    return html;
  }
  try {
    const doc = new DOMParser().parseFromString(`<div id="ds-chat-md-root">${html}</div>`, 'text/html');
    const root = doc.getElementById('ds-chat-md-root');
    if (!root) {
      return html;
    }

    root.querySelectorAll('a[href]').forEach((a) => {
      const href = a.getAttribute('href') ?? '';
      if (!isSafeRelativeWorkspaceHref(href)) {
        return;
      }
      const rel = normalizeWorkspaceRelPath(href);
      if (!rel) {
        return;
      }
      a.setAttribute('data-ds-workspace-rel', rel);
      a.setAttribute('href', '#');
      a.classList.add('ds-chat-ws-link');
      a.setAttribute('role', 'link');
    });

    root.querySelectorAll('code').forEach((code) => {
      if (code.closest('pre')) {
        return;
      }
      const t = code.textContent ?? '';
      if (!isWorkspacePathlike(t)) {
        return;
      }
      const rel = normalizeWorkspaceRelPath(t);
      if (!rel) {
        return;
      }
      const a = doc.createElement('a');
      a.setAttribute('href', '#');
      a.setAttribute('data-ds-workspace-rel', rel);
      a.className = 'ds-chat-ws-link';
      a.setAttribute('role', 'link');
      const parent = code.parentNode;
      if (!parent) {
        return;
      }
      parent.replaceChild(a, code);
      a.appendChild(code);
    });

    return root.innerHTML;
  } catch {
    return html;
  }
}

function sanitizeChatMarkdown(html: string): string {
  return DOMPurify.sanitize(html, {
    ALLOWED_URI_REGEXP: URI_ALLOW,
  });
}

export type ChatMarkdownVariant = 'user' | 'assistant' | 'system';

interface Props {
  content: string;
  variant: ChatMarkdownVariant;
  isStreaming?: boolean;
  onOpenWorkspacePath: (relPath: string) => void | Promise<void>;
}

export function ChatMarkdown({
  content,
  variant,
  isStreaming,
  onOpenWorkspacePath,
}: Props) {
  const [html, setHtml] = useState('');

  const proseUser =
    variant === 'user'
      ? `prose-a:text-accent prose-a:underline-offset-2 prose-a:decoration-2
         prose-code:bg-canvas-alt prose-code:text-t-text`
      : '';

  const className = useMemo(
    () =>
      [
        'chat-md-wrap break-words',
        variant === 'user' ? 'prose prose-sm max-w-none' : 'prose prose-base max-w-none',
        'font-display',
        'prose-headings:font-display prose-headings:font-semibold prose-headings:tracking-tight',
        variant === 'user' ? 'prose-headings:my-2' : 'prose-headings:my-3',
        variant === 'user'
          ? 'prose-p:my-1.5 prose-p:leading-relaxed'
          : 'prose-p:my-2 prose-p:leading-[1.65]',
        'prose-code:font-mono prose-code:text-[0.9em] prose-code:px-1 prose-code:py-0.5 prose-code:rounded',
        'prose-pre:my-2 prose-pre:p-0 prose-pre:bg-transparent prose-pre:border-0',
        variant === 'user' ? 'prose-ul:my-2 prose-ol:my-2' : 'prose-ul:my-3 prose-ol:my-3',
        'prose-li:my-0.5',
        'prose-strong:font-semibold',
        variant === 'user'
          ? 'prose-headings:text-inherit prose-p:text-inherit prose-li:text-inherit prose-strong:text-inherit prose-code:text-inherit prose-th:text-inherit prose-td:text-inherit'
          : [
              'dark:prose-invert',
              'prose-headings:text-t-text prose-p:text-t-text prose-li:text-t-text prose-ol:text-t-text prose-ul:text-t-text prose-strong:text-t-text',
              'prose-th:text-t-text prose-td:text-t-text',
              'prose-blockquote:text-t-text-secondary prose-blockquote:border-divider',
              'prose-hr:border-divider',
              /* Tables: use theme hairlines (default prose-invert borders read as overly bright in dark UI) */
              '[&_table]:w-full [&_table]:border-collapse [&_table]:my-3 [&_table]:text-sm',
              '[&_th]:border [&_th]:border-divider [&_th]:bg-canvas-alt/40 [&_th]:px-3 [&_th]:py-2 [&_th]:align-top',
              '[&_td]:border [&_td]:border-divider [&_td]:px-3 [&_td]:py-2 [&_td]:align-top',
            ].join(' '),
        variant === 'user' ? '' : 'prose-code:bg-canvas-alt prose-code:text-t-text',
        variant === 'user'
          ? 'prose-a:text-accent prose-a:underline hover:prose-a:text-accent-hover'
          : 'prose-a:text-accent prose-a:no-underline hover:prose-a:underline',
        proseUser,
        isStreaming ? 'streaming-cursor' : '',
        '[&_a.ds-chat-ws-link]:cursor-pointer [&_a.ds-chat-ws-link_code]:underline [&_a.ds-chat-ws-link_code]:decoration-dotted',
        variant === 'user'
          ? '[&_a.ds-chat-ws-link_code]:text-inherit'
          : '[&_a.ds-chat-ws-link_code]:text-accent',
      ].join(' '),
    [variant, isStreaming, proseUser],
  );

  useEffect(() => {
    if (!content) {
      setHtml('');
      return;
    }
    const raw = chatMarkdownIt.render(content);
    const safe = sanitizeChatMarkdown(raw);
    setHtml(enhanceWorkspacePathTargets(safe));
  }, [content]);

  const onClickCapture = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const t = e.target as HTMLElement | null;
      if (!t) {
        return;
      }
      const a = t.closest('a[data-ds-workspace-rel]') as HTMLAnchorElement | null;
      if (!a) {
        return;
      }
      const rel = a.getAttribute('data-ds-workspace-rel')?.trim();
      if (!rel) {
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      void onOpenWorkspacePath(rel);
    },
    [onOpenWorkspacePath],
  );

  if (!content) {
    return null;
  }

  return (
    <div
      className={className}
      onClickCapture={onClickCapture}
      // eslint-disable-next-line react/no-danger
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

export default ChatMarkdown;
