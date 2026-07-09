import { useT } from '../../../i18n';
import type { TurnChatMessage } from '../../../hooks/useTurnSend';
import type { AgentState } from '../../../types/agent';
import {
  legacyFieldsToBlocks,
  usesDegenerateLegacyLayout,
} from '../../../lib/chat/timeline/legacyMessageAdapter';
import {
  buildTimelinePresentation,
  type TimelinePresentationItem,
  type TimelinePresentationRoot,
} from '../../../lib/chat/timeline/timelineDisplayPipeline';
import { renderTurnBlock } from './blockRenderers';
import { CollapsedToolRunBlock } from './CollapsedToolRunBlock';
import { StepCard } from './StepCard';

function isStepGroup(item: TimelinePresentationRoot): item is Extract<TimelinePresentationRoot, { kind: 'step' }> {
  return typeof item === 'object' && item !== null && 'kind' in item && item.kind === 'step';
}

function renderPresentationItem(
  item: TimelinePresentationItem,
  blockCtx: Parameters<typeof renderTurnBlock>[1],
) {
  if (item.kind === 'block') {
    return renderTurnBlock(item.block, blockCtx);
  }
  return (
    <CollapsedToolRunBlock
      key={item.id}
      blocks={item.blocks}
      category={item.category}
      onOpenDiffInPanel={blockCtx.onOpenDiffInPanel}
      agentStates={blockCtx.agentStates}
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
  const thinkingMissing = Boolean(message.thinkingIncomplete);
  const presentation = buildTimelinePresentation(blocks, {
    stepGrouping: true,
  });
  const blockCtx = {
    isTurnStreaming,
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
          {presentation.map((item) => {
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
            return renderPresentationItem(item, blockCtx);
          })}
        </div>
        {isTurnStreaming && (
          <div className="streaming-status-line mt-2" aria-live="polite">
            {t('message.generating')}
          </div>
        )}
      </div>
    </div>
  );
}
