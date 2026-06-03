import type { RendererProps } from '../types';

export function PdfRenderer({ state }: RendererProps) {
  const mime = state.mimeType ?? 'application/pdf';
  const b64 = state.content.trim();
  if (!b64) {
    return <p className="p-4 text-sm text-t-text-muted">无 PDF 数据</p>;
  }
  const src = `data:${mime};base64,${b64}`;
  return (
    <iframe
      title={state.title}
      src={src}
      className="h-full min-h-[480px] w-full border-0 bg-white"
    />
  );
}
