/**
 * Make dense assistant summaries easier to scan when the model omits blank lines.
 * Conservative heuristics only — avoids touching fenced code blocks.
 */

function splitOutsideFences(content: string): { text: string; fenced: boolean }[] {
  const parts: { text: string; fenced: boolean }[] = [];
  const re = /```[\s\S]*?```/g;
  let last = 0;
  for (const match of content.matchAll(re)) {
    const start = match.index ?? 0;
    if (start > last) {
      parts.push({ text: content.slice(last, start), fenced: false });
    }
    parts.push({ text: match[0], fenced: true });
    last = start + match[0].length;
  }
  if (last < content.length) {
    parts.push({ text: content.slice(last), fenced: false });
  }
  return parts.length > 0 ? parts : [{ text: content, fenced: false }];
}

function enhancePlainSegment(segment: string): string {
  let s = segment.replace(/\r\n/g, '\n');

  // Blank line before bold phase / task markers run into prior prose.
  s = s.replace(/([^\n])(\*\*(?:Phase\s*\d+|P\d+-T\d+|阶段\s*\d+|任务\s*[A-Z]?\d+))/gi, '$1\n\n$2');

  // Task ids (P1-T01) after CJK sentence end when the model omits markdown structure.
  s = s.replace(/([。！？；])(P\d+-T\d+\b)/g, '$1\n\n$2');

  // Phase labels after CJK sentence end.
  s = s.replace(/([。！？；])(Phase\s+\d+\b)/gi, '$1\n\n$2');

  // Blank line before markdown horizontal rules.
  s = s.replace(/([^\n])\s*(---+\s*(?:\n|$))/g, '$1\n\n$2');

  // List items after CJK sentence punctuation.
  s = s.replace(/([。！？；])\s*(-\s+\S)/g, '$1\n\n$2');

  // Numbered steps after CJK sentence punctuation.
  s = s.replace(/([。！？；])\s*(\d+\.\s+\*\*)/g, '$1\n\n$2');

  return s;
}

/** Insert paragraph breaks in assistant prose before markdown render. */
export function enhanceAssistantParagraphBreaks(content: string): string {
  if (!content.trim()) {
    return content;
  }
  return splitOutsideFences(content)
    .map((part) => (part.fenced ? part.text : enhancePlainSegment(part.text)))
    .join('')
    .trim();
}

/**
 * Merge a completed agent_message segment into accumulated turn text (thread replay).
 * Mirrors live SSE: one assistant bubble per turn, no duplicate flush on item.completed.
 */
export function mergeAgentMessageSegment(current: string, incoming: string): string {
  const next = incoming.trim();
  if (!next) {
    return current;
  }
  const cur = current.trim();
  if (!cur) {
    return next;
  }
  if (cur === next || cur.endsWith(next)) {
    return current;
  }
  if (next.startsWith(cur)) {
    return next;
  }
  if (cur.includes(next) && next.length < cur.length) {
    return current;
  }
  const sep = cur.endsWith('\n') ? '\n' : '\n\n';
  return `${current}${sep}${next}`;
}
