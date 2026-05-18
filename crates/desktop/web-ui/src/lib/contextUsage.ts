/** Turn usage slice returned by GET /v1/threads/{id} (full ThreadDetail JSON). */
export interface ThreadTurnUsage {
  input_tokens?: number;
}

export interface ThreadTurnRecordLite {
  usage?: ThreadTurnUsage | null;
}

export interface ThreadDetailWithTurns {
  thread?: { model?: string };
  turns?: ThreadTurnRecordLite[];
}

export const DEFAULT_CONTEXT_WINDOW_TOKENS = 1_000_000;

export function contextWindowTokensForModel(model: string | undefined): number {
  const lower = (model ?? '').toLowerCase();
  if (lower.includes('claude')) {
    return 200_000;
  }
  if (lower.includes('deepseek') && lower.includes('v4')) {
    return 1_000_000;
  }
  if (lower.includes('deepseek')) {
    return 128_000;
  }
  return DEFAULT_CONTEXT_WINDOW_TOKENS;
}

/** Sum persisted turn input_tokens — same basis as turn_completed accumulation in App. */
export function sumThreadInputTokens(detail: ThreadDetailWithTurns): number {
  let sum = 0;
  for (const turn of detail.turns ?? []) {
    const input = turn.usage?.input_tokens;
    if (input != null && Number.isFinite(input) && input > 0) {
      sum += input;
    }
  }
  return sum;
}

export function contextUsagePercent(
  inputTokens: number,
  pendingTokens: number,
  contextWindow: number,
): number {
  if (contextWindow <= 0) {
    return 0;
  }
  return Math.min(100, ((inputTokens + pendingTokens) / contextWindow) * 100);
}
