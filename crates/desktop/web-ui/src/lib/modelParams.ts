import type { ModelParams } from '../components/ModelParamsDialog';

export const MODEL_PARAMS_STORAGE_KEY = 'zagens-desktop-model-params';

/**
 * DeepSeek V4 (`v4-flash` / `v4-pro`) official max output length is **384K**
 * (= 384 × 1024 = 393216 tokens; context window 1M). V4 defaults to **thinking
 * mode**, where `max_tokens` budgets the reasoning (`reasoning_content`) AND the
 * answer together — so a reasoning-heavy coding turn on the old 8192 default
 * burned the whole budget on chain-of-thought and got cut off
 * (`finish_reason=length`) before emitting any answer (silent empty-body
 * truncation). We default to the model's full ceiling so reasoning never starves
 * the answer; `max_tokens` is an upper bound, not a reservation — the model
 * stops when done, so this only removes the premature cap.
 *
 * Other third-party models (OpenRouter, SenseNova `sensenova-*`, Agnes `agnes-*`, …)
 * share a unified 64K cap unless the provider catalog publishes a lower limit.
 */
export const MODEL_MAX_TOKENS = 384 * 1024; // 393216 — DeepSeek V4 official cap
export const THIRD_PARTY_MAX_TOKENS = 65_536;
/** Agnes 2.0 chat models: 64K max output per official docs. */
export const AGNES_CHAT_MAX_OUTPUT_TOKENS = 65_536;
/** Agnes 2.0 chat models: 256K context per official docs. */
export const AGNES_CHAT_CONTEXT_TOKENS = 256_000;
/** NVIDIA NIM hosted chat APIs cap completion tokens separately from context length. */
export const NVIDIA_NIM_MAX_COMPLETION_TOKENS = 262_144;
/** Kimi K3 default / typical request budget (API default max_completion_tokens). */
export const KIMI_K3_DEFAULT_MAX_TOKENS = 131_072;
/** Kimi K3 hard output ceiling. */
export const KIMI_K3_MAX_OUTPUT_TOKENS = 1_048_576;
/** Kimi K3 context window. */
export const KIMI_K3_CONTEXT_TOKENS = 1_000_000;
export const DEFAULT_MAX_TOKENS = MODEL_MAX_TOKENS;

/** Stored `maxTokens` at or below this (any historical default: 8192, then the
 *  interim 65536) is treated as a stale too-low value and lifted to
 *  {@link DEFAULT_MAX_TOKENS} on load, so existing installs stop truncating
 *  without the user manually re-opening the dialog. */
const LEGACY_LOW_MAX_TOKENS = 65536;

/** Catalog `/v1/models` output limits keyed by model id (desktop cache). */
let catalogOutputLimits: Record<string, number> = {};

export function setCatalogOutputLimits(limits: Record<string, number>): void {
  catalogOutputLimits = limits;
}

/** @deprecated Use {@link setCatalogOutputLimits} */
export function setSenseNovaOutputLimits(limits: Record<string, number>): void {
  setCatalogOutputLimits(limits);
}

/** @deprecated Use {@link setCatalogOutputLimits} */
export function setNvidiaNimOutputLimits(limits: Record<string, number>): void {
  setCatalogOutputLimits(limits);
}

/** @deprecated Use {@link setCatalogOutputLimits} */
export function setAgnesOutputLimits(limits: Record<string, number>): void {
  setCatalogOutputLimits(limits);
}

export function isDeepSeekV4Model(model: string): boolean {
  const lower = model.toLowerCase();
  if (!lower.includes('deepseek')) return false;
  return (
    lower.includes('v4-pro') ||
    lower.includes('v4-flash') ||
    lower.includes('v4pro') ||
    lower.includes('v4flash') ||
    (lower.includes('v4') && !lower.includes('v3'))
  );
}

/** Moonshot Kimi K3 (always-on thinking; fixed sampling). */
export function isKimiK3Model(model: string): boolean {
  const lower = model.toLowerCase();
  return lower.includes('kimi-k3') || lower.startsWith('kimi-k');
}

function catalogLimitForModel(model: string): number | undefined {
  const limit = catalogOutputLimits[model];
  return typeof limit === 'number' && limit > 0 ? limit : undefined;
}

/** Model-aware output cap for the Composer / runtime API. */
export function maxTokensCapForModel(model: string): number {
  const catalog = catalogLimitForModel(model);
  if (isDeepSeekV4Model(model)) {
    if (catalog != null) {
      if (catalog > THIRD_PARTY_MAX_TOKENS) {
        return Math.min(MODEL_MAX_TOKENS, Math.min(catalog, NVIDIA_NIM_MAX_COMPLETION_TOKENS));
      }
      return Math.min(THIRD_PARTY_MAX_TOKENS, catalog);
    }
    return MODEL_MAX_TOKENS;
  }
  if (isKimiK3Model(model)) {
    if (catalog != null) {
      return Math.min(KIMI_K3_MAX_OUTPUT_TOKENS, catalog);
    }
    return KIMI_K3_MAX_OUTPUT_TOKENS;
  }
  if (catalog != null) {
    return Math.min(THIRD_PARTY_MAX_TOKENS, catalog);
  }
  return THIRD_PARTY_MAX_TOKENS;
}

export function maxTokensForModel(model: string, params: ModelParams): number {
  const cap = maxTokensCapForModel(model);
  return Math.min(Math.max(256, Math.round(params.maxTokens) || 256), cap);
}

export const DEFAULT_MODEL_PARAMS: ModelParams = {
  temperature: 1.0,
  topP: 0.95,
  maxTokens: DEFAULT_MAX_TOKENS,
};

export function loadModelParams(): ModelParams {
  try {
    const raw = localStorage.getItem(MODEL_PARAMS_STORAGE_KEY);
    if (!raw) return { ...DEFAULT_MODEL_PARAMS };
    const parsed = JSON.parse(raw) as Partial<ModelParams>;
    const storedMaxTokens =
      typeof parsed.maxTokens === 'number' ? parsed.maxTokens : DEFAULT_MODEL_PARAMS.maxTokens;
    return {
      temperature:
        typeof parsed.temperature === 'number' ? parsed.temperature : DEFAULT_MODEL_PARAMS.temperature,
      topP: typeof parsed.topP === 'number' ? parsed.topP : DEFAULT_MODEL_PARAMS.topP,
      // Migration: lift legacy low budgets (≤ old 8192 default) so thinking-mode
      // turns stop truncating before the answer.
      maxTokens:
        storedMaxTokens <= LEGACY_LOW_MAX_TOKENS ? DEFAULT_MAX_TOKENS : storedMaxTokens,
    };
  } catch {
    return { ...DEFAULT_MODEL_PARAMS };
  }
}

export function saveModelParams(params: ModelParams): void {
  localStorage.setItem(MODEL_PARAMS_STORAGE_KEY, JSON.stringify(params));
}

/** Wire names for runtime HTTP (`temperature` / `top_p` / `max_tokens`). */
export interface ApiModelSampling {
  temperature?: number;
  top_p?: number;
  max_tokens?: number;
}

export function modelSamplingForApi(params: ModelParams, model?: string): ApiModelSampling {
  const trimmed = model?.trim() ?? '';
  const max_tokens = trimmed ? maxTokensForModel(trimmed, params) : params.maxTokens;
  // Kimi K3 fixes temperature/top_p server-side; omit them from the wire payload.
  if (trimmed && isKimiK3Model(trimmed)) {
    return { max_tokens };
  }
  return {
    temperature: params.temperature,
    top_p: params.topP,
    max_tokens,
  };
}
