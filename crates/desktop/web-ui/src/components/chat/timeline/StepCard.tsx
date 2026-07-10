import { useEffect, useRef, useState } from 'react';
import { useT } from '../../../i18n';
import type { TimelinePresentationItem } from '../../../lib/chat/timeline/timelinePresentationTypes';
import { stepHasVisibleProse } from '../../../lib/chat/timeline/settledTurnDisplay';
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
  const isTurnStreaming = Boolean(blockCtx.isTurnStreaming);
  const hasFinalProse = stepHasVisibleProse(items);
  // While streaming: keep steps open so live work is visible.
  // After settle: only steps with final prose stay open (thr_ea9c).
  const preferExpanded = isTurnStreaming || hasFinalProse;
  const [expanded, setExpanded] = useState(preferExpanded);
  const userToggledRef = useRef(false);

  useEffect(() => {
    userToggledRef.current = false;
    setExpanded(preferExpanded);
  }, [stepIndex, preferExpanded]);

  useEffect(() => {
    if (userToggledRef.current) return;
    setExpanded(preferExpanded);
  }, [preferExpanded]);

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
        onClick={() => {
          userToggledRef.current = true;
          setExpanded((v) => !v);
        }}
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
                absorbedCaptions={item.absorbedCaptions}
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
