import { memo } from 'react';
import type { TurnBlock } from '../../../../lib/chat/timeline/turnBlockTypes';
import type { AgentState } from '../../../../types/agent';
import { isCollapsibleToolCategory, toolCategory } from '../../../../lib/chat/timeline/toolCategories';
import { CompactToolRow } from '../CompactToolRow';
import { renderToolBlockCard, toolBlockToCardModel } from './toolBlockRouter';
import { useT } from '../../../../i18n';

function prefersCompact(block: Extract<TurnBlock, { kind: 'tool' }>): boolean {
  if (block.status === 'running') return false;
  return isCollapsibleToolCategory(toolCategory(block.name));
}

export const ToolBlock = memo(function ToolBlock({
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

  if (prefersCompact(block)) {
    return (
      <CompactToolRow
        block={block}
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
});
