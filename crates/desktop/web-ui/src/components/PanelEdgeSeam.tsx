import type { KeyboardEvent, PointerEvent } from 'react';

/** Panel-collapse glyph: vertical rail + chevron toward the hidden edge. */
function CollapseIndentIcon({ side }: { side: 'left' | 'right' }) {
  return (
    <svg
      className="h-3.5 w-3.5"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      aria-hidden
    >
      {side === 'left' ? (
        <>
          <path d="M11 3.5v9" strokeLinecap="round" />
          <path d="M8 8L5 5v6l3-3z" strokeLinejoin="round" />
        </>
      ) : (
        <>
          <path d="M5 3.5v9" strokeLinecap="round" />
          <path d="M8 8l3-3v6l-3-3z" strokeLinejoin="round" />
        </>
      )}
    </svg>
  );
}

export interface PanelEdgeSeamProps {
  side: 'left' | 'right';
  seamClass: 'chrome-seam-r' | 'chrome-seam-l';
  resizing: boolean;
  ariaResize: string;
  collapseTitle?: string;
  onCollapse?: () => void;
  onPointerDown: (e: PointerEvent<HTMLDivElement>) => void;
  onPointerMove: (e: PointerEvent<HTMLDivElement>) => void;
  onPointerUp: (e: PointerEvent<HTMLDivElement>) => void;
  onPointerCancel: (e: PointerEvent<HTMLDivElement>) => void;
  onKeyDown: (e: KeyboardEvent<HTMLDivElement>) => void;
}

/**
 * Resize gutter between chrome columns. Collapse control is hidden until hover
 * (same zone as `cursor-col-resize`).
 */
export default function PanelEdgeSeam({
  side,
  seamClass,
  resizing,
  ariaResize,
  collapseTitle,
  onCollapse,
  onPointerDown,
  onPointerMove,
  onPointerUp,
  onPointerCancel,
  onKeyDown,
}: PanelEdgeSeamProps) {
  const collapsePos =
    side === 'left'
      ? 'right-0 translate-x-1/2'
      : 'left-0 -translate-x-1/2';

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={ariaResize}
      tabIndex={0}
      className={`group panel-edge-seam relative shrink-0 w-3 touch-none select-none cursor-col-resize ${
        resizing ? 'panel-edge-seam--active bg-canvas-alt/80' : ''
      }`}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerCancel}
      onKeyDown={onKeyDown}
    >
      <div
        className={`pointer-events-none absolute inset-y-0 left-1/2 w-1.5 -translate-x-1/2 transition-colors bg-canvas ${seamClass} ${
          resizing ? 'bg-canvas-alt' : 'group-hover:bg-hover'
        }`}
        aria-hidden
      />
      {onCollapse ? (
        <button
          type="button"
          title={collapseTitle}
          aria-label={collapseTitle}
          className={`absolute top-1/2 z-10 flex h-7 w-6 -translate-y-1/2 items-center justify-center rounded-md border border-card-border bg-card text-t-text-muted shadow-md opacity-0 pointer-events-none transition-opacity duration-150 group-hover:opacity-100 group-hover:pointer-events-auto hover:bg-hover hover:text-t-text ${collapsePos}`}
          onClick={(e) => {
            e.stopPropagation();
            onCollapse();
          }}
          onPointerDown={(e) => e.stopPropagation()}
        >
          <CollapseIndentIcon side={side} />
        </button>
      ) : null}
    </div>
  );
}
