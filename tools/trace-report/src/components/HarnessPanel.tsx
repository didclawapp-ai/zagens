import type { TraceBundle } from '../types';

interface Props {
  harness: TraceBundle['harness'];
}

export function HarnessPanel({ harness }: Props) {
  if (!harness?.task_graph) {
    return (
      <section class="panel">
        <h2>Harness Audit</h2>
        <p class="lane-placeholder">
          No harness snapshot — use <code>zagens trace export --thread …</code> with default
          <code> --include-harness</code>.
        </p>
      </section>
    );
  }

  const graph = harness.task_graph as Record<string, unknown>;
  const progress = graph.progress as Record<string, unknown> | undefined;
  const checklist = graph.checklist as { items?: Array<Record<string, unknown>> } | undefined;
  const nodes = graph.recent_nodes as unknown[] | undefined;
  const snapshotSource = (graph.snapshot_source as string | undefined) ?? harness.snapshot_source;

  return (
    <section class="panel">
      <h2>Harness Audit</h2>
      <p class="muted">
        Offline task graph from thread store snapshots
        {snapshotSource ? ` · ${snapshotSource}` : ''}.
      </p>

      {progress && (
        <div class="kpi-grid harness-kpi">
          <div class="kpi">
            <span class="kpi-label">Progress</span>
            <span class="kpi-value">{String(progress.percent ?? '—')}%</span>
          </div>
          <div class="kpi">
            <span class="kpi-label">Open items</span>
            <span class="kpi-value">{String(progress.open_items ?? '—')}</span>
          </div>
          <div class="kpi">
            <span class="kpi-label">Graph complete</span>
            <span class="kpi-value">{String(progress.graph_complete ?? '—')}</span>
          </div>
        </div>
      )}

      <h3 class="subhead">Checklist</h3>
      {!checklist?.items?.length ? (
        <p class="lane-placeholder">No checklist items in snapshot.</p>
      ) : (
        <ul class="checklist">
          {checklist.items.slice(0, 40).map((item, idx) => (
            <li key={String(item.id ?? idx)} class={`status-${String(item.status ?? 'pending')}`}>
              <span class="check-status">{String(item.status ?? 'pending')}</span>
              <span>{String(item.label ?? item.text ?? item.id ?? 'item')}</span>
            </li>
          ))}
        </ul>
      )}

      <h3 class="subhead">Nodes</h3>
      {!nodes?.length ? (
        <p class="lane-placeholder">
          No persisted harness nodes (live cache only during runtime). Gate decisions appear in
          Timeline Guards lane when emitted as kernel events.
        </p>
      ) : (
        <ul class="data-list">
          {nodes.map((node, idx) => (
            <li key={idx}>
              <pre>{JSON.stringify(node, null, 2)}</pre>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
