import { useState } from 'react';
import { useT } from '../../../i18n';
import type { TimelinePresentationItem } from '../../../lib/chat/timeline/timelinePresentationTypes';
import { renderTurnBlock, type BlockRendererContext } from './blockRenderers';
import { CollapsedToolRunBlock } from './CollapsedToolRunBlock';

export function StepCard({
  stepIndex,
  stepTotal,
  title,
  items,
  blockCtx,
}: {
  stepIndex: number;
  stepTotal: number;
  title: string;
  items: TimelinePresentationItem[];
  blockCtx: BlockRendererContext;
}) {
  const { t } = useT();
  const [expanded, setExpanded] = useState(true);
  const label =
    title.trim() ||
    t('message.timelineStepUntitled', {
      index: String(stepIndex),
      total: String(stepTotal),
    });

  return (
    <section className="rounded-lg border border-t-border/60 bg-t-surface/30">
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-2 text-left text-[12px] font-medium text-t-text-muted hover:text-t-text"
        onClick={() => setExpanded((v) => !v)}
        aria-expanded={expanded}
      >
        <span className="tabular-nums text-t-text-muted/80">
          {t('message.timelineStepBadge', {
            index: String(stepIndex),
            total: String(stepTotal),
          })}
        </span>
        <span className="min-w-0 flex-1 truncate">{label}</span>
        <span className="text-[11px]">{expanded ? '▾' : '▸'}</span>
      </button>
      {expanded && (
        <div className="space-y-2 border-t border-t-border/40 px-3 py-2">
          {items.map((item) => {
            if (item.kind === 'block') {
              return renderTurnBlock(item.block, blockCtx);
            }
            return (
              <CollapsedToolRunBlock
                key={item.id}
                blocks={item.blocks}
                category={item.category}
                absorbedThinking={item.absorbedThinking}
                onOpenDiffInPanel={blockCtx.onOpenDiffInPanel}
                agentStates={blockCtx.agentStates}
              />
            );
          })}
        </div>
      )}
    </section>
  );
}
