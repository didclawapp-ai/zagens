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

/** Opening cue used to detect rewritten final-report duplicates. */
function proseHeadCue(text: string): string {
  const s = text.replace(/\r\n/g, '\n').trim();
  const cjk = s.match(/^(.+?[。！？])/);
  if (cjk?.[1] && cjk[1].length >= 4) return cjk[1];
  const latin = s.match(/^(.+?[.!?])(?:\s|$)/);
  if (latin?.[1] && latin[1].length >= 8) return latin[1].trim();
  return s.slice(0, 48).trim();
}

/**
 * True when two long prose blobs are the same report with minor edits
 * (e.g. path typo fixes) — not two distinct sections of one answer.
 */
export function isNearDuplicateProse(a: string, b: string): boolean {
  const x = a.replace(/\r\n/g, '\n').trim();
  const y = b.replace(/\r\n/g, '\n').trim();
  if (!x || !y) return false;
  if (x === y) return true;
  if (Math.min(x.length, y.length) < 200) return false;
  const ratio = x.length / y.length;
  if (ratio < 0.45 || ratio > 2.2) return false;

  const hx = proseHeadCue(x);
  const hy = proseHeadCue(y);
  if (hx.length >= 12 && (hx === hy || x.startsWith(hy) || y.startsWith(hx))) {
    return true;
  }

  const nx = x.replace(/\s+/g, ' ');
  const ny = y.replace(/\s+/g, ' ');
  const probe = Math.min(96, Math.floor(Math.min(nx.length, ny.length) * 0.25));
  if (probe >= 48) {
    if (nx.includes(ny.slice(0, probe)) || ny.includes(nx.slice(0, probe))) {
      return true;
    }
  }
  return false;
}

/**
 * If a single text block is two near-duplicate final reports joined by blank lines,
 * keep the longer copy. Safe for normal multi-section answers (halves won't match).
 */
export function collapseNearDuplicateReport(content: string): string {
  const s = content.replace(/\r\n/g, '\n').trim();
  if (s.length < 400) return content;

  const re = /\n{2,}/g;
  let match: RegExpExecArray | null;
  while ((match = re.exec(s)) !== null) {
    const left = s.slice(0, match.index).trim();
    const right = s.slice(match.index + match[0].length).trim();
    if (left.length >= 200 && right.length >= 200 && isNearDuplicateProse(left, right)) {
      return left.length >= right.length ? left : right;
    }
  }
  return content;
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
  // Rewritten final report (same opening, minor path/typo diffs) — keep the longer copy.
  if (isNearDuplicateProse(cur, next)) {
    return next.length >= cur.length ? next : current;
  }
  const sep = cur.endsWith('\n') ? '\n' : '\n\n';
  return `${current}${sep}${next}`;
}

/**
 * Append an incremental streaming text delta without duplicating replay/coalesced chunks.
 */
export function appendStreamingTextDelta(current: string, incoming: string): string {
  if (!incoming) {
    return current;
  }
  if (!current) {
    return incoming;
  }
  if (incoming === current) {
    return current;
  }
  if (current.endsWith(incoming)) {
    return current;
  }
  if (incoming.startsWith(current)) {
    return incoming;
  }
  if (current.includes(incoming) && incoming.length >= 16) {
    return current;
  }
  return current + incoming;
}
