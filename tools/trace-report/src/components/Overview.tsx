import type { TraceBundle } from '../types';
import { sourceLabel } from '../lib/bundle';
import { buildExecutiveSummary } from '../lib/summary';

interface Props {
  bundle: TraceBundle;
}

function latestTurn(bundle: TraceBundle) {
  const turns = bundle.replay_summary.turns;
  return turns.length > 0 ? turns[turns.length - 1] : undefined;
}

export function Overview({ bundle }: Props) {
  const { replay_summary: replay, events } = bundle;
  const turn = latestTurn(bundle);
  const summary = buildExecutiveSummary(bundle);
  const badgeClass = replay.coherence_ok ? 'badge badge-ok' : 'badge badge-fail';
  const toolCount = events.filter((e) => {
    const k = e.payload?.event_type;
    return k === 'tool_call_finished';
  }).length;
  const turnCount = replay.turns.length;
  const coherentCount = replay.turns.filter((t) => t.coherence_ok).length;

  return (
    <section class="panel overview">
      <div class="overview-header">
        <div>
          <h1>Kernel Trace Report</h1>
          <p class="muted">Zagens Flight Recorder · {sourceLabel(bundle)}</p>
        </div>
        <div class={badgeClass}>
          {replay.coherence_ok ? 'coherence_ok' : 'coherence_failed'}
        </div>
      </div>

      <article class={`exec-summary ${replay.coherence_ok ? 'exec-ok' : 'exec-fail'}`}>
        <h2>{summary.headline}</h2>
        <p class="exec-lead">{summary.lead}</p>
        <ul class="exec-bullets">
          {summary.bullets.map((line) => (
            <li key={line}>{line}</li>
          ))}
        </ul>
        {summary.findings.length > 0 && (
          <div class="exec-findings" role="list">
            {summary.findings.map((f) => (
              <div key={f.text} class={`finding finding-${f.severity}`} role="listitem">
                {f.text}
              </div>
            ))}
          </div>
        )}
      </article>

      <h3 class="subhead">Metrics</h3>
      <div class="kpi-grid">
        <div class="kpi">
          <span class="kpi-label">Events</span>
          <span class="kpi-value">{events.length}</span>
        </div>
        <div class="kpi">
          <span class="kpi-label">Turns</span>
          <span class="kpi-value">
            {turnCount > 0 ? `${coherentCount}/${turnCount} coherent` : '—'}
          </span>
        </div>
        <div class="kpi">
          <span class="kpi-label">Latest turn</span>
          <span class="kpi-value">{turn?.turn_id ?? '—'}</span>
        </div>
        <div class="kpi">
          <span class="kpi-label">Outcome</span>
          <span class="kpi-value">{turn?.outcome ?? '—'}</span>
        </div>
        <div class="kpi">
          <span class="kpi-label">Tools finished</span>
          <span class="kpi-value">{toolCount || 'N/A'}</span>
        </div>
      </div>

      {replay.synthetic_timeline && (
        <p class="note">Timeline uses synthetic timestamps (fixture export).</p>
      )}

      {!replay.coherence_ok && replay.coherence_error && (
        <details class="raw-coherence">
          <summary>Raw coherence message</summary>
          <pre class="error-box">{replay.coherence_error}</pre>
        </details>
      )}
    </section>
  );
}
