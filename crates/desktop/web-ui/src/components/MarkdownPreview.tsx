import { useCallback, useEffect, useState } from 'react';
import MarkdownIt from 'markdown-it';
import DOMPurify from 'dompurify';
import hljs from 'highlight.js/lib/core';
import plaintext from 'highlight.js/lib/languages/plaintext';
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import rust from 'highlight.js/lib/languages/rust';
import bash from 'highlight.js/lib/languages/bash';
import json from 'highlight.js/lib/languages/json';
import markdown from 'highlight.js/lib/languages/markdown';

import 'highlight.js/styles/github.css';

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
    return hljs.highlight(raw, { language: 'plaintext', ignoreIllegals: true }).value;
  } catch {
    return escapeFallback(raw);
  }
}

const md = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: true,
  highlight(str: string, langRaw: string): string {
    return highlightFence(str, langRaw ?? '');
  },
});

interface Props {
  content: string;
  fileName?: string;
  language?: string;
}

export default function MarkdownPreview({ content, fileName, language }: Props) {
  const [rendered, setRendered] = useState('');

  const renderRaw = useCallback(
    (raw: string) => {
      if (!raw) return '';
      if (!language && !fileName?.endsWith('.md')) {
        const escaped = escapeFallback(raw);
        return `<pre class="text-sm whitespace-pre-wrap font-mono leading-relaxed">${escaped}</pre>`;
      }
      // Sanitize markdown-it output: allow only safe URL protocols
      // (https://, mailto:) and strip executable attributes (#M9).
      return DOMPurify.sanitize(md.render(raw), {
        ALLOWED_URI_REGEXP:
          /^(?:(?:https?|ftp|mailto|tel):|[^a-z]|[a-z+.-]+(?:[^a-z+.-:]|$))/i,
      });
    },
    [fileName, language],
  );

  useEffect(() => {
    setRendered(renderRaw(content));
  }, [content, renderRaw]);

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
        dangerouslySetInnerHTML={{ __html: rendered }}
      />
    </div>
  );
}
