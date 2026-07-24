import { useT } from '../../../i18n';
import type { TurnChatMessage } from '../../../hooks/useTurnSend';
import type { AgentState } from '../../../types/agent';
import {
  legacyFieldsToBlocks,
  usesDegenerateLegacyLayout,
} from '../../../lib/chat/timeline/legacyMessageAdapter';
import { buildTimelinePresentation } from '../../../lib/chat/timeline/timelineDisplayPipeline';
import {
  isStepGroup,
  partitionPresentationForSettledView,
} from '../../../lib/chat/timeline/settledTurnDisplay';
import type {
  TimelinePresentationItem,
} from '../../../lib/chat/timeline/timelinePresentationTypes';
import { renderTurnBlock } from './blockRenderers';
import { CollapsedToolRunBlock } from './CollapsedToolRunBlock';
import { StepCard } from './StepCard';
import { TurnProcessBundle } from './TurnProcessBundle';
import { AssistantTurnActions } from './AssistantTurnActions';
import { trailingActivityIndex } from './activityPresentation';

function renderPresentationItem(
  item: TimelinePresentationItem,
  blockCtx: Parameters<typeof renderTurnBlock>[1],
  isTrailingActivity: boolean,
) {
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
      isTurnStreaming={blockCtx.isTurnStreaming}
      isTrailingActivity={isTrailingActivity}
    />
  );
}

export function AssistantTurnFrame({
  message,
  workspaceRoot,
  desktopHost,
  agentStates,
  onOpenWorkspacePath,
  onRevealWorkspacePath,
  onOpenDiffInPanel,
}: {
  message: TurnChatMessage;
  workspaceRoot?: string;
  desktopHost?: boolean;
  agentStates?: AgentState[];
  onOpenWorkspacePath: (relPath: string) => void | Promise<void>;
  onRevealWorkspacePath?: (relPath: string) => void;
  onOpenDiffInPanel?: () => void;
}) {
  const { t } = useT();
  const degraded = usesDegenerateLegacyLayout(message);
  const blocks =
    message.blocks && message.blocks.length > 0
      ? message.blocks
      : legacyFieldsToBlocks(message, message.id);
  const isTurnStreaming = Boolean(message.isStreaming);
  // Prefer live layout while any block is still in flight — guards against a
  // one-frame `isStreaming: false` sticky that flashed settled「工作过程」.
  const hasInFlightBlocks = blocks.some(
    (b) =>
      (b.kind === 'thinking' && b.streaming !== false) ||
      (b.kind === 'text' && b.streaming !== false) ||
      (b.kind === 'tool' && b.status === 'running'),
  );
  const useLiveLayout = isTurnStreaming || hasInFlightBlocks;
  const thinkingMissing = Boolean(message.thinkingIncomplete);
  const presentation = buildTimelinePresentation(blocks, {
    stepGrouping: true,
    // Keep P4.6 absorb during live turns (folding); visibility of absorbed
    // reasoning is handled by CollapsedToolRunBlock while streaming.
    absorbActivityGaps: true,
  });
  const blockCtx = {
    isTurnStreaming: useLiveLayout,
    workspaceRoot,
    desktopHost,
    onOpenWorkspacePath,
    onRevealWorkspacePath,
    onOpenDiffInPanel,
    agentStates,
  };

  return (
    <div className="flex my-5 justify-start">
      <div className="message-bubble message-bubble--assistant w-full min-w-0 text-t-text">
        {degraded && (
          <p className="mb-2 text-[11px] text-t-text-muted" role="note">
            {t('message.timelineDegradedOrder')}
          </p>
        )}
        {thinkingMissing && (
          <p className="mb-2 text-[11px] text-t-text-muted" role="note">
            {t('message.timelineThinkingNotPersisted')}
          </p>
        )}
        <div className="space-y-2">
          {useLiveLayout
            ? presentation.map((item, index) => {
                if (isStepGroup(item)) {
                  return (
                    <StepCard
                      key={item.id}
                      stepIndex={item.stepIndex}
                      stepTotal={item.stepTotal}
                      title={item.title}
                      items={item.items}
                      blockCtx={blockCtx}
                    />
                  );
                }
                const trailingIdx = trailingActivityIndex(
                  presentation.filter((p) => !isStepGroup(p)) as TimelinePresentationItem[],
                );
                const flatIndex = presentation
                  .slice(0, index + 1)
                  .filter((p) => !isStepGroup(p)).length - 1;
                const isTrailingActivity =
                  item.kind === 'collapsed_tools' && flatIndex === trailingIdx;
                return (
                  <div key={item.kind === 'block' ? item.block.id : item.id}>
                    {renderPresentationItem(item, blockCtx, isTrailingActivity)}
                  </div>
                );
              })
            : partitionPresentationForSettledView(presentation).map((segment) => {
                if (segment.kind === 'final-step') {
                  return (
                    <StepCard
                      key={segment.id}
                      stepIndex={segment.step.stepIndex}
                      stepTotal={segment.step.stepTotal}
                      title={segment.step.title}
                      items={segment.step.items}
                      blockCtx={blockCtx}
                    />
                  );
                }
                if (segment.kind === 'final-item') {
                  return renderPresentationItem(segment.item, blockCtx, false);
                }
                return (
                  <TurnProcessBundle
                    key={segment.id}
                    items={segment.items}
                    stepCount={segment.stepCount}
                    blockCtx={blockCtx}
                    defaultExpanded={false}
                  />
                );
              })}
        </div>
        {useLiveLayout ? (
          <div className="streaming-status-line mt-2" aria-live="polite">
            {t('message.generating')}
          </div>
        ) : (
          <AssistantTurnActions blocks={blocks} legacyContent={message.content} />
        )}
      </div>
    </div>
  );
}
