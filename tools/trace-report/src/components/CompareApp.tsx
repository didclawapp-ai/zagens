import { useState } from 'preact/hooks';
import type { TraceCompareDocument } from '../types';

interface Props {
  doc: TraceCompareDocument;
}

function CoherenceBadge({ ok }: { ok: boolean }) {
  return (
    <span class={ok ? 'badge badge-ok' : 'badge badge-fail'}>
      {ok ? 'coherence_ok' : 'coherence_failed'}
    </span>
  );
}

export function CompareOverview({ doc }: Props) {
  const { diff, left, right } = doc;
  const sequenceOk = diff.event_kind_sequence_match;

  return (
    <section class="panel overview">
      <div class="overview-header">
        <div>
          <h1>Kernel Trace Compare</h1>
          <p class="muted">Side-by-side replay diff · Zagens Flight Recorder</p>
        </div>
        <div class={sequenceOk ? 'badge badge-ok' : 'badge badge-fail'}>
          {sequenceOk ? 'event_sequence_match' : 'event_sequence_diff'}
        </div>
      </div>

      <div class="compare-columns">
        <div class="compare-side">
          <h2>{left.label}</h2>
          <CoherenceBadge ok={diff.left_coherence_ok} />
          <ul class="stat-list">
            <li>{diff.left_event_count} events</li>
            <li>{diff.turn_count_left} turns</li>
          </ul>
        </div>
        <div class="compare-side">
          <h2>{right.label}</h2>
          <CoherenceBadge ok={diff.right_coherence_ok} />
          <ul class="stat-list">
            <li>{diff.right_event_count} events</li>
            <li>{diff.turn_count_right} turns</li>
          </ul>
        </div>
      </div>

      {!diff.coherence_match ? (
        <p class="compare-alert">
          Coherence badges differ — one side failed replay verification.
        </p>
      ) : null}

      {diff.first_kind_mismatch_index != null ? (
        <p class="compare-alert">
          First event-kind mismatch at index {diff.first_kind_mismatch_index}.
        </p>
      ) : null}

      {diff.effect_count_deltas.length > 0 ? (
        <div class="compare-section">
          <h3>Effect count deltas</h3>
          <table class="compare-table">
            <thead>
              <tr>
                <th>Effect</th>
                <th>Left</th>
                <th>Right</th>
                <th>Δ</th>
              </tr>
            </thead>
            <tbody>
              {diff.effect_count_deltas.map((row) => (
                <tr key={row.field}>
                  <td>{row.field}</td>
                  <td>{row.left}</td>
                  <td>{row.right}</td>
                  <td>{row.delta > 0 ? `+${row.delta}` : row.delta}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}

      {diff.guard_event_deltas.length > 0 ? (
        <div class="compare-section">
          <h3>Guard / LHT event deltas</h3>
          <table class="compare-table">
            <thead>
              <tr>
                <th>Kind</th>
                <th>Left</th>
                <th>Right</th>
              </tr>
            </thead>
            <tbody>
              {diff.guard_event_deltas.map((row) => (
                <tr key={row.kind}>
                  <td>{row.kind}</td>
                  <td>{row.left}</td>
                  <td>{row.right}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}
    </section>
  );
}

export function CompareReplayLab({ doc }: Props) {
  const maxLen = Math.max(doc.diff.left_event_kinds.length, doc.diff.right_event_kinds.length);
  const mismatchAt = doc.diff.first_kind_mismatch_index;

  return (
    <section class="panel">
      <h2>Replay Lab — event kind sequence</h2>
      <p class="muted">
        Normalized `event_type` chain (seq/ts ignored). Mismatches highlighted.
      </p>
      <div class="replay-lab-grid">
        <div>
          <h3>{doc.left.label}</h3>
          <ol class="replay-seq">
            {Array.from({ length: maxLen }, (_, idx) => {
              const kind = doc.diff.left_event_kinds[idx];
              const mismatch =
                mismatchAt != null && idx >= mismatchAt && kind !== doc.diff.right_event_kinds[idx];
              return (
                <li key={idx} class={mismatch ? 'replay-diff' : undefined}>
                  {kind ?? '—'}
                </li>
              );
            })}
          </ol>
        </div>
        <div>
          <h3>{doc.right.label}</h3>
          <ol class="replay-seq">
            {Array.from({ length: maxLen }, (_, idx) => {
              const kind = doc.diff.right_event_kinds[idx];
              const mismatch =
                mismatchAt != null && idx >= mismatchAt && kind !== doc.diff.left_event_kinds[idx];
              return (
                <li key={idx} class={mismatch ? 'replay-diff' : undefined}>
                  {kind ?? '—'}
                </li>
              );
            })}
          </ol>
        </div>
      </div>
    </section>
  );
}

export function CompareApp({ doc }: Props) {
  const [tab, setTab] = useState<'overview' | 'replay'>('overview');

  return (
    <main class="app compare-app">
      <header class="app-header">
        <div>
          <strong>Zagens Trace Compare</strong>
          <span class="muted">
            {' '}
            · {doc.left.label} vs {doc.right.label}
          </span>
        </div>
        <nav class="tabs" aria-label="Compare sections">
          <button
            type="button"
            class={tab === 'overview' ? 'tab active' : 'tab'}
            onClick={() => setTab('overview')}
          >
            Overview
          </button>
          <button
            type="button"
            class={tab === 'replay' ? 'tab active' : 'tab'}
            onClick={() => setTab('replay')}
          >
            Replay Lab
          </button>
        </nav>
      </header>

      {tab === 'overview' && <CompareOverview doc={doc} />}
      {tab === 'replay' && <CompareReplayLab doc={doc} />}

      <footer class="footer muted">compare · schema v{doc.schema_version}</footer>
    </main>
  );
}
