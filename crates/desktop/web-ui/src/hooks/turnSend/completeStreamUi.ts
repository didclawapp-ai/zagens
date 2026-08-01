import type { TurnChatMessage } from '../useTurnSend';
import { lastAssistantMessageId } from '../../lib/chat/activeTurnStreamUi';
import { finalizeInactiveAssistants } from '../../lib/chat/finalizeInactiveAssistants';
import { isNearDuplicateProse } from '../../lib/chat/formatAssistantContent';
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
      // Live may have coalesced several captions into one longer block while the
      // replay spine still emits one text per agent_message. Pushing those would
      // re-render the same prose as the turn grows (multi-tool / large threads).
      if (textAlreadyRepresented(merged, block.content)) {
        continue;
      }
    }
    if (block.kind === 'thinking' && thinkingAlreadyRepresented(merged, block.text)) {
      continue;
    }
    merged.push(block);
  }
  return merged;
}

function textAlreadyRepresented(merged: TurnBlock[], content: string): boolean {
  const next = content.trim();
  if (!next) return true;
  for (const b of merged) {
    if (b.kind !== 'text') continue;
    const cur = b.content.trim();
    if (!cur) continue;
    // Incoming already covered by a live block (no length floor — short Chinese
    // captions are often <12 chars). Do NOT treat "next contains cur" alone as
    // represented: that means the replay is richer and should still enrich/push.
    if (cur === next || cur.includes(next)) return true;
    if (proseOverlaps(cur, next) || isNearDuplicateProse(cur, next)) return true;
  }
  return false;
}

function thinkingAlreadyRepresented(merged: TurnBlock[], text: string): boolean {
  const next = text.trim();
  if (!next) return true;
  for (const b of merged) {
    if (b.kind !== 'thinking') continue;
    const cur = b.text.trim();
    if (!cur) continue;
    if (cur === next || cur.includes(next)) return true;
    if (proseOverlaps(cur, next) || isNearDuplicateProse(cur, next)) return true;
  }
  return false;
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
    // Keep the live streaming flag. Recovery/transcript merges mid-turn must
    // not flip the bubble into settled layout (or look like a wholesale replace).
    isStreaming: Boolean(live.isStreaming),
    ...(thinkingIncomplete
      ? { thinkingIncomplete: true }
      : { thinkingIncomplete: undefined }),
  };
}

function userMessageCount(messages: TurnChatMessage[]): number {
  return messages.reduce((n, m) => (m.role === 'user' ? n + 1 : n), 0);
}

function assistantMessageCount(messages: TurnChatMessage[]): number {
  return messages.reduce((n, m) => (m.role === 'assistant' ? n + 1 : n), 0);
}

function assistantProse(message: TurnChatMessage): string {
  const direct = message.content?.trim() ?? '';
  if (direct) return direct;
  return (message.blocks ?? [])
    .filter((b): b is Extract<TurnBlock, { kind: 'text' }> => b.kind === 'text')
    .map((b) => b.content)
    .join('\n')
    .trim();
}

export function reconcileMessagesFromThread(
  live: TurnChatMessage[],
  persisted: TurnChatMessage[],
): TurnChatMessage[] {
  if (persisted.length === 0) return live;
  // Stale-snapshot guard: when the persisted replay predates an in-flight turn
  // (live already has the next user prompt the snapshot lacks), the last
  // persisted assistant belongs to the PREVIOUS turn. Merging it into live's
  // fresh streaming bubble would re-render the prior turn's output (duplicate
  // stream on the 2nd prompt of a session). Keep live; a fresh rebuild runs
  // when the in-flight turn completes.
  if (userMessageCount(persisted) < userMessageCount(live)) {
    return live;
  }
  // Equal user counts are not enough: replay often has user C persisted while
  // assistant C has no items yet (`pushAssistantFromBlocks` skips empty). The
  // last persisted assistant is still turn B — merging would REPLACE streaming
  // output C with completed output B (the "C → D replaced C" symptom).
  if (assistantMessageCount(persisted) < assistantMessageCount(live)) {
    return live;
  }

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

  // Belt-and-suspenders: even with matching assistant counts, never clobber an
  // in-flight bubble with unrelated completed prose (wrong-turn / wrong-bubble).
  if (liveAssistant.isStreaming) {
    const liveText = assistantProse(liveAssistant);
    const persText = assistantProse(persistedAssistant);
    if (
      liveText.length > 0 &&
      persText.length > 0 &&
      !proseOverlaps(liveText, persText) &&
      !isNearDuplicateProse(liveText, persText)
    ) {
      return live;
    }
  }

  const prefix =
    persisted.length > live.length ? persisted.slice(0, persistedAssistantIdx) : live.slice(0, liveAssistantIdx);

  const reconciledAssistant = reconcileAssistantTurn(liveAssistant, persistedAssistant);
  const suffix = persisted.slice(persistedAssistantIdx + 1);
  const merged = [...prefix, reconciledAssistant, ...suffix];
  // Keep in-flight blocks only on the latest assistant — older bubbles with
  // stale `running` tools otherwise render a second「生成中」live frame.
  return finalizeInactiveAssistants(merged, reconciledAssistant.id);
}

/** Merge live registry transcript with authoritative thread replay (P1). */
export function mergeThreadTranscript(
  live: TurnChatMessage[],
  rebuilt: TurnChatMessage[],
): TurnChatMessage[] {
  if (rebuilt.length === 0) return live;
  if (live.length === 0) {
    return finalizeInactiveAssistants(rebuilt, lastAssistantMessageId(rebuilt));
  }
  return reconcileMessagesFromThread(live, rebuilt);
}
