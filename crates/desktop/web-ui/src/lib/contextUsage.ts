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

import { isDeepSeekV4Model } from './modelParams';

export const DEFAULT_CONTEXT_WINDOW_TOKENS = 128_000;
export const DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS = 1_000_000;

/** Conservative system + tools prompt overhead when the UI cannot read runtime system blocks. */
export const DEFAULT_SYSTEM_PROMPT_OVERHEAD = 12_000;

export interface MessageForContextEstimate {
  role: string;
  content: string;
  thinking?: string;
  tools?: { input?: string; output?: string }[];
}

/** Runtime-aligned context snapshot (`GET /v1/threads/{id}/context`). */
export interface ThreadContextSnapshot {
  estimated_input_tokens: number;
  context_window_tokens: number;
  usage_percent: number;
  message_count: number;
  compaction_enabled: boolean;
  compaction_threshold_tokens: number;
  compaction_floor_tokens: number;
  should_compact: boolean;
  /** Provider `usage.input_tokens` from the last API round (authoritative). */
  last_api_input_tokens?: number | null;
  /** Percent from `last_api_input_tokens` when present. */
  last_api_usage_percent?: number | null;
  /** Deprecated: last turn's summed `usage.input_tokens` (multi-round turns inflate). */
  last_reported_input_tokens?: number | null;
  source: string;
}

export function contextWindowTokensForModel(model: string | undefined): number {
  const lower = (model ?? '').toLowerCase();
  if (isDeepSeekV4Model(model ?? '')) {
    return DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS;
  }
  if (lower.includes('claude')) {
    return 200_000;
  }
  if (lower.includes('deepseek')) {
    return 128_000;
  }
  if (
    lower.includes('ollama') ||
    lower.includes('agnes') ||
    lower.includes('sensenova')
  ) {
    return 8192;
  }
  return DEFAULT_CONTEXT_WINDOW_TOKENS;
}

/** DeepSeek doc heuristic: ~0.3 token/ASCII char, ~0.6 token/CJK char. */
export function estimateTokensFromText(text: string): number {
  if (!text) return 0;
  let cjk = 0;
  let other = 0;
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
      other++;
    }
  }
  return Math.ceil((other * 3) / 10 + (cjk * 6) / 10);
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

/** Pick context used: runtime snapshot when populated, else transcript / turn fallback. */
export function resolveContextUsedTokens(
  messages: MessageForContextEstimate[],
  threadDetail: ThreadDetailWithTurns | null | undefined,
  contextWindow: number,
  runtimeSnapshot?: ThreadContextSnapshot | null,
): number {
  const fromMessages =
    messages.length > 0
      ? Math.min(estimateContextTokensFromMessages(messages), contextWindow)
      : 0;

  if (runtimeSnapshot) {
    const fromRuntime = Math.min(runtimeSnapshot.estimated_input_tokens, contextWindow);
    // After session switch the runtime may briefly return an empty engine/store
    // snapshot while the UI transcript is already restored from cache.
    if (fromRuntime > 0 || runtimeSnapshot.message_count > 0) {
      return Math.max(fromRuntime, fromMessages);
    }
    if (fromMessages > 0) {
      return fromMessages;
    }
    return fromRuntime;
  }
  if (fromMessages > 0) {
    return fromMessages;
  }
  if (threadDetail) {
    return Math.min(maxTurnInputTokensFallback(threadDetail), contextWindow);
  }
  return Math.min(DEFAULT_SYSTEM_PROMPT_OVERHEAD, contextWindow);
}

export function resolveContextUsagePercent(
  usedTokens: number,
  contextWindow: number,
  runtimeSnapshot?: ThreadContextSnapshot | null,
): number {
  const fromUsed = contextUsagePercent(usedTokens, 0, contextWindow);
  if (!runtimeSnapshot) {
    return fromUsed;
  }
  const snapPct = runtimeSnapshot.usage_percent;
  if (
    snapPct > 0 &&
    runtimeSnapshot.estimated_input_tokens > 0 &&
    runtimeSnapshot.message_count > 0
  ) {
    return snapPct;
  }
  return fromUsed;
}
