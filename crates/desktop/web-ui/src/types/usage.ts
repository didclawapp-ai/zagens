/** Aggregated usage response from GET /v1/usage */
export interface UsageBucket {
  key: string;
  input_tokens: number;
  output_tokens: number;
  cached_tokens: number;
  miss_tokens: number;
  reasoning_tokens: number;
  cost_usd: number;
  cost_usd_without_cache: number;
  cache_savings_usd: number;
  cache_hit_rate?: number | null;
  turns: number;
}

export interface UsageTotals {
  input_tokens: number;
  output_tokens: number;
  cached_tokens: number;
  miss_tokens: number;
  reasoning_tokens: number;
  cost_usd: number;
  cost_usd_without_cache: number;
  cache_savings_usd: number;
  cache_hit_rate?: number | null;
  turns: number;
}

export interface UsageAggregation {
  since: string | null;
  until: string | null;
  group_by: string;
  totals: UsageTotals;
  buckets: UsageBucket[];
  cache_telemetry_incomplete: boolean;
}

export type UsageGroupBy = 'day' | 'model' | 'provider' | 'thread';

export interface UsageParams {
  since?: string;
  until?: string;
  group_by?: UsageGroupBy;
}
