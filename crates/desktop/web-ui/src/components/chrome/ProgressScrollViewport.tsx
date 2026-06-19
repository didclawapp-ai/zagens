import type { ReactNode } from 'react';
import { useT } from '../../i18n';
import { usePrefersReducedMotion } from '../../lib/usePrefersReducedMotion';
import type { ProgressScrollItem } from '../../lib/progressScroll';
import { useProgressScrollLayout } from '../../lib/useProgressScroll';

export type ProgressScrollViewportProps = {
  items: readonly ProgressScrollItem[];
  maxRows?: number;
  emptyLabel?: string;
  renderItem: (item: ProgressScrollItem, index: number) => ReactNode;
  className?: string;
};

/** Card body viewport — hides done rows and scrolls to the current row. */
export default function ProgressScrollViewport({
  items,
  maxRows = 2,
  emptyLabel,
  renderItem,
  className = '',
}: ProgressScrollViewportProps) {
  const { t } = useT();
  const prefersReducedMotion = usePrefersReducedMotion();
  const layout = useProgressScrollLayout(items, maxRows);
  const resolvedEmpty = emptyLabel ?? t('harnessCard.progressAllDone');

  if (layout.allDone) {
    return (
      <div className={`card-scroll card-scroll--empty ${className}`.trim()} style={{ height: layout.viewportHeightPx }}>
        <div className="card-scroll-empty">{resolvedEmpty}</div>
      </div>
    );
  }

  const openItems = items.filter((item) => item.progress !== 'done');

  return (
    <div
      className={`card-scroll ${layout.overflow ? 'card-scroll--overflow' : ''} ${
        layout.scrollTop ? 'card-scroll--top' : ''
      } ${layout.scrollBottom ? 'card-scroll--bottom' : ''} ${className}`.trim()}
      style={{ height: layout.viewportHeightPx }}
    >
      <div
        className="card-scroll-track"
        style={{
          transform: `translateY(-${layout.offsetPx}px)`,
          transition: prefersReducedMotion ? 'none' : 'transform 0.35s ease',
        }}
      >
        {openItems.map((item, index) => (
          <div
            key={item.id}
            className={`card-scroll-line ${
              item.progress === 'current' ? 'card-scroll-line--current' : ''
            } ${item.progress === 'pending' ? 'card-scroll-line--pending' : ''}`}
            data-progress={item.progress}
          >
            {renderItem(item, index)}
          </div>
        ))}
      </div>
    </div>
  );
}
