const ANSI_CSI = /\x1B\[/;

import TerminalCard from '../../../TerminalCard';
import DiffCard from '../../../DiffCard';
import { AgentSpawnInline } from '../../../AgentSpawnInline';
import { ToolCard, type ToolCardModel } from '../../../ToolCard';
import { extractUnifiedDiff, parseFileNameFromToolInput } from '../../../../lib/diff/diffEntries';
import { parseAgentIdFromSpawnOutput } from '../../../../lib/chat/toolOutput';
import { isAgentSpawnToolName } from '../../../../lib/agentSpawnMeta';
import type { AgentState } from '../../../../types/agent';
import type { TurnBlock } from '../../../../lib/chat/timeline/turnBlockTypes';

function tryParseCommand(input: string): string | null {
  try {
    const parsed = JSON.parse(input) as { command?: string };
    return typeof parsed.command === 'string' ? parsed.command : null;
  } catch {
    return null;
  }
}

export function toolBlockToCardModel(block: Extract<TurnBlock, { kind: 'tool' }>): ToolCardModel {
  return {
    id: block.id,
    name: block.name,
    input: block.input,
    output: block.output,
    status:
      block.status === 'running'
        ? 'running'
        : block.status === 'error'
          ? 'error'
          : 'done',
  };
}

export function renderToolBlockCard(
  tool: ToolCardModel,
  onOpenDiffInPanel?: () => void,
  copyToolTitle?: string,
  agentStates?: AgentState[],
) {
  const outputHasAnsi = Boolean(tool.output && ANSI_CSI.test(tool.output));

  if (
    tool.name === 'exec_shell' ||
    tool.name === 'task_shell_start' ||
    tool.name === 'task_shell_wait' ||
    outputHasAnsi
  ) {
    return (
      <TerminalCard
        key={tool.id}
        output={tool.output ?? ''}
        command={tryParseCommand(tool.input) ?? tool.name}
        status={tool.status}
      />
    );
  }

  if (
    tool.name === 'edit_file' ||
    tool.name === 'apply_patch' ||
    tool.name === 'write_file'
  ) {
    const diffText = extractUnifiedDiff(tool.output ?? '');
    const fileName = parseFileNameFromToolInput(tool.input);

    if (diffText) {
      return (
        <DiffCard
          key={tool.id}
          diffText={diffText}
          fileName={fileName}
          onOpenInPanel={onOpenDiffInPanel}
        />
      );
    }
  }

  if (isAgentSpawnToolName(tool.name)) {
    const agentId = parseAgentIdFromSpawnOutput(tool.output ?? '');
    const linkedAgent =
      agentId != null ? agentStates?.find((a) => a.agentId === agentId) : undefined;
    return (
      <div key={tool.id}>
        <ToolCard tool={tool} copyTitle={copyToolTitle} />
        {linkedAgent ? <AgentSpawnInline agent={linkedAgent} /> : null}
      </div>
    );
  }

  return <ToolCard key={tool.id} tool={tool} copyTitle={copyToolTitle} />;
}
