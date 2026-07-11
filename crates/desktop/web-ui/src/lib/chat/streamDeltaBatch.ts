/** Batch high-frequency thinking/message deltas before React setState (~1 frame). */
export const STREAM_DELTA_BATCH_MS = 24;

export type BatchedDeltaKind = 'thinking_delta' | 'message_delta';

export type StreamDeltaBatcher = {
  push: (kind: BatchedDeltaKind, content: string) => void;
  /** Flush pending deltas immediately (call before non-delta timeline events). */
  flush: () => void;
};

type ScheduleFn = (fn: () => void, ms: number) => ReturnType<typeof setTimeout>;
type ClearFn = (id: ReturnType<typeof setTimeout>) => void;

/**
 * Coalesce consecutive thinking/message deltas into one flush per window.
 * Preserves order: thinking buffer is flushed before message buffer.
 */
export function createStreamDeltaBatcher(
  onFlush: (kind: BatchedDeltaKind, content: string) => void,
  options?: {
    windowMs?: number;
    schedule?: ScheduleFn;
    clearSchedule?: ClearFn;
  },
): StreamDeltaBatcher {
  const windowMs = options?.windowMs ?? STREAM_DELTA_BATCH_MS;
  const schedule = options?.schedule ?? setTimeout;
  const clearSchedule = options?.clearSchedule ?? clearTimeout;

  let pendingThinking = '';
  let pendingMessage = '';
  let timer: ReturnType<typeof setTimeout> | null = null;

  const flush = () => {
    if (timer != null) {
      clearSchedule(timer);
      timer = null;
    }
    if (pendingThinking) {
      const chunk = pendingThinking;
      pendingThinking = '';
      onFlush('thinking_delta', chunk);
    }
    if (pendingMessage) {
      const chunk = pendingMessage;
      pendingMessage = '';
      onFlush('message_delta', chunk);
    }
  };

  const push = (kind: BatchedDeltaKind, content: string) => {
    if (!content) return;
    if (kind === 'thinking_delta') {
      pendingThinking += content;
    } else {
      pendingMessage += content;
    }
    if (timer == null) {
      timer = schedule(flush, windowMs);
    }
  };

  return { push, flush };
}
