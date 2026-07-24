/**
 * Live accordion for activity groups.
 *
 * While the turn is streaming, expand rows with running tools and the
 * trailing activity row (covers inter-tool gaps). Earlier rows stay
 * collapsed — avoids fold→expand→fold when a later activity starts/stops.
 */
export function shouldPreferActivityExpanded(opts: {
  isTurnStreaming: boolean;
  runningCount: number;
  /** Last activity row in the current presentation slice. */
  isTrailingActivity: boolean;
}): boolean {
  if (!opts.isTurnStreaming) return false;
  if (opts.runningCount > 0) return true;
  return opts.isTrailingActivity;
}

/** @deprecated use shouldPreferActivityExpanded */
export function shouldAutoExpandActivityGroup(opts: {
  isTurnStreaming: boolean;
  runningCount: number;
}): boolean {
  return opts.isTurnStreaming && opts.runningCount > 0;
}

/**
 * Trailing activity during a live turn: Hold header stays put; reasoning/tools
 * stream inside a bounded panel (reduces transcript scroll jitter).
 */
export function shouldUseLiveHoldPanel(opts: {
  isTurnStreaming: boolean;
  isTrailingActivity: boolean;
}): boolean {
  return opts.isTurnStreaming && opts.isTrailingActivity;
}
