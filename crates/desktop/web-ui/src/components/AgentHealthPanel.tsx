import { useCallback, useEffect, useState } from 'react';
import { fetchAgentHealth, type RuntimeConnectionState } from '../api/client';
import { useT } from '../i18n';
import { isRuntimeApiAvailable } from '../lib/runtimeReachable';
import type { AgentHealthReport } from '../types/agentHealth';

function formatRate(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return '—';
  return `${value.toFixed(1)}%`;
}

function MetricCard({
  label,
  value,
  hint,
}: {
  label: string;
  value: string;
  hint?: string;
}) {
  return (
    <div className="rounded-lg border border-t-border/60 bg-t-surface/40 p-3">
      <div className="text-lg font-semibold tabular-nums text-t-text">{value}</div>
      <div className="text-[10px] text-t-text-muted mt-0.5" title={hint}>
        {label}
      </div>
    </div>
  );
}

export default function AgentHealthPanel({
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
  const [data, setData] = useState<AgentHealthReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await fetchAgentHealth();
      setData(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (runtimeReady) {
      void reload();
    }
  }, [runtimeReady, reload]);

  if (!runtimeReady) {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center space-y-2">
        <p>{t('agentHealth.waitingRuntime')}</p>
        <p className="text-[10px]">{t('agentHealth.waitingDetail')}</p>
      </div>
    );
  }

  if (loading && !data) {
    return <div className="p-4 text-xs text-t-text-muted text-center">{t('agentHealth.loading')}</div>;
  }

  if (error && !data) {
    return (
      <div className="p-4 text-center space-y-2">
        <p className="text-xs text-t-error">{t('agentHealth.loadFailed', { error })}</p>
        <button type="button" className="text-xs text-t-accent hover:underline" onClick={() => void reload()}>
          {t('agentHealth.retry')}
        </button>
      </div>
    );
  }

  if (!data) {
    return null;
  }

  const harnessPassRate =
    data.harness_verify_events > 0
      ? (data.harness_verify_passes / data.harness_verify_events) * 100
      : null;

  return (
    <div className="flex flex-col gap-4 p-4 text-xs overflow-y-auto min-h-0">
      <div className="flex items-start justify-between gap-2">
        <div>
          <p className="text-[11px] text-t-text-muted">{t('agentHealth.subtitle')}</p>
          {!data.present ? (
            <p className="text-t-warning mt-1">{t('agentHealth.noSessionsDb')}</p>
          ) : null}
          {data.note ? <p className="text-[10px] text-t-text-muted mt-1">{data.note}</p> : null}
        </div>
        <button
          type="button"
          className="shrink-0 text-[10px] text-t-accent hover:underline"
          onClick={() => void reload()}
          disabled={loading}
        >
          {loading ? t('agentHealth.refreshing') : t('agentHealth.refresh')}
        </button>
      </div>

      <div className="grid grid-cols-2 gap-2">
        <MetricCard label={t('agentHealth.toolCalls')} value={String(data.tool_calls)} />
        <MetricCard
          label={t('agentHealth.toolFailureRate')}
          value={formatRate(data.tool_failure_rate)}
        />
        <MetricCard
          label={t('agentHealth.harnessVerifyEvents')}
          value={String(data.harness_verify_events)}
          hint={t('agentHealth.harnessVerifyHint')}
        />
        <MetricCard
          label={t('agentHealth.harnessVerifyPassRate')}
          value={formatRate(harnessPassRate)}
        />
        <MetricCard
          label={t('agentHealth.harnessSelfHealRate')}
          value={formatRate(data.harness_verify_self_heal_rate)}
        />
        <MetricCard
          label={t('agentHealth.stageGateBlocked')}
          value={String(data.stage_gate_blocked_events)}
        />
        <MetricCard
          label={t('agentHealth.loopGuardEvents')}
          value={String(data.loop_guard_events)}
        />
        <MetricCard
          label={t('agentHealth.hintCoverage')}
          value={formatRate(data.hint_coverage_rate)}
        />
      </div>

      {data.top_by_calls.length > 0 ? (
        <section>
          <h3 className="text-[11px] font-semibold text-t-text mb-2">{t('agentHealth.topByCalls')}</h3>
          <ul className="space-y-1">
            {data.top_by_calls.slice(0, 8).map((tool) => (
              <li key={tool.name} className="flex justify-between gap-2 text-[10px]">
                <span className="truncate font-mono">{tool.name}</span>
                <span className="shrink-0 text-t-text-muted tabular-nums">
                  {tool.calls} · {formatRate(tool.failure_rate)} fail
                </span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {data.top_by_failure_rate.length > 0 ? (
        <section>
          <h3 className="text-[11px] font-semibold text-t-text mb-2">{t('agentHealth.topMisused')}</h3>
          <ul className="space-y-1">
            {data.top_by_failure_rate.map((tool) => (
              <li key={tool.name} className="flex justify-between gap-2 text-[10px]">
                <span className="truncate font-mono">{tool.name}</span>
                <span className="shrink-0 text-t-text-muted tabular-nums">
                  {formatRate(tool.failure_rate)} ({tool.failures}/{tool.calls})
                </span>
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {data.hint_coverage_top_failures.length > 0 ? (
        <section>
          <h3 className="text-[11px] font-semibold text-t-text mb-2">{t('agentHealth.hintCoverageTitle')}</h3>
          <ul className="space-y-2">
            {data.hint_coverage_top_failures.map((entry) => (
              <li key={entry.name} className="text-[10px] border-l-2 border-t-border/60 pl-2">
                <div className="font-mono">
                  {entry.hint_covered ? '✓' : '✗'} {entry.name}
                </div>
                <div className="text-t-text-muted mt-0.5">
                  {entry.hint_summary ?? t('agentHealth.noHintYet')}
                </div>
              </li>
            ))}
          </ul>
        </section>
      ) : null}
    </div>
  );
}
