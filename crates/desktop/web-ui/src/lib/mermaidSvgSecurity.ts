/**
 * Lightweight scan for obviously malicious content in Mermaid SVG output.
 * Used on trusted preview paths instead of full DOMPurify (which strips theme CSS).
 */

/** Thrown when SVG scan finds suspicious markup; carries the raw SVG for opt-in render. */
export class MermaidSvgThreatError extends Error {
  readonly code = 'MERMAID_SVG_THREAT' as const;

  constructor(
    message: string,
    readonly svg: string,
    readonly reason: string,
  ) {
    super(message);
    this.name = 'MermaidSvgThreatError';
  }
}

export function isMermaidSvgThreatError(err: unknown): err is MermaidSvgThreatError {
  return err instanceof MermaidSvgThreatError;
}

const THREAT_CHECKS: ReadonlyArray<{ id: string; pattern: RegExp }> = [
  { id: 'script', pattern: /<script[\s>]/i },
  { id: 'iframe', pattern: /<iframe[\s>]/i },
  { id: 'embed', pattern: /<embed[\s>]/i },
  { id: 'object', pattern: /<object[\s>]/i },
  { id: 'event-handler', pattern: /\s(on\w+)\s*=/i },
  { id: 'javascript-url', pattern: /javascript\s*:/i },
  { id: 'vbscript-url', pattern: /vbscript\s*:/i },
  { id: 'data-html', pattern: /data\s*:\s*text\/html/i },
];

/** Return a short reason id when suspicious; null when scan passes. */
export function scanMermaidSvgThreats(svg: string): string | null {
  for (const { id, pattern } of THREAT_CHECKS) {
    if (pattern.test(svg)) {
      return id;
    }
  }
  return null;
}

/** Run scan; throw {@link MermaidSvgThreatError} when blocked. */
export function assertMermaidSvgSafe(svg: string, reasonLabel?: (id: string) => string): void {
  const hit = scanMermaidSvgThreats(svg);
  if (!hit) {
    return;
  }
  const label = reasonLabel?.(hit) ?? hit;
  throw new MermaidSvgThreatError(label, svg, hit);
}
