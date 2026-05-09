// ---------------------------------------------------------------------------
// CodeRenderer — direct hljs syntax highlighting for source files.
//
// Uses the `language` hint from the runtime API (already resolved server-side
// by `language_from_name`).  Falls back to plaintext highlighting when the
// language is unknown.
// ---------------------------------------------------------------------------

import { useMemo } from 'react';
import hljs from 'highlight.js/lib/core';
import plaintext from 'highlight.js/lib/languages/plaintext';
import rust from 'highlight.js/lib/languages/rust';
import typescript from 'highlight.js/lib/languages/typescript';
import javascript from 'highlight.js/lib/languages/javascript';
import json from 'highlight.js/lib/languages/json';
import bash from 'highlight.js/lib/languages/bash';
import toml from 'highlight.js/lib/languages/toml';
import yaml from 'highlight.js/lib/languages/yaml';
import python from 'highlight.js/lib/languages/python';
import css from 'highlight.js/lib/languages/css';
import xml from 'highlight.js/lib/languages/xml';
import sql from 'highlight.js/lib/languages/sql';
import go from 'highlight.js/lib/languages/go';
import c from 'highlight.js/lib/languages/c';
import cpp from 'highlight.js/lib/languages/cpp';

import type { RendererProps } from '../types';

// ---------------------------------------------------------------------------
// Language registration (one-time, module scope)
// ---------------------------------------------------------------------------
hljs.registerLanguage('plaintext', plaintext);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('rs', rust);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('ts', typescript);
hljs.registerLanguage('tsx', typescript);
hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('js', javascript);
hljs.registerLanguage('jsx', javascript);
hljs.registerLanguage('json', json);
hljs.registerLanguage('bash', bash);
hljs.registerLanguage('sh', bash);
hljs.registerLanguage('shell', bash);
hljs.registerLanguage('toml', toml);
hljs.registerLanguage('yaml', yaml);
hljs.registerLanguage('yml', yaml);
hljs.registerLanguage('python', python);
hljs.registerLanguage('py', python);
hljs.registerLanguage('css', css);
hljs.registerLanguage('html', xml); // highlight.js uses `xml` grammar for HTML
hljs.registerLanguage('htm', xml);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('sql', sql);
hljs.registerLanguage('go', go);
hljs.registerLanguage('c', c);
hljs.registerLanguage('h', c);
hljs.registerLanguage('cpp', cpp);
hljs.registerLanguage('cc', cpp);
hljs.registerLanguage('hpp', cpp);

// ---- helpers ---------------------------------------------------------------

function langKey(language?: string): string {
  const key = language?.trim().split(/\s+/)[0]?.toLowerCase() ?? '';
  return key && hljs.getLanguage(key) ? key : 'plaintext';
}

function escapeHtml(str: string): string {
  return str.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

// ---- component -------------------------------------------------------------

export function CodeRenderer({ state }: RendererProps) {
  const { content, language, fileName, size } = state;

  const html = useMemo(() => {
    if (!content) return '';
    const truncated =
      content.length > 512_000 ? content.slice(0, 512_000) : content;
    const key = langKey(language);
    try {
      return hljs.highlight(truncated, { language: key, ignoreIllegals: true })
        .value;
    } catch {
      return escapeHtml(truncated);
    }
  }, [content, language]);

  const truncated = content.length > 512_000;
  const displaySize = size ?? content.length;

  if (!content) {
    return (
      <div className="flex h-full items-center justify-center px-6 text-center text-sm text-t-text-muted">
        空文件
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto p-5">
      <pre className="hljs rounded-lg p-4 text-sm !bg-canvas-alt border border-card-border">
        <code
          className="hljs !bg-transparent !p-0 text-sm"
          dangerouslySetInnerHTML={{ __html: html }}
        />
      </pre>
      {truncated && (
        <p className="mt-2 text-xs text-amber-text/90">
          文件过大（{(displaySize / 1024).toFixed(1)} KB），仅显示前 512 KB。
          {fileName ? `（${fileName}）` : ''}
        </p>
      )}
    </div>
  );
}
