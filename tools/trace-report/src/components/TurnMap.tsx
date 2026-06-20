import type { TraceReplaySummary, TraceTurnSummary } from '../types';

interface Props {
  summary: TraceReplaySummary;
}

export function TurnMap({ summary }: Props) {
  const { turns, effect_counts: effects } = summary;

  return (
    <section class="panel">
      <h2>Turn Map</h2>
      <p class="muted">Per-turn coherence and outcomes from kernel replay.</p>

      {turns.length === 0 ? (
        <p class="lane-placeholder">No turn summaries in this trace.</p>
      ) : (
        <div class="turn-grid">
          {turns.map((turn: TraceTurnSummary) => (
            <article class={`turn-card ${turn.coherence_ok ? 'ok' : 'fail'}`} key={turn.turn_id}>
              <header>
                <code>{turn.turn_id}</code>
                <span class={`badge ${turn.coherence_ok ? 'badge-ok' : 'badge-fail'}`}>
                  {turn.coherence_ok ? 'coherent' : 'incoherent'}
                </span>
              </header>
              <dl>
                <div>
                  <dt>Events</dt>
                  <dd>{turn.event_count}</dd>
                </div>
                <div>
                  <dt>Outcome</dt>
                  <dd>{turn.outcome ?? '—'}</dd>
                </div>
              </dl>
              {turn.coherence_error && (
                <pre class="error-box">{turn.coherence_error}</pre>
              )}
            </article>
          ))}
        </div>
      )}

      <h3 class="subhead">Thread effect totals</h3>
      <ul class="effect-list">
        {Object.entries(effects).map(([key, value]) => (
          <li key={key}>
            <span>{key}</span>
            <strong>{String(value)}</strong>
          </li>
        ))}
      </ul>
    </section>
  );
}
