import type { TurnUsage } from '../api/streamNormalize';

/** Per-turn cache hit % (hit / input_tokens), matching runtime `usage_aggregate`. */
export function turnCacheHitPercent(usage: TurnUsage): number | null {
  if (!usage.input_tokens || usage.input_tokens <= 0) return null;
  const hit = usage.prompt_cache_hit_tokens ?? 0;
  return (hit / usage.input_tokens) * 100;
}

export function formatCacheHitPercent(rate: number | null | undefined): string {
  if (rate == null || !Number.isFinite(rate)) return '—';
  return `${rate.toFixed(1)}%`;
}

/** Footer chip colors aligned with runtime prompt guidance (<40% red, <80% yellow). */
export function cacheHitPercentTextClass(rate: number): string {
  if (rate < 40) return 'text-t-error';
  if (rate < 80) return 'text-warning';
  return 'text-success';
}

/** From persisted thread turn usage (GET thread detail). */
export function usageRecordCacheHitPercent(usage: {
  input_tokens?: number;
  prompt_cache_hit_tokens?: number | null;
} | null | undefined): number | null {
  if (!usage?.input_tokens || usage.input_tokens <= 0) return null;
  const hit = usage.prompt_cache_hit_tokens ?? 0;
  return (hit / usage.input_tokens) * 100;
}
