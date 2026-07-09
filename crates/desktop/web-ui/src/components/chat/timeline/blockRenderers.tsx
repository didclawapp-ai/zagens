import type { ComponentType } from 'react';
import type { TurnBlock, TurnBlockKind } from '../../../lib/chat/timeline/turnBlockTypes';
import type { AgentState } from '../../../types/agent';
import { ThinkingBlock } from './blocks/ThinkingBlock';
import { ToolBlock } from './blocks/ToolBlock';
import { TextBlock } from './blocks/TextBlock';

export type BlockRendererContext = {
  isTurnStreaming: boolean;
  workspaceRoot?: string;
  desktopHost?: boolean;
  onOpenWorkspacePath: (relPath: string) => void | Promise<void>;
  onRevealWorkspacePath?: (relPath: string) => void;
  onOpenDiffInPanel?: () => void;
  agentStates?: AgentState[];
};

type BlockRenderer = ComponentType<{ block: TurnBlock; ctx: BlockRendererContext }>;

function ThinkingRenderer({ block, ctx }: { block: TurnBlock; ctx: BlockRendererContext }) {
  if (block.kind !== 'thinking') return null;
  return <ThinkingBlock block={block} isTurnStreaming={ctx.isTurnStreaming} />;
}

function ToolRenderer({ block, ctx }: { block: TurnBlock; ctx: BlockRendererContext }) {
  if (block.kind !== 'tool') return null;
  return (
    <ToolBlock
      block={block}
      onOpenDiffInPanel={ctx.onOpenDiffInPanel}
      agentStates={ctx.agentStates}
    />
  );
}

function TextRenderer({ block, ctx }: { block: TurnBlock; ctx: BlockRendererContext }) {
  if (block.kind !== 'text') return null;
  return (
    <TextBlock
      block={block}
      workspaceRoot={ctx.workspaceRoot}
      desktopHost={ctx.desktopHost}
      isTurnStreaming={ctx.isTurnStreaming}
      onOpenWorkspacePath={ctx.onOpenWorkspacePath}
      onRevealWorkspacePath={ctx.onRevealWorkspacePath}
    />
  );
}

export const turnBlockRenderers: Record<TurnBlockKind, BlockRenderer> = {
  thinking: ThinkingRenderer,
  tool: ToolRenderer,
  text: TextRenderer,
};

export function renderTurnBlock(block: TurnBlock, ctx: BlockRendererContext) {
  const Renderer = turnBlockRenderers[block.kind];
  return (
    <div key={block.id} data-timeline-block={block.id}>
      <Renderer block={block} ctx={ctx} />
    </div>
  );
}
