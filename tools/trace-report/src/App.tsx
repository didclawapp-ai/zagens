import { useState } from 'preact/hooks';
import { Overview } from './components/Overview';
import { Timeline } from './components/Timeline';
import { TurnMap } from './components/TurnMap';
import { MemoryPanel } from './components/MemoryPanel';
import { HarnessPanel } from './components/HarnessPanel';
import { ReplayLab } from './components/ReplayLab';
import { CompareApp } from './components/CompareApp';
import { groupEventsByLane, isCompareDocument, loadTraceDocument, sourceLabel } from './lib/bundle';
import type { TraceTab } from './types';

const TABS: { id: TraceTab; label: string }[] = [
  { id: 'overview', label: 'Overview' },
  { id: 'timeline', label: 'Timeline' },
  { id: 'turnmap', label: 'Turn Map' },
  { id: 'memory', label: 'Memory' },
  { id: 'harness', label: 'Harness' },
  { id: 'replay', label: 'Replay Lab' },
];

export function App() {
  const [tab, setTab] = useState<TraceTab>('overview');

  let document;
  try {
    document = loadTraceDocument();
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return (
      <main class="app error-state">
        <h1>Trace bundle missing</h1>
        <p>{message}</p>
      </main>
    );
  }

  if (isCompareDocument(document)) {
    return <CompareApp doc={document} />;
  }

  const bundle = document;
  const lanes = groupEventsByLane(bundle.events);

  return (
    <main class="app">
      <header class="app-header">
        <div>
          <strong>Zagens Flight Recorder</strong>
          <span class="muted"> · {sourceLabel(bundle)}</span>
        </div>
        <nav class="tabs" aria-label="Report sections">
          {TABS.map((t) => (
            <button
              type="button"
              key={t.id}
              class={tab === t.id ? 'tab active' : 'tab'}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </nav>
      </header>

      {tab === 'overview' && <Overview bundle={bundle} />}
      {tab === 'timeline' && <Timeline lanes={lanes} />}
      {tab === 'turnmap' && <TurnMap summary={bundle.replay_summary} />}
      {tab === 'memory' && <MemoryPanel analysis={bundle.analysis} />}
      {tab === 'harness' && <HarnessPanel harness={bundle.harness} />}
      {tab === 'replay' && <ReplayLab bundle={bundle} />}

      <footer class="footer muted">
        schema v{bundle.schema_version} · {bundle.source.kind}
        {bundle.replay_summary.synthetic_timeline ? ' · synthetic timeline' : ''}
      </footer>
    </main>
  );
}
