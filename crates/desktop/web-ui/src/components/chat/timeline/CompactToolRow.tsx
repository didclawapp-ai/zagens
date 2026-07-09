import { useState } from 'react';
import { MessageMetaBar } from '../MessageMetaBar';
import { IconWrench } from '../../icons/FlatIcons';
import { useT } from '../../../i18n';
import type { TurnBlock } from '../../../lib/chat/timeline/turnBlockTypes';
import { isExploreTool, isPlanTool, isWriteTool } from '../../../lib/chat/timeline/toolCategories';
import { parseFileNameFromToolInput } from '../../../lib/diff/diffEntries';
import { toolBlockToCardModel } from './blocks/toolBlockRouter';
import { ToolBlock } from './blocks/ToolBlock';
import type { AgentState } from '../../../types/agent';

function compactToolLabel(
  block: Extract<TurnBlock, { kind: 'tool' }>,
  t: (key: string, params?: Record<string, string>) => string,
): string {
  if (isExploreTool(block.name)) {
    const path = parseFileNameFromToolInput(block.input) ?? block.name;
    return t('message.timelineExploredOne', { target: path });
  }
  if (isWriteTool(block.name)) {
    const path = parseFileNameFromToolInput(block.input) ?? block.name;
    return t('message.timelineEditedFile', { file: path });
  }
  if (isPlanTool(block.name)) {
    return t('message.timelinePlanUpdated');
  }
  return block.name;
}

export function CompactToolRow({
  block,
  isTurnStreaming,
  onOpenDiffInPanel,
  agentStates,
}: {
  block: Extract<TurnBlock, { kind: 'tool' }>;
  isTurnStreaming: boolean;
  onOpenDiffInPanel?: () => void;
  agentStates?: AgentState[];
}) {
  const { t } = useT();
  const [expanded, setExpanded] = useState(false);
  const running = block.status === 'running';
  const showCompact =
    !running &&
    !isTurnStreaming &&
    (isExploreTool(block.name) || isPlanTool(block.name) || isWriteTool(block.name));

  if (!showCompact) {
    return (
      <div className="tool-stream-item">
        <ToolBlock
          block={block}
          onOpenDiffInPanel={onOpenDiffInPanel}
          agentStates={agentStates}
          compact={false}
        />
      </div>
    );
  }

  const tool = toolBlockToCardModel(block);
  const label = compactToolLabel(block, t);
  const hint = t('message.toolsCollapsed');

  return (
    <MessageMetaBar
      icon={<IconWrench className="size-3.5" />}
      label={label}
      hint={hint}
      expanded={expanded}
      onToggle={() => setExpanded((v) => !v)}
      copyText={tool.output ?? tool.input}
      copyTitle={t('chatMarkdown.copyTool')}
      copyDisabled={!tool.output?.trim() && !tool.input.trim()}
    >
      <div className="tool-stream-item">
        <ToolBlock
          block={block}
          onOpenDiffInPanel={onOpenDiffInPanel}
          agentStates={agentStates}
          compact={false}
        />
      </div>
    </MessageMetaBar>
  );
}
