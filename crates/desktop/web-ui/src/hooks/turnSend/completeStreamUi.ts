import type { TurnChatMessage } from '../useTurnSend';
import {
  blocksToLegacyFields,
  legacyFieldsToBlocks,
} from '../../lib/chat/timeline/legacyMessageAdapter';
import type { TurnBlock } from '../../lib/chat/timeline/turnBlockTypes';

function blockFieldRichness(block: TurnBlock): number {
  switch (block.kind) {
    case 'thinking':
      return block.text.length;
    case 'tool':
      return (block.output?.length ?? 0) + block.input.length + block.name.length;
    case 'text':
      return block.content.length;
    default:
      return 0;
  }
}

function blocksRichness(blocks: TurnBlock[]): number {
  return blocks.reduce((sum, block) => sum + blockFieldRichness(block), 0);
}

function patchToolBlock(
  existing: Extract<TurnBlock, { kind: 'tool' }>,
  incoming: Extract<TurnBlock, { kind: 'tool' }>,
): Extract<TurnBlock, { kind: 'tool' }> {
  return {
    ...existing,
    name: incoming.name || existing.name,
    input: incoming.input || existing.input,
    output:
      (incoming.output?.length ?? 0) > (existing.output?.length ?? 0)
        ? incoming.output
        : existing.output,
    status: incoming.status === 'running' ? existing.status : incoming.status,
  };
}

/** True when two prose blobs are the same segment (equal / prefix / containment). */
function proseOverlaps(a: string, b: string): boolean {
  const cur = a.trim();
  const next = b.trim();
  if (!cur || !next) return false;
  if (cur === next) return true;
  if (next.startsWith(cur) || cur.startsWith(next)) return true;
  if (cur.endsWith(next) || next.endsWith(cur)) return true;
  if (next.length >= 12 && cur.includes(next)) return true;
  if (cur.length >= 12 && next.includes(cur)) return true;
  return false;
}

function findMatchingTextIndex(
  merged: TurnBlock[],
  incoming: Extract<TurnBlock, { kind: 'text' }>,
): number {
  if (incoming.itemId) {
    const byItem = merged.findIndex(
      (b) => b.kind === 'text' && b.itemId && b.itemId === incoming.itemId,
    );
    if (byItem >= 0) return byItem;
  }
  // Prefer the longest overlapping live text (final report over short captions).
  let best = -1;
  let bestLen = -1;
  for (let i = 0; i < merged.length; i++) {
    const b = merged[i];
    if (b.kind !== 'text') continue;
    if (!proseOverlaps(b.content, incoming.content)) continue;
    if (b.content.length > bestLen) {
      best = i;
      bestLen = b.content.length;
    }
  }
  return best;
}

function findMatchingThinkingIndex(
  merged: TurnBlock[],
  incoming: Extract<TurnBlock, { kind: 'thinking' }>,
): number {
  let best = -1;
  let bestLen = -1;
  for (let i = 0; i < merged.length; i++) {
    const b = merged[i];
    if (b.kind !== 'thinking') continue;
    if (!proseOverlaps(b.text, incoming.text)) continue;
    if (b.text.length > bestLen) {
      best = i;
      bestLen = b.text.length;
    }
  }
  return best;
}

/** Patch-only merge: keep live order/ids; enrich from persisted snapshot blocks. */
export function reconcileAssistantBlocks(
  liveBlocks: TurnBlock[],
  persisted: Pick<TurnChatMessage, 'blocks' | 'thinking' | 'tools' | 'content'>,
): TurnBlock[] {
  const persistedBlocks =
    persisted.blocks?.length && persisted.blocks.length > 0
      ? persisted.blocks
      : legacyFieldsToBlocks(persisted, 'persisted');

  if (blocksRichness(persistedBlocks) <= blocksRichness(liveBlocks)) {
    return liveBlocks;
  }

  const merged: TurnBlock[] = [...liveBlocks];
  for (const block of persistedBlocks) {
    if (block.kind === 'tool') {
      const idx = merged.findIndex((b) => b.kind === 'tool' && b.id === block.id);
      if (idx >= 0 && merged[idx].kind === 'tool') {
        merged[idx] = patchToolBlock(merged[idx], block);
        continue;
      }
    }
    if (block.kind === 'thinking') {
      const idx = findMatchingThinkingIndex(merged, block);
      if (idx >= 0 && merged[idx].kind === 'thinking') {
        const existing = merged[idx];
        if (block.text.length > existing.text.length) {
          merged[idx] = { ...existing, text: block.text, streaming: false, status: 'done' };
        }
        continue;
      }
    }
    if (block.kind === 'text') {
      const idx = findMatchingTextIndex(merged, block);
      if (idx >= 0 && merged[idx].kind === 'text') {
        const existing = merged[idx];
        if (block.content.length > existing.content.length) {
          merged[idx] = { ...existing, content: block.content, streaming: false };
        }
        // Equal / shorter / overlapping: keep live text — never push a duplicate.
        continue;
      }
    }
    merged.push(block);
  }
  return merged;
}

export function reconcileAssistantTurn(
  live: TurnChatMessage,
  persisted: TurnChatMessage,
): TurnChatMessage {
  const liveBlocks = live.blocks ?? legacyFieldsToBlocks(live, live.id);
  const nextBlocks = reconcileAssistantBlocks(liveBlocks, persisted);
  const legacy = blocksToLegacyFields(nextBlocks);
  const thinkingIncomplete =
    Boolean(persisted.thinkingIncomplete) &&
    !nextBlocks.some((b) => b.kind === 'thinking');
  return {
    ...live,
    blocks: nextBlocks,
    content: legacy.content || live.content,
    thinking: legacy.thinking ?? live.thinking,
    tools: legacy.tools ?? live.tools,
    isStreaming: false,
    ...(thinkingIncomplete
      ? { thinkingIncomplete: true }
      : { thinkingIncomplete: undefined }),
  };
}

export function reconcileMessagesFromThread(
  live: TurnChatMessage[],
  persisted: TurnChatMessage[],
): TurnChatMessage[] {
  if (persisted.length === 0) return live;

  const lastLiveAssistantIdx = [...live].reverse().findIndex((m) => m.role === 'assistant');
  const lastPersistedAssistantIdx = [...persisted]
    .reverse()
    .findIndex((m) => m.role === 'assistant');

  if (lastLiveAssistantIdx < 0 || lastPersistedAssistantIdx < 0) {
    return persisted.length >= live.length ? persisted : live;
  }

  const liveAssistantIdx = live.length - 1 - lastLiveAssistantIdx;
  const persistedAssistantIdx = persisted.length - 1 - lastPersistedAssistantIdx;
  const liveAssistant = live[liveAssistantIdx];
  const persistedAssistant = persisted[persistedAssistantIdx];

  const prefix =
    persisted.length > live.length ? persisted.slice(0, persistedAssistantIdx) : live.slice(0, liveAssistantIdx);

  const reconciledAssistant = reconcileAssistantTurn(liveAssistant, persistedAssistant);
  const suffix = persisted.slice(persistedAssistantIdx + 1);

  return [...prefix, reconciledAssistant, ...suffix];
}

/** Merge live registry transcript with authoritative thread replay (P1). */
export function mergeThreadTranscript(
  live: TurnChatMessage[],
  rebuilt: TurnChatMessage[],
): TurnChatMessage[] {
  if (rebuilt.length === 0) return live;
  if (live.length === 0) return rebuilt;
  return reconcileMessagesFromThread(live, rebuilt);
}
