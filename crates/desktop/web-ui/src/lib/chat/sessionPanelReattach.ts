import type { Dispatch, SetStateAction } from 'react';
import type { PanelSlice } from '../../hooks/useStreamContextRegistry';
import type { ThreadContextSnapshot } from '../contextUsage';
import type { LhtChipState } from '../lhtChip';
import {
  dispatchPanelChecklist,
  dispatchPanelContext,
  dispatchPanelScratchpad,
  dispatchPanelTaskGraph,
  type ChecklistPanelPayload,
} from '../panelChannel';

/**
 * Restore a background thread's panel slice into the active UI (multi-session P0.6).
 *
 * `threadId` is the thread being reattached (should already be the active view).
 * It is forwarded to the dispatchers so the panelChannel guard can verify the
 * event belongs to the active thread.
 */
export function restorePanelSliceToUi(
  slice: PanelSlice,
  setLhtChip: Dispatch<SetStateAction<LhtChipState | null>>,
  applyThreadContextSnapshot?: (threadId: string, snapshot: ThreadContextSnapshot) => void,
  threadId?: string,
): void {
  if (slice.checklist) {
    dispatchPanelChecklist(slice.checklist as ChecklistPanelPayload, threadId);
  }
  if (slice.taskGraph) {
    dispatchPanelTaskGraph(slice.taskGraph, threadId);
  }
  if (slice.scratchpad) {
    dispatchPanelScratchpad(slice.scratchpad, threadId);
  }
  if (slice.context) {
    if (threadId && applyThreadContextSnapshot) {
      applyThreadContextSnapshot(threadId, slice.context);
    }
    dispatchPanelContext(slice.context, threadId);
  }
  setLhtChip(slice.lhtChip);
}
