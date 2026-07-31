/** Composer / settings model presets and config-driven option lists. */

export const DESKTOP_MODEL_PRESET_IDS = [
  'deepseek-v4-pro',
  'deepseek-v4-flash',
] as const;

const PRESET_LABELS: Record<string, string> = {
  'deepseek-v4-pro': 'DeepSeek V4 Pro',
  'deepseek-v4-flash': 'DeepSeek V4 Flash (0731)',
};

const PRESET_SHORT: Record<string, string> = {
  'deepseek-v4-pro': 'V4 Pro',
  'deepseek-v4-flash': 'Flash 0731',
};

export const DEFAULT_COMPOSER_MODEL = 'deepseek-v4-pro';

export function normalizeComposerModel(raw: unknown): string | undefined {
  if (typeof raw !== 'string') return undefined;
  const trimmed = raw.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

export function composerModelLabel(model: string): string {
  return PRESET_LABELS[model] ?? model;
}

export function composerModelShortLabel(model: string, maxLen = 18): string {
  const preset = PRESET_SHORT[model];
  if (preset) return preset;
  if (model.length <= maxLen) return model;
  return `${model.slice(0, maxLen - 1)}…`;
}

export function isPresetComposerModel(model: string): boolean {
  return DESKTOP_MODEL_PRESET_IDS.includes(model as (typeof DESKTOP_MODEL_PRESET_IDS)[number]);
}

/** Presets first, then config.toml models, then the active selection. */
export function mergeComposerModelOptions(configured: string[], current?: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  const add = (raw: string) => {
    const normalized = normalizeComposerModel(raw);
    if (!normalized) return;
    const key = normalized.toLowerCase();
    if (seen.has(key)) return;
    seen.add(key);
    out.push(normalized);
  };
  for (const id of DESKTOP_MODEL_PRESET_IDS) add(id);
  for (const m of configured) add(m);
  if (current) add(current);
  return out;
}

export function loadStoredComposerModel(): string {
  try {
    return normalizeComposerModel(localStorage.getItem('zagens-desktop-model')) ?? DEFAULT_COMPOSER_MODEL;
  } catch {
    return DEFAULT_COMPOSER_MODEL;
  }
}
