/** Turn usage slice returned by GET /v1/threads/{id} (full ThreadDetail JSON). */
export interface ThreadTurnUsage {
  input_tokens?: number;
  output_tokens?: number;
}

export interface ThreadTurnRecordLite {
  usage?: ThreadTurnUsage | null;
}

export interface ThreadDetailWithTurns {
  thread?: { model?: string };
  turns?: ThreadTurnRecordLite[];
}

export const DEFAULT_CONTEXT_WINDOW_TOKENS = 1_000_000;

/** Conservative system + tools prompt overhead when the UI cannot read runtime system blocks. */
export const DEFAULT_SYSTEM_PROMPT_OVERHEAD = 12_000;

export interface MessageForContextEstimate {
  role: string;
  content: string;
  thinking?: string;
  tools?: { input?: string; output?: string }[];
}

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

/** Rough token estimate: ~1 token per 4 ASCII chars, ~1 per 1.3 CJK chars (matches App heuristic). */
export function estimateTokensFromText(text: string): number {
  if (!text) return 0;
  let cjk = 0;
  let ascii = 0;
  for (const ch of text) {
    const code = ch.charCodeAt(0);
    if (
      (code >= 0x4e00 && code <= 0x9fff) ||
      (code >= 0x3400 && code <= 0x4dbf) ||
      (code >= 0x3000 && code <= 0x303f) ||
      (code >= 0xff00 && code <= 0xffef) ||
      (code >= 0x2e80 && code <= 0x2fdf)
    ) {
      cjk++;
    } else {
      ascii++;
    }
  }
  return Math.ceil(cjk / 1.3 + ascii / 4);
}

/**
 * Estimate current context fill from visible transcript (aligned with TUI
 * `estimate_input_tokens_conservative` + system overhead). Monotonic as the
 * conversation grows; does not sum per-round API `input_tokens`.
 */
export function estimateContextTokensFromMessages(
  messages: MessageForContextEstimate[],
  options?: { systemOverhead?: number },
): number {
  const overhead = options?.systemOverhead ?? DEFAULT_SYSTEM_PROMPT_OVERHEAD;
  let raw = 0;
  for (const m of messages) {
    raw += estimateTokensFromText(m.content);
    if (m.thinking) {
      raw += estimateTokensFromText(m.thinking);
    }
    for (const t of m.tools ?? []) {
      if (t.input) raw += estimateTokensFromText(t.input);
      if (t.output) raw += estimateTokensFromText(t.output);
    }
  }
  const conservative = Math.ceil((raw * 3) / 2);
  const framing = messages.length * 12 + 48;
  return conservative + framing + overhead;
}

/**
 * @deprecated Summing turn `input_tokens` inflates context % (each turn sums every
 * API round in that turn). Kept only as a capped fallback when the transcript is empty.
 */
export function maxTurnInputTokensFallback(detail: ThreadDetailWithTurns): number {
  let max = 0;
  for (const turn of detail.turns ?? []) {
    const input = turn.usage?.input_tokens;
    if (input != null && Number.isFinite(input) && input > max) {
      max = input;
    }
  }
  return max;
}

export function contextUsagePercent(
  usedTokens: number,
  pendingTokens: number,
  contextWindow: number,
): number {
  if (contextWindow <= 0) {
    return 0;
  }
  return Math.min(100, ((usedTokens + pendingTokens) / contextWindow) * 100);
}

/** Pick context used: transcript estimate first; avoid summing persisted turn usage. */
export function resolveContextUsedTokens(
  messages: MessageForContextEstimate[],
  threadDetail: ThreadDetailWithTurns | null | undefined,
  contextWindow: number,
): number {
  const fromMessages = estimateContextTokensFromMessages(messages);
  if (messages.length > 0) {
    return Math.min(fromMessages, contextWindow);
  }
  if (threadDetail) {
    return Math.min(maxTurnInputTokensFallback(threadDetail), contextWindow);
  }
  return Math.min(DEFAULT_SYSTEM_PROMPT_OVERHEAD, contextWindow);
}
