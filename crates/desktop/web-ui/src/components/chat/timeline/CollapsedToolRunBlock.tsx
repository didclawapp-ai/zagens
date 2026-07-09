import { useState } from 'react';
import { MessageMetaBar } from '../MessageMetaBar';
import { IconSparkle, IconWrench } from '../../icons/FlatIcons';
import { useT } from '../../../i18n';
import type { TurnBlock } from '../../../lib/chat/timeline/turnBlockTypes';
import type { TimelineCollapsedCategory } from '../../../lib/chat/timeline/timelinePresentationTypes';
import { summarizeToolCalls } from '../../../lib/chat/summarizeToolCalls';
import { ToolBlock } from './blocks/ToolBlock';
import { toolBlockToCardModel } from './blocks/toolBlockRouter';
import type { AgentState } from '../../../types/agent';

export function CollapsedToolRunBlock({
  blocks,
  category,
  absorbedThinking,
  onOpenDiffInPanel,
  agentStates,
}: {
  blocks: Extract<TurnBlock, { kind: 'tool' }>[];
  category: TimelineCollapsedCategory;
  absorbedThinking?: Extract<TurnBlock, { kind: 'thinking' }>[];
  onOpenDiffInPanel?: () => void;
  agentStates?: AgentState[];
}) {
  const { t } = useT();
  const [expanded, setExpanded] = useState(false);
  const [thinkingOpen, setThinkingOpen] = useState(false);
  const tools = blocks.map(toolBlockToCardModel);
  const label = summarizeToolCalls(tools, t);
  const thinkingCount = absorbedThinking?.length ?? 0;
  const runningCount = tools.filter((tool) => tool.status === 'running').length;
  // Chevron already signals expandability — only surface live running status.
  const hint =
    runningCount > 0
      ? t('message.toolsRunning', { count: String(runningCount) })
      : undefined;

  const useCompact =
    category === 'explore' ||
    category === 'plan' ||
    category === 'write' ||
    category === 'shell' ||
    category === 'office' ||
    category === 'workflow' ||
    category === 'agent' ||
    category === 'mixed';

  return (
    <MessageMetaBar
      icon={<IconWrench className="size-3.5" />}
      label={label}
      hint={hint}
      expanded={expanded}
      onToggle={() => setExpanded((v) => !v)}
    >
      <div className="space-y-1.5">
        {thinkingCount > 0 && absorbedThinking ? (
          <div className="rounded-md border border-card-border/60 bg-canvas-alt/40">
            <button
              type="button"
              className="flex w-full items-center gap-1.5 px-2 py-1.5 text-left text-[11px] text-t-text-muted hover:text-t-text"
              onClick={() => setThinkingOpen((v) => !v)}
              aria-expanded={thinkingOpen}
            >
              <IconSparkle className="size-3 shrink-0" />
              <span className="min-w-0 flex-1 truncate">
                {t('message.timelineAbsorbedReasoning', {
                  count: String(thinkingCount),
                })}
              </span>
              <span>{thinkingOpen ? '▾' : '▸'}</span>
            </button>
            {thinkingOpen ? (
              <div className="max-h-[4.5rem] space-y-2 overflow-y-auto border-t border-card-border/40 px-2 py-1.5 text-[11px] leading-relaxed whitespace-pre-wrap text-t-text-secondary">
                {absorbedThinking.map((th) => (
                  <div key={th.id}>{th.text.trim() || '…'}</div>
                ))}
              </div>
            ) : null}
          </div>
        ) : null}
        {blocks.map((block) => (
          <ToolBlock
            key={block.id}
            block={block}
            onOpenDiffInPanel={onOpenDiffInPanel}
            agentStates={agentStates}
            compact={useCompact}
          />
        ))}
      </div>
    </MessageMetaBar>
  );
}
