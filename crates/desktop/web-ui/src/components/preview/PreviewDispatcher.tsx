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
  PdfRenderer,
  HtmlRenderer,
} from './renderers';

export function PreviewDispatcher({
  state,
  theme,
  onOpenWorkspaceRelativePath,
}: RendererProps) {
  const common = { state, theme, onOpenWorkspaceRelativePath };
  switch (state.fileType) {
    case FileType.Markdown:
      return <MarkdownRenderer {...common} />;
    case FileType.Code:
      return <CodeRenderer {...common} />;
    case FileType.Image:
      return <ImageRenderer {...common} />;
    case FileType.Csv:
      return <CsvRenderer {...common} />;
    case FileType.Pdf:
      return <PdfRenderer {...common} />;
    case FileType.Html:
      return <HtmlRenderer {...common} />;
    case FileType.Office:
      return (
        <p className="p-6 text-center text-sm text-t-text-muted">
          Word / Excel / PowerPoint 请使用系统默认应用打开（双击文件或右键「用系统应用打开」）。
        </p>
      );
    case FileType.Text:
    case FileType.Unknown:
    default:
      return <TextRenderer {...common} />;
  }
}
