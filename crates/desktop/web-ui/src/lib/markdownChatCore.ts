// Shared markdown-it + highlighter for chat bubbles (mirrors preview MarkdownRenderer).

import MarkdownIt from 'markdown-it';
import hljs from 'highlight.js/lib/core';
import plaintext from 'highlight.js/lib/languages/plaintext';
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import rust from 'highlight.js/lib/languages/rust';
import bash from 'highlight.js/lib/languages/bash';
import json from 'highlight.js/lib/languages/json';
import markdown from 'highlight.js/lib/languages/markdown';

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

export const chatMarkdownIt = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: true,
  highlight(str: string, langRaw: string): string {
    return highlightFence(str, langRaw ?? '');
  },
});
