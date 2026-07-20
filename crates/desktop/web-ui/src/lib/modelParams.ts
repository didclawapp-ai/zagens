import type { ModelParams } from '../components/ModelParamsDialog';
import {
  AGNES_CHAT_CONTEXT_TOKENS,
  AGNES_CHAT_MAX_OUTPUT_TOKENS,
  DEFAULT_MAX_OUTPUT_TOKENS,
  DEEPSEEK_V4_MAX_OUTPUT_TOKENS,
  KIMI_K3_CONTEXT_TOKENS,
  KIMI_K3_DEFAULT_MAX_TOKENS,
  KIMI_K3_MAX_OUTPUT_TOKENS,
  hidesEffortOff,
  isDeepSeekV4Model,
  isKimiK3Model,
  mapReasoningEffort,
  resolveModelCaps,
} from './generated/modelCatalog';

export {
  AGNES_CHAT_CONTEXT_TOKENS,
  AGNES_CHAT_MAX_OUTPUT_TOKENS,
  KIMI_K3_CONTEXT_TOKENS,
  KIMI_K3_DEFAULT_MAX_TOKENS,
  KIMI_K3_MAX_OUTPUT_TOKENS,
  hidesEffortOff,
  isDeepSeekV4Model,
  isKimiK3Model,
  mapReasoningEffort,
  resolveModelCaps,
};

export const MODEL_PARAMS_STORAGE_KEY = 'zagens-desktop-model-params';

/**
 * DeepSeek V4 official max output. Defaults prefer the model ceiling so
 * thinking-mode turns are not truncated before the answer.
 * Value comes from `shared-defs/model-catalog.json` (deepseek_v4.max_output).
 */
export const MODEL_MAX_TOKENS = DEEPSEEK_V4_MAX_OUTPUT_TOKENS;
/** Catalog default max_output — used when hosted catalogs publish a low third-party cap. */
export const THIRD_PARTY_MAX_TOKENS = DEFAULT_MAX_OUTPUT_TOKENS;
/** NVIDIA NIM hosted chat APIs cap completion tokens separately from context length. */
export const NVIDIA_NIM_MAX_COMPLETION_TOKENS = 262_144;
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

function catalogLimitForModel(model: string): number | undefined {
  const limit = catalogOutputLimits[model];
  return typeof limit === 'number' && limit > 0 ? limit : undefined;
}

/** Model-aware output cap for the Composer / runtime API. */
export function maxTokensCapForModel(model: string): number {
  const caps = resolveModelCaps(model);
  const providerCatalog = catalogLimitForModel(model);
  if (caps.familyId === 'deepseek_v4') {
    if (providerCatalog != null) {
      if (providerCatalog > THIRD_PARTY_MAX_TOKENS) {
        return Math.min(caps.maxOutput, Math.min(providerCatalog, NVIDIA_NIM_MAX_COMPLETION_TOKENS));
      }
      return Math.min(THIRD_PARTY_MAX_TOKENS, providerCatalog);
    }
    return caps.maxOutput;
  }
  if (providerCatalog != null) {
    return Math.min(caps.maxOutput, providerCatalog);
  }
  return caps.maxOutput;
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
  if (trimmed && resolveModelCaps(trimmed).omitSampling) {
    return { max_tokens };
  }
  return {
    temperature: params.temperature,
    top_p: params.topP,
    max_tokens,
  };
}
