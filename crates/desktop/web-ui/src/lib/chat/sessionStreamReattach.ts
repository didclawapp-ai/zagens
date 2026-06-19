import { threadTurnStillActive } from '../../api/client';
import { markLastAssistantStreaming } from './activeTurnStreamUi';
import { restorePanelSliceToUi } from './sessionPanelReattach';
import { removeThreadFromStreamingSet } from './streamContextStore';
import type { TurnChatMessage } from '../../hooks/useTurnSend';
import type { StreamContextRegistry } from '../../hooks/useStreamContextRegistry';
import type { ApprovalState } from '../../hooks/useTurnApproval';
import type { ThreadContextSnapshot } from '../contextUsage';
import type { LhtChipState } from '../lhtChip';
import type { MutableRefObject, Dispatch, SetStateAction } from 'react';

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
    streamingThreadIdsRef: MutableRefObject<Set<string>>;
    streamRegistry?: StreamContextRegistry | null;
    setStreamingThreadIds?: Dispatch<SetStateAction<Set<string>>>;
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

  const inSet = options.streamingThreadIdsRef.current.has(tid);
  const ctx = options.streamRegistry?.getContext(tid);
  let stillActive = false;
  try {
    stillActive = await threadTurnStillActive(tid, ctx?.threadTurn.turnId || undefined);
  } catch {
    stillActive = false;
  }

  if (!stillActive) {
    options.streamRegistry?.patchContext(tid, {
      isStreaming: false,
      pendingApproval: null,
    });
    if (inSet && options.setStreamingThreadIds) {
      options.setStreamingThreadIds((prev) => {
        const next = removeThreadFromStreamingSet(prev, tid);
        return next ?? prev;
      });
    }
    return { messages, composerLocked: false, pendingApproval: null };
  }

  const pendingApproval = ctx?.pendingApproval ?? null;

  if (!inSet && options.setStreamingThreadIds) {
    options.setStreamingThreadIds((prev) => new Set(prev).add(tid));
  }

  const { messages: marked } = markLastAssistantStreaming(messages);
  if (options.streamRegistry) {
    options.streamRegistry.ensureContext(tid, ctx?.sessionId ?? null);
    options.streamRegistry.patchContext(tid, {
      messages: marked,
      isStreaming: true,
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
    composerLocked: true,
    pendingApproval,
  };
}
