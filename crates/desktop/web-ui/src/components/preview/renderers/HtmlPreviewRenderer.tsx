import type { RendererProps } from '../types';

/** Renders UTF-8 HTML preview sidecars (e.g. XLSX table preview). */
export function HtmlPreviewRenderer({ state }: RendererProps) {
  return (
    <iframe
      title={state.title}
      srcDoc={state.content}
      sandbox="allow-same-origin"
      className="h-full min-h-[360px] w-full border-0 bg-white"
    />
  );
}
