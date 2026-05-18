/** Keeps milestone lines from `tool.progress` visible above the final tool result. */
export const TOOL_OUTPUT_MERGE_SEPARATOR = '\n\n────────────────────────────────\n\n';

/** Cap tool output kept in React state so edit_file progress cannot freeze the UI. */
export const MAX_TOOL_OUTPUT_DISPLAY_CHARS = 96_000;

const TRUNC_HEAD = '…[earlier tool output truncated for UI performance]\n\n';

export function capToolOutputForDisplay(text: string, maxChars = MAX_TOOL_OUTPUT_DISPLAY_CHARS): string {
  if (text.length <= maxChars) {
    return text;
  }
  return TRUNC_HEAD + text.slice(-maxChars);
}

export function appendCappedToolOutput(prev: string, chunk: string): string {
  return capToolOutputForDisplay(prev + chunk);
}

/** Merge streamed shell/tool progress with the final payload without duplicating the shared suffix. */
export function mergeStreamingToolOutput(prevRaw: string, finalRaw: string): string {
  const prev = prevRaw.trimEnd();
  const fin = finalRaw.trimEnd();
  if (!prev) return finalRaw;
  if (!fin) return prevRaw;
  const pt = prev.trim();
  const ft = fin.trim();
  if (ft.length >= 16 && pt.endsWith(ft)) return prevRaw;
  if (pt.length >= ft.length && pt.endsWith(ft)) return prevRaw;
  if (fin.startsWith(prev)) return finalRaw;
  return `${prevRaw.trimEnd()}${TOOL_OUTPUT_MERGE_SEPARATOR}${finalRaw}`;
}

export function toolOutputString(output: unknown): string {
  if (output == null) {
    return '';
  }
  if (typeof output === 'string') {
    return output;
  }
  try {
    return JSON.stringify(output, null, 2);
  } catch {
    return String(output);
  }
}

export function stringifyToolInput(input: unknown): string {
  if (input == null || input === '') {
    return '';
  }
  if (typeof input === 'string') {
    return input;
  }
  try {
    return JSON.stringify(input, null, 2);
  } catch {
    return String(input);
  }
}
