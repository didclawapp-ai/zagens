import { useEffect, useRef, useState } from 'react';
import { MessageMetaBar } from '../MessageMetaBar';
import { IconWrench } from '../../icons/FlatIcons';
import { useT } from '../../../i18n';
import type { TimelinePresentationItem } from '../../../lib/chat/timeline/timelinePresentationTypes';
import { countToolsInPresentationItems } from '../../../lib/chat/timeline/settledTurnDisplay';
import { renderTurnBlock, type BlockRendererContext } from './blockRenderers';
import { CollapsedToolRunBlock } from './CollapsedToolRunBlock';

/**
 * Settled-turn wrapper: collapses tool/thinking trail so only final prose stays open.
 */
export function TurnProcessBundle({
  items,
  blockCtx,
  stepCount = 0,
  defaultExpanded = false,
}: {
  items: TimelinePresentationItem[];
  blockCtx: BlockRendererContext;
  /** When process was merged from N tool-only steps. */
  stepCount?: number;
  defaultExpanded?: boolean;
}) {
  const { t } = useT();
  const [expanded, setExpanded] = useState(defaultExpanded);
  const userToggledRef = useRef(false);
  const toolCount = countToolsInPresentationItems(items);

  useEffect(() => {
    if (userToggledRef.current) return;
    setExpanded(defaultExpanded);
  }, [defaultExpanded]);

  let label = t('message.timelineProcess');
  if (toolCount > 0 && stepCount > 0) {
    label = t('message.timelineProcessWithStepsAndTools', {
      steps: String(stepCount),
      count: String(toolCount),
    });
  } else if (toolCount > 0) {
    label = t('message.timelineProcessWithTools', { count: String(toolCount) });
  } else if (stepCount > 0) {
    label = t('message.timelineProcessWithSteps', { steps: String(stepCount) });
  }

  return (
    <MessageMetaBar
      icon={<IconWrench className="size-3.5" />}
      label={label}
      expanded={expanded}
      onToggle={() => {
        userToggledRef.current = true;
        setExpanded((v) => !v);
      }}
    >
      <div className="space-y-2">
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
    </MessageMetaBar>
  );
}
