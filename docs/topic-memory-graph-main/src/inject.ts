/**
 * Replace or append a bounded section in markdown using HTML comment markers.
 * Safe for AGENTS.md-style files: markers are invisible in most Markdown renderers.
 */

export interface MemorySectionMarkers {
  readonly start: string;
  readonly end: string;
}

/** Neutral defaults for new integrations */
export const DEFAULT_MARKERS: MemorySectionMarkers = {
  start: "<!-- topic-memory-graph-start -->",
  end: "<!-- topic-memory-graph-end -->",
};

/** Markers used by DidClaw desktop (Tauri) — keep in sync when migrating hosts */
export const DIDCLAW_PHEROMONE_MARKERS: MemorySectionMarkers = {
  start: "<!-- didclaw-pheromone-start -->",
  end: "<!-- didclaw-pheromone-end -->",
};

/**
 * Insert `content` between markers in `existing`. If markers are absent, appends a new block.
 */
export function injectMemorySection(
  existing: string,
  content: string,
  markers: MemorySectionMarkers = DEFAULT_MARKERS,
): string {
  const { start, end } = markers;
  const section = `${start}\n${content}\n${end}`;

  const si = existing.indexOf(start);
  const ei = existing.indexOf(end);
  if (si !== -1 && ei !== -1 && ei > si) {
    const afterEnd = ei + end.length;
    return `${existing.slice(0, si)}${section}${existing.slice(afterEnd)}`;
  }

  const trimmed = existing.trimEnd();
  if (trimmed.length === 0) return section;
  return `${trimmed}\n\n${section}`;
}
