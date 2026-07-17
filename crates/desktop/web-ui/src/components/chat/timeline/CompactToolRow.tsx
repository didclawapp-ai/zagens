import { useState } from 'react';
import { MessageMetaBar } from '../MessageMetaBar';
import { IconWrench } from '../../icons/FlatIcons';
import { useT } from '../../../i18n';
import type { TurnBlock } from '../../../lib/chat/timeline/turnBlockTypes';
import {
  isAgentTool,
  isBrowserTool,
  isExploreTool,
  isOfficeTool,
  isPlanTool,
  isWorkflowTool,
  isWriteTool,
  isCollapsibleToolCategory,
  toolCategory,
} from '../../../lib/chat/timeline/toolCategories';
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
  if (isWriteTool(block.name) || isOfficeTool(block.name)) {
    const path = parseFileNameFromToolInput(block.input) ?? block.name;
    return t('message.timelineEditedFile', { file: path });
  }
  if (isPlanTool(block.name)) {
    return t('message.timelinePlanUpdated');
  }
  if (isWorkflowTool(block.name) || isAgentTool(block.name)) {
    return t('message.timelineWorkflowOne', { name: block.name });
  }
  if (isBrowserTool(block.name)) {
    return t('message.timelineBrowserOne', { name: block.name });
  }
  if (toolCategory(block.name) === 'shell') {
    return t('message.timelineShellOne', { name: block.name });
  }
  return block.name;
}

function canShowCompact(block: Extract<TurnBlock, { kind: 'tool' }>): boolean {
  if (block.status === 'running') return false;
  return isCollapsibleToolCategory(toolCategory(block.name));
}

export function CompactToolRow({
  block,
  isTurnStreaming: _isTurnStreaming,
  onOpenDiffInPanel,
  agentStates,
}: {
  block: Extract<TurnBlock, { kind: 'tool' }>;
  /** Kept for call-site compatibility; compact depends on block.status only (P4.4). */
  isTurnStreaming?: boolean;
  onOpenDiffInPanel?: () => void;
  agentStates?: AgentState[];
}) {
  const { t } = useT();
  const [expanded, setExpanded] = useState(false);
  void _isTurnStreaming;
  const showCompact = canShowCompact(block);

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
