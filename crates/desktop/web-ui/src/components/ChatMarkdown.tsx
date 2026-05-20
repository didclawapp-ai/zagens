import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import DOMPurify from 'dompurify';
import 'highlight.js/styles/github.css';
import WorkspaceLinkContextMenu, {
  type WorkspaceLinkMenuState,
} from './chat/WorkspaceLinkContextMenu';
import { useT } from '../i18n';
import { enhanceChatCodeBlocks } from '../lib/enhanceChatCodeBlocks';
import { chatMarkdownIt } from '../lib/markdownChatCore';
import {
  CHAT_MARKDOWN_ALLOWED_URI,
  isSafeRelativeWorkspaceHref,
  isWorkspacePathlike,
} from '../lib/workspacePathLike';
import { normalizeWorkspaceRelPath } from '../lib/openWorkspaceFile';
import { workspaceAbsolutePath } from '../lib/workspaceLinkMenu';

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
    ALLOWED_URI_REGEXP: CHAT_MARKDOWN_ALLOWED_URI,
  });
}

export type ChatMarkdownVariant = 'user' | 'assistant' | 'system';

interface Props {
  content: string;
  variant: ChatMarkdownVariant;
  isStreaming?: boolean;
  workspaceRoot?: string;
  desktopHost?: boolean;
  onOpenWorkspacePath: (relPath: string) => void | Promise<void>;
}

export function ChatMarkdown({
  content,
  variant,
  isStreaming,
  workspaceRoot = '',
  desktopHost = false,
  onOpenWorkspacePath,
}: Props) {
  const { t } = useT();
  const containerRef = useRef<HTMLDivElement>(null);
  const [html, setHtml] = useState('');
  const [wsMenu, setWsMenu] = useState<WorkspaceLinkMenuState | null>(null);

  const proseUser =
    variant === 'user'
      ? `text-msg-user-text
         prose-p:text-msg-user-text prose-li:text-msg-user-text prose-ol:text-msg-user-text prose-ul:text-msg-user-text
         prose-strong:text-msg-user-text prose-headings:text-msg-user-text
         prose-code:bg-canvas-alt prose-code:text-msg-user-text prose-td:text-msg-user-text prose-th:text-msg-user-text
         prose-blockquote:text-t-text-secondary
         prose-a:text-accent prose-a:underline-offset-2 prose-a:decoration-2
         [--tw-prose-body:var(--color-msg-user-text)][--tw-prose-headings:var(--color-msg-user-text)][--tw-prose-bold:var(--color-msg-user-text)]
         [--tw-prose-code:var(--color-msg-user-text)][--tw-prose-quotes:var(--color-text-secondary)]
         [--tw-prose-counters:var(--color-text-muted)][--tw-prose-bullets:var(--color-text-muted)]`
      : '';

  const className = useMemo(
    () =>
      [
        'chat-md-wrap break-words',
        variant === 'user' ? 'chat-md-wrap--user prose prose-sm max-w-none' : 'prose prose-base max-w-none',
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
          ? ''
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
        '[&_a.ds-chat-ws-link]:cursor-pointer [&_a.ds-chat-ws-link_code]:underline [&_a.ds-chat-ws-link_code]:decoration-dotted',
        variant === 'user'
          ? '[&_a.ds-chat-ws-link_code]:text-msg-user-text'
          : '[&_a.ds-chat-ws-link_code]:text-accent',
      ].join(' '),
    [variant, proseUser],
  );

  const streamingPlainClassName = useMemo(
    () =>
      [
        'chat-md-wrap break-words font-display text-sm leading-relaxed',
        variant === 'user' ? 'text-msg-user-text' : 'text-t-text',
      ].join(' '),
    [variant],
  );

  useEffect(() => {
    if (!content) {
      setHtml('');
      return;
    }
    if (isStreaming) {
      return;
    }
    const raw = chatMarkdownIt.render(content);
    const safe = sanitizeChatMarkdown(raw);
    setHtml(enhanceWorkspacePathTargets(safe));
  }, [content, isStreaming]);

  useEffect(() => {
    enhanceChatCodeBlocks(containerRef.current, {
      copy: t('chatMarkdown.copyCode'),
      copied: t('chatMarkdown.copied'),
    });
  }, [html, t]);

  const onClickCapture = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const target = e.target as HTMLElement | null;
      if (!target) {
        return;
      }
      const a = target.closest('a[data-ds-workspace-rel]') as HTMLAnchorElement | null;
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

  const onContextMenuCapture = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const target = e.target as HTMLElement | null;
      if (!target) {
        return;
      }
      const a = target.closest('a[data-ds-workspace-rel]') as HTMLAnchorElement | null;
      if (!a) {
        return;
      }
      e.preventDefault();
      const rel = a.getAttribute('data-ds-workspace-rel')?.trim();
      if (!rel) {
        return;
      }
      const fileName = rel.split('/').pop() ?? rel;
      setWsMenu({
        relPath: rel,
        absPath: workspaceAbsolutePath(workspaceRoot, rel),
        fileName,
        x: e.clientX,
        y: e.clientY,
      });
    },
    [workspaceRoot],
  );

  const onOpenSystem = useCallback(async (absPath: string) => {
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('open_with_system_app', { path: absPath });
    } catch {
      /* ignore — same as RightPanel file tree */
    }
  }, []);

  if (!content) {
    return null;
  }

  if (isStreaming) {
    return (
      <div ref={containerRef} className={streamingPlainClassName}>
        <div className="whitespace-pre-wrap break-words">{content}</div>
      </div>
    );
  }

  return (
    <>
      <div
        ref={containerRef}
        className={className}
        onClickCapture={onClickCapture}
        onContextMenuCapture={onContextMenuCapture}
        // eslint-disable-next-line react/no-danger
        dangerouslySetInnerHTML={{ __html: html }}
      />
      {wsMenu ? (
        <WorkspaceLinkContextMenu
          menu={wsMenu}
          desktopHost={desktopHost}
          onClose={() => setWsMenu(null)}
          onOpenSystem={onOpenSystem}
        />
      ) : null}
    </>
  );
}

export default ChatMarkdown;
