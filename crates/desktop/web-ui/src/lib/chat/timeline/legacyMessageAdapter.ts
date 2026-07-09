import type { TurnChatMessage } from '../../../hooks/useTurnSend';
import type { UiMessage, UiToolCall } from '../sessionMessages';
import type { TurnBlock } from './turnBlockTypes';

type LegacyFields = {
  thinking?: string;
  tools?: UiToolCall[];
  content: string;
};

export function blocksToLegacyFields(blocks: TurnBlock[]): LegacyFields {
  const thinkingParts: string[] = [];
  const tools: UiToolCall[] = [];
  const textParts: string[] = [];

  for (const block of blocks) {
    switch (block.kind) {
      case 'thinking':
        if (block.text.trim()) thinkingParts.push(block.text);
        break;
      case 'tool':
        tools.push({
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
        });
        break;
      case 'text':
        if (block.content.trim()) textParts.push(block.content);
        break;
      default:
        break;
    }
  }

  return {
    content: textParts.join('\n\n'),
    thinking: thinkingParts.length > 0 ? thinkingParts.join('\n\n') : undefined,
    tools: tools.length > 0 ? tools : undefined,
  };
}

/**
 * Cold-load fallback: thinking → tools → text (does not restore interleaved order).
 * Use only for copy/export or sessions without persisted blocks/events.
 */
export function legacyFieldsToBlocks(
  fields: Pick<TurnChatMessage, 'thinking' | 'tools' | 'content'>,
  messageId: string,
): TurnBlock[] {
  const blocks: TurnBlock[] = [];
  if (fields.thinking?.trim()) {
    blocks.push({
      kind: 'thinking',
      id: `${messageId}-think`,
      text: fields.thinking,
      streaming: false,
      status: 'done',
    });
  }
  for (const tool of fields.tools ?? []) {
    blocks.push({
      kind: 'tool',
      id: tool.id,
      name: tool.name,
      input: tool.input,
      output: tool.output,
      status: tool.status,
    });
  }
  if (fields.content.trim()) {
    blocks.push({
      kind: 'text',
      id: `${messageId}-text`,
      content: fields.content,
      streaming: false,
    });
  }
  return blocks;
}

/** True when rendering must fall back to the degenerate legacy stack order. */
export function usesDegenerateLegacyLayout(message: {
  blocks?: TurnBlock[];
  thinking?: string;
  tools?: UiToolCall[];
  content: string;
}): boolean {
  if (message.blocks && message.blocks.length > 0) {
    return false;
  }
  return Boolean(
    message.thinking?.trim() ||
      (message.tools && message.tools.length > 0) ||
      message.content.trim(),
  );
}

export function blocksToUiMessage(
  blocks: TurnBlock[],
  base: Pick<UiMessage, 'id' | 'role'>,
): UiMessage {
  const legacy = blocksToLegacyFields(blocks);
  return {
    ...base,
    content: legacy.content,
    thinking: legacy.thinking,
    tools: legacy.tools,
    blocks,
  };
}
