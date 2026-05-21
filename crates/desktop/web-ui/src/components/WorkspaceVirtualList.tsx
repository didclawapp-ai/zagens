import { useCallback, useEffect, useState, type ReactNode, type RefObject } from 'react';
import { WORKSPACE_LIST_ROW_PX } from '../lib/workspaceFileSearch';

type Props = {
  scrollRef: RefObject<HTMLDivElement | null>;
  count: number;
  rowHeight?: number;
  renderRow: (index: number) => ReactNode;
  /** Extra bottom padding inside the scroll area (px). */
  paddingBottom?: number;
};

/**
 * Fixed-height windowed list (B4) — no extra dependency; parent must be the scroll container.
 */
export default function WorkspaceVirtualList({
  scrollRef,
  count,
  rowHeight = WORKSPACE_LIST_ROW_PX,
  renderRow,
  paddingBottom = 8,
}: Props) {
  const [range, setRange] = useState({ start: 0, end: Math.min(count, 40) });

  const updateRange = useCallback(() => {
    const el = scrollRef.current;
    if (!el || count === 0) {
      setRange({ start: 0, end: 0 });
      return;
    }
    const top = el.scrollTop;
    const viewH = el.clientHeight;
    const overscan = 8;
    const start = Math.max(0, Math.floor(top / rowHeight) - overscan);
    const visible = Math.ceil(viewH / rowHeight) + overscan * 2;
    const end = Math.min(count, start + visible);
    setRange({ start, end });
  }, [scrollRef, count, rowHeight]);

  useEffect(() => {
    updateRange();
    const el = scrollRef.current;
    if (!el) return;
    el.addEventListener('scroll', updateRange, { passive: true });
    const ro = new ResizeObserver(updateRange);
    ro.observe(el);
    return () => {
      el.removeEventListener('scroll', updateRange);
      ro.disconnect();
    };
  }, [scrollRef, updateRange]);

  useEffect(() => {
    updateRange();
  }, [count, updateRange]);

  if (count === 0) {
    return null;
  }

  const totalH = count * rowHeight + paddingBottom;
  const offsetY = range.start * rowHeight;

  return (
    <div className="relative w-full" style={{ height: totalH }}>
      <div
        className="absolute left-0 right-0 space-y-0.5"
        style={{ top: offsetY }}
      >
        {Array.from({ length: range.end - range.start }, (_, i) => {
          const index = range.start + i;
          return <div key={index}>{renderRow(index)}</div>;
        })}
      </div>
    </div>
  );
}
