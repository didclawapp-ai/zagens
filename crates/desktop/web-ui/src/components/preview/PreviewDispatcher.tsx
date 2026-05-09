// ---------------------------------------------------------------------------
// PreviewDispatcher — routes a PreviewState to the correct renderer.
// ---------------------------------------------------------------------------

import { FileType, type RendererProps } from './types';
import {
  MarkdownRenderer,
  CodeRenderer,
  TextRenderer,
  ImageRenderer,
  CsvRenderer,
} from './renderers';

export function PreviewDispatcher({ state }: RendererProps) {
  switch (state.fileType) {
    case FileType.Markdown:
      return <MarkdownRenderer state={state} />;
    case FileType.Code:
      return <CodeRenderer state={state} />;
    case FileType.Image:
      return <ImageRenderer state={state} />;
    case FileType.Csv:
      return <CsvRenderer state={state} />;
    case FileType.Pdf:
    case FileType.Office:
      return <OfficePlaceholder state={state} />;
    case FileType.Text:
    case FileType.Unknown:
    default:
      return <TextRenderer state={state} />;
  }
}

// ---------------------------------------------------------------------------
// Phase 2 placeholder for binary document types
// ---------------------------------------------------------------------------

function OfficePlaceholder({ state }: RendererProps) {
  const label =
    state.fileType === FileType.Pdf
      ? 'PDF 预览即将支持'
      : 'Office 文档预览即将支持';

  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 px-6 text-center text-sm text-t-text-muted">
      <p>{label}</p>
      <p className="text-xs">
        文件：{state.fileName ?? state.title}
        {state.size != null ? `（${(state.size / 1024).toFixed(1)} KB）` : ''}
      </p>
      {state.truncated && (
        <p className="text-xs text-amber-text/90 max-w-md">
          内嵌预览尚未支持；已读取大小用于展示，若文件超过 10 MB 仅部分加载。
        </p>
      )}
      <p className="text-xs text-t-text-muted/70">
        目前可右键点击文件，用外部程序打开。
      </p>
    </div>
  );
}
