import { threadTurnStillActive } from '../../api/client';
import { markLastAssistantStreaming } from './activeTurnStreamUi';
import { restorePanelSliceToUi } from './sessionPanelReattach';
import { sessionMessageRichness } from './sessionMessagePick';
import { getActiveThreadIdsFromStore } from './threadStatusStore';
import type { TurnChatMessage } from '../../hooks/useTurnSend';
import type { StreamContextRegistry } from '../../hooks/useStreamContextRegistry';
import type { ApprovalState } from '../../hooks/useTurnApproval';
import type { ThreadContextSnapshot } from '../contextUsage';
import type { LhtChipState } from '../lhtChip';
import type { Dispatch, SetStateAction } from 'react';

export type StreamingReattachResult = {
  messages: TurnChatMessage[];
  composerLocked: boolean;
  pendingApproval: ApprovalState | null;
};

/**
 * After navigating to a thread, restore live-stream UI if the turn is still
 * running in the background (multi-session P0.4 reattach).
 */
export async function applyStreamingReattach(
  threadId: string,
  messages: TurnChatMessage[],
  options: {
    streamRegistry?: StreamContextRegistry | null;
    setLhtChip?: Dispatch<SetStateAction<LhtChipState | null>>;
    applyThreadContextSnapshot?: (
      threadId: string,
      snapshot: ThreadContextSnapshot,
    ) => void;
  },
): Promise<StreamingReattachResult> {
  const tid = threadId.trim();
  if (!tid) {
    return { messages, composerLocked: false, pendingApproval: null };
  }

  const inStore = getActiveThreadIdsFromStore().has(tid);
  const ctx = options.streamRegistry?.getContext(tid);
  const turnId = ctx?.threadTurn.turnId || undefined;
  let stillActive = false;
  try {
    stillActive = await threadTurnStillActive(tid, turnId);
  } catch {
    stillActive = false;
  }

  if (!stillActive) {
    options.streamRegistry?.patchContext(tid, {
      isStreaming: false,
      pendingApproval: null,
    });
    return { messages, composerLocked: false, pendingApproval: null };
  }

  const pendingApproval = ctx?.pendingApproval ?? null;

  // Prefer in-memory registry transcript when background SSE kept it richer
  // than the rebuild/cache snapshot (thr_436d).
  const live = (ctx?.messages ?? []) as TurnChatMessage[];
  const base =
    live.length > 0 && sessionMessageRichness(live) >= sessionMessageRichness(messages)
      ? live
      : messages;

  const { messages: marked } = markLastAssistantStreaming(base);
  if (options.streamRegistry) {
    options.streamRegistry.ensureContext(tid, ctx?.sessionId ?? null);
    options.streamRegistry.patchContext(tid, {
      messages: marked,
      isStreaming: true,
      // Keep existing turnId — never wipe it on reattach.
      ...(turnId
        ? { threadTurn: { threadId: tid, turnId } }
        : {}),
    });
  }

  const panelSlice = ctx?.panelSlice;
  if (
    panelSlice &&
    (panelSlice.checklist ||
      panelSlice.taskGraph ||
      panelSlice.context ||
      panelSlice.scratchpad ||
      panelSlice.lhtChip) &&
    options.setLhtChip
  ) {
    restorePanelSliceToUi(
      panelSlice,
      options.setLhtChip,
      options.applyThreadContextSnapshot,
      tid,
    );
  }

  return {
    messages: marked as TurnChatMessage[],
    composerLocked: inStore || stillActive,
    pendingApproval,
  };
}
