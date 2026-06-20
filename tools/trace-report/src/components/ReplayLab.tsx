import type { TraceBundle } from '../types';
import { classifyLane, eventKind, eventLabel } from '../lib/bundle';

interface Props {
  bundle: TraceBundle;
}

export function ReplayLab({ bundle }: Props) {
  const { replay_summary: replay } = bundle;

  return (
    <section class="panel">
      <h2>Replay Lab</h2>
      <p class="muted">
        Logged kernel events vs replay summary from the same coherence gate as CI golden tests.
      </p>

      <div class="replay-lab-summary">
        <span class={`badge ${replay.coherence_ok ? 'badge-ok' : 'badge-fail'}`}>
          {replay.coherence_ok ? 'coherence_ok' : 'coherence_failed'}
        </span>
        {replay.coherence_error ? (
          <p class="compare-alert">{replay.coherence_error}</p>
        ) : null}
      </div>

      <h3 class="subhead">Logged event sequence</h3>
      <ol class="replay-seq replay-seq-single">
        {bundle.events.map((entry) => {
          const payload = entry.payload as Record<string, unknown>;
          const kind = eventKind(payload);
          const lane = classifyLane(kind);
          return (
            <li key={entry.seq} class={lane === 'guards' ? 'replay-guard' : undefined}>
              <span class="replay-seq-idx">{entry.seq}</span>
              <code>{kind}</code>
              <span class="muted"> — {eventLabel(kind, payload)}</span>
            </li>
          );
        })}
      </ol>

      <h3 class="subhead">Replay effect totals (interpreted)</h3>
      <ul class="effect-list">
        {Object.entries(replay.effect_counts).map(([key, value]) => (
          <li key={key}>
            <span>{key}</span>
            <strong>{String(value)}</strong>
          </li>
        ))}
      </ul>
    </section>
  );
}
