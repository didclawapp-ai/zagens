import { useCallback, useEffect, useState } from 'react';
import { fetchUsage, type RuntimeConnectionState } from '../api/client';
import { useT } from '../i18n';
import {
  cacheHitPercentTextClass,
  formatCacheHitPercent,
} from '../lib/cacheUsage';
import { isRuntimeApiAvailable } from '../lib/runtimeReachable';
import type { UsageAggregation, UsageGroupBy } from '../types/usage';

const GROUP_BY_KEYS = {
  day: 'usageDashboard.groupDay',
  model: 'usageDashboard.groupModel',
  provider: 'usageDashboard.groupProvider',
  thread: 'usageDashboard.groupThread',
} as const;

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

/** Backend `cost_usd` is USD from `pricing::calculate_turn_cost_from_usage` (not CNY). */
function formatCostUsd(n: number): string {
  return `$${n.toFixed(2)}`;
}

function resolveHitRate(
  rate: number | null | undefined,
  cached: number,
  input: number,
): number | null {
  if (rate != null && Number.isFinite(rate)) return rate;
  if (input <= 0) return null;
  return (cached / input) * 100;
}

export default function UsageDashboard({
  runtimeConn,
  streaming = false,
  runtimeSessionEstablished = false,
}: {
  runtimeConn: RuntimeConnectionState;
  streaming?: boolean;
  runtimeSessionEstablished?: boolean;
}) {
  const { t } = useT();
  const runtimeReady = isRuntimeApiAvailable(runtimeConn, {
    streaming,
    sessionEstablished: runtimeSessionEstablished,
  });
  const [data, setData] = useState<UsageAggregation | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [groupBy, setGroupBy] = useState<UsageGroupBy>('day');

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await fetchUsage({ group_by: groupBy });
      setData(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [groupBy]);

  useEffect(() => {
    if (runtimeReady) {
      reload();
    }
  }, [runtimeReady, reload]);

  if (!runtimeReady) {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center space-y-2">
        <p>{t('usageDashboard.waitingRuntime')}</p>
        <p className="text-[10px]">{t('usageDashboard.waitingDetail')}</p>
      </div>
    );
  }

  if (loading && !data) {
    return <div className="p-4 text-xs text-t-text-muted text-center">{t('usageDashboard.loading')}</div>;
  }

  if (error && !data) {
    return (
      <div className="p-4 space-y-2">
        <p className="text-xs text-t-error">{t('usageDashboard.loadFailed', { error })}</p>
        <button type="button" onClick={reload} className="text-xs text-accent hover:underline">
          {t('usageDashboard.retry')}
        </button>
      </div>
    );
  }

  if (!data) return null;

  const hitRate = resolveHitRate(
    data.totals.cache_hit_rate,
    data.totals.cached_tokens,
    data.totals.input_tokens,
  );
  const missTokens = data.totals.miss_tokens ?? 0;
  const savings = data.totals.cache_savings_usd ?? 0;
  const hitRateClass =
    hitRate != null ? cacheHitPercentTextClass(hitRate) : 'text-t-text-muted';

  const maxBar = Math.max(...data.buckets.map((b) => b.input_tokens + b.output_tokens), 1);

  return (
    <div className="overflow-y-auto px-3 py-3 space-y-4">
      {data.cache_telemetry_incomplete ? (
        <p className="text-[10px] text-warning leading-snug rounded-md border border-warning/30 bg-warning/5 px-2 py-1.5">
          {t('usageDashboard.cacheTelemetryIncomplete')}
        </p>
      ) : null}

      <div className="grid grid-cols-2 gap-2">
        <div className="rounded-lg border border-card-border bg-canvas-alt p-3 text-center">
          <div className="text-lg font-bold text-accent font-display">
            {formatTokens(data.totals.input_tokens + data.totals.output_tokens)}
          </div>
          <div className="text-[10px] text-t-text-muted mt-0.5">{t('usageDashboard.totalTokens')}</div>
        </div>
        <div className="rounded-lg border border-card-border bg-canvas-alt p-3 text-center">
          <div className="text-lg font-bold text-warning font-display">
            {formatCostUsd(data.totals.cost_usd)}
          </div>
          <div className="text-[10px] text-t-text-muted mt-0.5">{t('usageDashboard.estimatedCostUsd')}</div>
        </div>
        <div className={`rounded-lg border border-card-border bg-canvas-alt p-3 text-center`}>
          <div className={`text-lg font-bold font-display tabular-nums ${hitRateClass}`}>
            {formatCacheHitPercent(hitRate)}
          </div>
          <div className="text-[10px] text-t-text-muted mt-0.5" title={t('usageDashboard.cacheHitRateHint')}>
            {t('usageDashboard.cacheHitRate')}
          </div>
        </div>
        <div className="rounded-lg border border-card-border bg-canvas-alt p-3 text-center">
          <div className="text-lg font-bold text-t-text font-display tabular-nums">
            {formatTokens(missTokens)}
          </div>
          <div className="text-[10px] text-t-text-muted mt-0.5">{t('usageDashboard.cacheMissTokens')}</div>
        </div>
        <div className="rounded-lg border border-card-border bg-canvas-alt p-3 text-center">
          <div className="text-lg font-bold text-success font-display tabular-nums">
            {savings > 0.0001 ? formatCostUsd(savings) : '—'}
          </div>
          <div className="text-[10px] text-t-text-muted mt-0.5">{t('usageDashboard.cacheSavingsUsd')}</div>
        </div>
        <div className="rounded-lg border border-card-border bg-canvas-alt p-3 text-center">
          <div className="text-lg font-bold text-t-text font-display">{data.totals.turns}</div>
          <div className="text-[10px] text-t-text-muted mt-0.5">{t('usageDashboard.turnCount')}</div>
        </div>
      </div>

      <div className="flex items-center gap-1">
        <span className="text-[10px] text-t-text-muted shrink-0">{t('usageDashboard.groupByLabel')}</span>
        {(Object.keys(GROUP_BY_KEYS) as UsageGroupBy[]).map((k) => (
          <button
            key={k}
            type="button"
            onClick={() => setGroupBy(k)}
            className={`px-2 py-0.5 rounded text-[10px] font-medium transition-colors ${
              groupBy === k
                ? 'bg-accent-soft text-accent'
                : 'text-t-text-muted hover:text-t-text hover:bg-hover'
            }`}
          >
            {t(GROUP_BY_KEYS[k])}
          </button>
        ))}
      </div>

      <div className="space-y-1.5">
        {data.buckets.length === 0 && (
          <p className="text-xs text-t-text-muted text-center py-4">{t('usageDashboard.noDataInRange')}</p>
        )}
        {data.buckets.map((b) => {
          const total = b.input_tokens + b.output_tokens;
          const pct = Math.max((total / maxBar) * 100, 1);
          return (
            <div key={b.key} className="flex items-center gap-2">
              <span className="w-16 text-right text-[10px] text-t-text-muted truncate" title={b.key}>
                {b.key}
              </span>
              <div className="flex-1 h-5 bg-canvas-alt rounded overflow-hidden">
                <div
                  className="h-full rounded bg-accent/70 transition-all"
                  style={{ width: `${pct}%` }}
                />
              </div>
              <span className="w-14 text-[10px] text-t-text-secondary text-right">
                {formatTokens(total)}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}
