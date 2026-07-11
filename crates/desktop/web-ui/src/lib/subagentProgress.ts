/** Parse sub-agent `agent.progress` status lines into steps / tool hints. */

export type ParsedSubagentProgress = {
  stepsTaken?: number;
  maxSteps?: number;
  toolName?: string;
  toolPhase?: 'running' | 'finished';
  toolOk?: boolean;
};

const STEP_RE = /step\s+(\d+)\s*\/\s*(\d+)/i;
const TOOL_RUN_RE = /(?:running|finished)\s+tool\s+'([^']+)'/i;
const TOOL_OUTCOME_RE = /\((ok|error)\)\s*$/i;

export function parseSubagentProgressStatus(status: string): ParsedSubagentProgress {
  const text = status.trim();
  if (!text) {
    return {};
  }
  const out: ParsedSubagentProgress = {};
  const step = text.match(STEP_RE);
  if (step) {
    const done = Number(step[1]);
    const max = Number(step[2]);
    if (Number.isFinite(done) && done >= 0) {
      out.stepsTaken = done;
    }
    if (Number.isFinite(max) && max > 0) {
      out.maxSteps = max;
    }
  }
  const tool = text.match(TOOL_RUN_RE);
  if (tool?.[1]) {
    out.toolName = tool[1];
    out.toolPhase = /finished\s+tool/i.test(text) ? 'finished' : 'running';
    const outcome = text.match(TOOL_OUTCOME_RE);
    if (outcome) {
      out.toolOk = outcome[1]!.toLowerCase() === 'ok';
    }
  }
  return out;
}
