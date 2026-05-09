// ---------------------------------------------------------------------------
// PreviewContainer — overlay shell extracted from RightPanel.
//
// Renders the "close preview" header bar and the scrollable content area
// below it.  The actual renderer is passed as `children`.
// ---------------------------------------------------------------------------

import type { ReactNode } from 'react';

interface Props {
  title: string;
  onClose: () => void;
  children: ReactNode;
}

export function PreviewContainer({ title, onClose, children }: Props) {
  return (
    <div className="flex flex-1 flex-col min-h-0">
      {/* Header bar */}
      <div className="shrink-0 flex items-center gap-2 border-b border-divider px-3 py-2 bg-canvas-alt/50">
        <button
          type="button"
          className="shrink-0 rounded-md px-2 py-1 text-xs font-medium text-accent hover:bg-hover"
          onClick={onClose}
        >
          关闭预览
        </button>
        <span
          className="truncate text-xs font-medium text-t-text"
          title={title}
        >
          {title}
        </span>
      </div>

      {/* Content area */}
      <div className="flex-1 min-h-0 overflow-y-auto">
        {children}
      </div>
    </div>
  );
}
