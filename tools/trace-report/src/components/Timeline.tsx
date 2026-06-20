import type { LaneGroup } from '../types';

interface Props {
  lanes: LaneGroup[];
}

function formatMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  return `${(ms / 1000).toFixed(1)}s`;
}

export function Timeline({ lanes }: Props) {
  return (
    <section class="panel timeline">
      <h2>Timeline</h2>
      <p class="muted">P0 lanes: Model · Tools · Guards</p>
      <div class="lanes">
        {lanes.map((lane) => (
          <div class="lane" key={lane.lane}>
            <div class="lane-header">{lane.title}</div>
            {lane.events.length === 0 ? (
              <div class="lane-placeholder">
                No events in this lane for this trace
              </div>
            ) : (
              <ul class="lane-events">
                {lane.events.map((ev) => (
                  <li class="lane-event" key={ev.seq}>
                    <span class="lane-time">{formatMs(ev.ts_ms)}</span>
                    <span class="lane-kind">{ev.kind}</span>
                    <span class="lane-label">{ev.label}</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        ))}
      </div>
    </section>
  );
}
