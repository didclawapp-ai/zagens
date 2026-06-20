import type { TraceCompareDocument, TraceDocument } from '../types';

declare global {
  interface Window {
    __ZAGENS_TRACE_BUNDLE__?: TraceDocument;
  }
}

const PLACEHOLDER = '__ZAGENS_TRACE_BUNDLE__';

export function isCompareDocument(doc: TraceDocument): doc is TraceCompareDocument {
  return (doc as TraceCompareDocument).document_kind === 'compare';
}

export function loadTraceDocument(): TraceDocument {
  if (window.__ZAGENS_TRACE_BUNDLE__) {
    return window.__ZAGENS_TRACE_BUNDLE__;
  }
  const el = document.getElementById('zagens-trace-bundle');
  if (!el?.textContent || el.textContent.includes(PLACEHOLDER)) {
    throw new Error(
      'Trace bundle not embedded — export with `zagens trace export` or `zagens trace compare`',
    );
  }
  const parsed = JSON.parse(el.textContent) as TraceDocument;
  window.__ZAGENS_TRACE_BUNDLE__ = parsed;
  return parsed;
}

/** @deprecated use loadTraceDocument */
export function loadTraceBundle() {
  const doc = loadTraceDocument();
  if (isCompareDocument(doc)) {
    throw new Error('Compare document loaded — use loadTraceDocument()');
  }
  return doc;
}

export function eventKind(payload: Record<string, unknown>): string {
  const kind = payload.event_type;
  return typeof kind === 'string' ? kind : 'unknown';
}

export function eventLabel(kind: string, payload: Record<string, unknown>): string {
  switch (kind) {
    case 'turn_started':
      return `Turn started · max_steps=${String(payload.max_steps ?? '?')}`;
    case 'turn_ended':
      return `Turn ended · ${String(payload.outcome ?? 'unknown')}`;
    case 'model_request_issued':
      return `Model request · step ${String(payload.step_idx ?? '?')}`;
    case 'model_message':
      return `Model message · step ${String(payload.step_idx ?? '?')}`;
    case 'tool_call_planned':
      return `Tool planned · ${String(payload.tool_name ?? '?')}`;
    case 'tool_call_started':
      return `Tool started · ${String(payload.call_id ?? '?')}`;
    case 'tool_call_finished':
      return `Tool finished · ${String(payload.tool_name ?? '?')}`;
    case 'steer_injected':
      return 'Steer injected (LHT nudge)';
    case 'step_limit_continuation':
      return 'Step limit continuation';
    case 'loop_guard_continuation':
      return 'Loop guard continuation';
    case 'loop_guard_triggered':
      return `Loop guard · ${String(payload.reason ?? '')}`;
    case 'capacity_checkpoint':
      return `Capacity checkpoint · ${String(payload.action ?? '')}`;
    default:
      return kind.replace(/_/g, ' ');
  }
}

export function classifyLane(kind: string): 'model' | 'tools' | 'guards' | null {
  if (
    kind === 'model_request_issued' ||
    kind === 'model_delta' ||
    kind === 'model_message'
  ) {
    return 'model';
  }
  if (
    kind === 'tool_call_planned' ||
    kind === 'tool_call_started' ||
    kind === 'tool_call_finished' ||
    kind === 'approval_resolved'
  ) {
    return 'tools';
  }
  if (
    kind === 'steer_injected' ||
    kind === 'step_limit_continuation' ||
    kind === 'loop_guard_continuation' ||
    kind === 'loop_guard_triggered' ||
    kind === 'capacity_checkpoint' ||
    kind === 'cycle_advanced' ||
    kind === 'deferred_tool_activated'
  ) {
    return 'guards';
  }
  return null;
}

export function groupEventsByLane(events: import('../types').TraceEventEnvelope[]) {
  const lanes = {
    model: [] as ReturnType<typeof toLaneEvent>[],
    tools: [] as ReturnType<typeof toLaneEvent>[],
    guards: [] as ReturnType<typeof toLaneEvent>[],
  };

  for (const envelope of events) {
    const payload = envelope.payload as Record<string, unknown>;
    const kind = eventKind(payload);
    const lane = classifyLane(kind);
    if (!lane) continue;
    lanes[lane].push(toLaneEvent(envelope, kind, payload));
  }

  return [
    { lane: 'model' as const, title: 'Model', events: lanes.model },
    { lane: 'tools' as const, title: 'Tools', events: lanes.tools },
    { lane: 'guards' as const, title: 'Guards / LHT', events: lanes.guards },
  ];
}

function toLaneEvent(
  envelope: import('../types').TraceEventEnvelope,
  kind: string,
  payload: Record<string, unknown>,
) {
  return {
    seq: envelope.seq,
    ts_ms: envelope.ts_ms,
    kind,
    label: eventLabel(kind, payload),
    payload,
  };
}

export function sourceLabel(bundle: import('../types').TraceBundle): string {
  if (bundle.source.kind === 'fixture' && bundle.source.fixture_path) {
    const parts = bundle.source.fixture_path.split(/[/\\]/);
    return parts[parts.length - 1] ?? bundle.source.fixture_path;
  }
  if (bundle.source.thread_id) {
    return bundle.source.thread_id;
  }
  return 'unknown';
}
