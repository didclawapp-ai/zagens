import type { ModelParams } from '../components/ModelParamsDialog';

export const MODEL_PARAMS_STORAGE_KEY = 'deepseek-desktop-model-params';

export const DEFAULT_MODEL_PARAMS: ModelParams = {
  temperature: 1.0,
  topP: 0.95,
  maxTokens: 8192,
};

export function loadModelParams(): ModelParams {
  try {
    const raw = localStorage.getItem(MODEL_PARAMS_STORAGE_KEY);
    if (!raw) return { ...DEFAULT_MODEL_PARAMS };
    const parsed = JSON.parse(raw) as Partial<ModelParams>;
    return {
      temperature:
        typeof parsed.temperature === 'number' ? parsed.temperature : DEFAULT_MODEL_PARAMS.temperature,
      topP: typeof parsed.topP === 'number' ? parsed.topP : DEFAULT_MODEL_PARAMS.topP,
      maxTokens:
        typeof parsed.maxTokens === 'number' ? parsed.maxTokens : DEFAULT_MODEL_PARAMS.maxTokens,
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

export function modelSamplingForApi(params: ModelParams): ApiModelSampling {
  return {
    temperature: params.temperature,
    top_p: params.topP,
    max_tokens: params.maxTokens,
  };
}
