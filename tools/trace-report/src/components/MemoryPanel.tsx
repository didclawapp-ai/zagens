import type { TraceAnalysis } from '../types';

interface Props {
  analysis: TraceAnalysis | null | undefined;
}

export function MemoryPanel({ analysis }: Props) {
  if (!analysis) {
    return (
      <section class="panel">
        <h2>Memory & Context</h2>
        <p class="lane-placeholder">No analysis block (fixture export or thread without compaction/capacity events).</p>
      </section>
    );
  }

  const { compaction_timeline, capacity_checkpoints } = analysis;

  return (
    <section class="panel">
      <h2>Memory & Context</h2>
      <p class="muted">Compaction artifacts and capacity checkpoints from kernel events.</p>

      <h3 class="subhead">Compaction timeline</h3>
      {compaction_timeline.length === 0 ? (
        <p class="lane-placeholder">No compaction artifacts in this trace.</p>
      ) : (
        <ul class="data-list">
          {compaction_timeline.map((c) => (
            <li key={`${c.turn_id}-${c.artifact_id}`}>
              <code>{c.artifact_id}</code>
              <span>
                turn {c.turn_id} · replaced msgs [{c.replaced_from}–{c.replaced_to}]
              </span>
            </li>
          ))}
        </ul>
      )}

      <h3 class="subhead">Capacity checkpoints</h3>
      {capacity_checkpoints.length === 0 ? (
        <p class="lane-placeholder">No capacity checkpoints in this trace.</p>
      ) : (
        <ul class="data-list">
          {capacity_checkpoints.map((c, idx) => (
            <li key={`${c.turn_id}-${c.step_idx}-${idx}`}>
              <span>
                step {c.step_idx} · {c.tokens_used}/{c.token_budget} tokens · {c.action}
              </span>
              <code>{c.turn_id}</code>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
