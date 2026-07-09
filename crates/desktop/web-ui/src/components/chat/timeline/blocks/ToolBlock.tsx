import type { TurnBlock } from '../../../../lib/chat/timeline/turnBlockTypes';
import type { AgentState } from '../../../../types/agent';
import { isExploreTool, isPlanTool, isWriteTool } from '../../../../lib/chat/timeline/toolCategories';
import { CompactToolRow } from '../CompactToolRow';
import { renderToolBlockCard, toolBlockToCardModel } from './toolBlockRouter';
import { useT } from '../../../../i18n';

export function ToolBlock({
  block,
  onOpenDiffInPanel,
  agentStates,
  compact,
}: {
  block: Extract<TurnBlock, { kind: 'tool' }>;
  onOpenDiffInPanel?: () => void;
  agentStates?: AgentState[];
  compact?: boolean;
}) {
  const { t } = useT();

  if (compact === false) {
    const tool = toolBlockToCardModel(block);
    return (
      <div className="tool-stream-item">
        {renderToolBlockCard(tool, onOpenDiffInPanel, t('chatMarkdown.copyTool'), agentStates)}
      </div>
    );
  }

  if (
    block.status !== 'running' &&
    (isExploreTool(block.name) || isPlanTool(block.name) || isWriteTool(block.name))
  ) {
    return (
      <CompactToolRow
        block={block}
        isTurnStreaming={false}
        onOpenDiffInPanel={onOpenDiffInPanel}
        agentStates={agentStates}
      />
    );
  }

  const tool = toolBlockToCardModel(block);
  return (
    <div className="tool-stream-item">
      {renderToolBlockCard(tool, onOpenDiffInPanel, t('chatMarkdown.copyTool'), agentStates)}
    </div>
  );
}
