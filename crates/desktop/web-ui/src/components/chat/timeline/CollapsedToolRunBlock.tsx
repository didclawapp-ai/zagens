import { useState } from 'react';
import { MessageMetaBar } from '../MessageMetaBar';
import { IconWrench } from '../../icons/FlatIcons';
import { useT } from '../../../i18n';
import type { TurnBlock } from '../../../lib/chat/timeline/turnBlockTypes';
import type { ToolCategory } from '../../../lib/chat/timeline/toolCategories';
import { summarizeToolCalls } from '../../../lib/chat/summarizeToolCalls';
import { ToolBlock } from './blocks/ToolBlock';
import { toolBlockToCardModel } from './blocks/toolBlockRouter';
import type { AgentState } from '../../../types/agent';

export function CollapsedToolRunBlock({
  blocks,
  category,
  onOpenDiffInPanel,
  agentStates,
}: {
  blocks: Extract<TurnBlock, { kind: 'tool' }>[];
  category: ToolCategory;
  onOpenDiffInPanel?: () => void;
  agentStates?: AgentState[];
}) {
  const { t } = useT();
  const [expanded, setExpanded] = useState(false);
  const tools = blocks.map(toolBlockToCardModel);
  const label = summarizeToolCalls(tools, t);
  const runningCount = tools.filter((tool) => tool.status === 'running').length;
  const hint =
    runningCount > 0
      ? t('message.toolsRunning', { count: String(runningCount) })
      : t('message.toolsCollapsed');

  return (
    <MessageMetaBar
      icon={<IconWrench className="size-3.5" />}
      label={label}
      hint={hint}
      expanded={expanded}
      onToggle={() => setExpanded((v) => !v)}
    >
      <div className="space-y-1.5">
        {blocks.map((block) => (
          <ToolBlock
            key={block.id}
            block={block}
            onOpenDiffInPanel={onOpenDiffInPanel}
            agentStates={agentStates}
            compact={category === 'explore' || category === 'plan' || category === 'write'}
          />
        ))}
      </div>
    </MessageMetaBar>
  );
}
