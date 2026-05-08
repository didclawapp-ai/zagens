/** Normalizes compat (`POST /v1/stream`) and raw (`GET …/events`) SSE payloads for one UI pipeline. */

export type NormalizedStreamEvent =
  | { kind: 'turn_started'; threadId: string; turnId: string }
  | { kind: 'message_delta'; content: string }
  | { kind: 'tool_started'; id: string; name: string; input: unknown }
  | { kind: 'tool_progress'; output: string }
  | { kind: 'tool_completed'; id: string; success: boolean; output: unknown }
  | { kind: 'approval_required'; id: string; toolName: string; description: string }
  | { kind: 'turn_completed' }
  | { kind: 'done' }
  | { kind: 'error'; message: string }
  | { kind: 'status'; message: string };

export function normalizeDesktopStreamEvent(
  ev: { event: string; data: string },
  filter?: { turnId?: string },
): NormalizedStreamEvent | null {
  let j: Record<string, unknown>;
  try {
    j = JSON.parse(ev.data) as Record<string, unknown>;
  } catch {
    return null;
  }

  const turnId = j.turn_id != null ? String(j.turn_id) : undefined;
  if (filter?.turnId && turnId !== undefined && turnId !== filter.turnId) {
    return null;
  }

  const sse = ev.event;

  if (sse === 'turn.started') {
    return {
      kind: 'turn_started',
      threadId: String(j.thread_id ?? ''),
      turnId: String(j.turn_id ?? ''),
    };
  }
  if (sse === 'done') {
    return { kind: 'done' };
  }
  if (sse === 'message.delta') {
    return { kind: 'message_delta', content: String(j.content ?? '') };
  }
  if (sse === 'tool.progress') {
    return { kind: 'tool_progress', output: String(j.output ?? '') };
  }
  if (sse === 'tool.started') {
    return {
      kind: 'tool_started',
      id: String(j.id ?? ''),
      name: String(j.name ?? ''),
      input: j.input,
    };
  }
  if (sse === 'tool.completed') {
    return {
      kind: 'tool_completed',
      id: String(j.id ?? ''),
      success: Boolean(j.success),
      output: j.output ?? null,
    };
  }
  if (sse === 'approval.required') {
    const p = (j.payload as Record<string, unknown> | undefined) ?? j;
    return {
      kind: 'approval_required',
      id: String(p.id ?? ''),
      toolName: String(p.tool_name ?? ''),
      description: String(p.description ?? ''),
    };
  }
  if (sse === 'turn.completed') {
    return { kind: 'turn_completed' };
  }
  if (sse === 'error') {
    return { kind: 'error', message: String(j.message ?? '') };
  }
  if (sse === 'status') {
    return { kind: 'status', message: String(j.message ?? '') };
  }

  // —— Raw runtime records from `GET /v1/threads/{id}/events` ——
  const inner = j.payload as Record<string, unknown> | undefined;
  const recordEvent = (j.event as string | undefined) ?? sse;

  if (recordEvent === 'item.delta' && inner) {
    const kind = String(inner.kind ?? '');
    if (kind === 'agent_message') {
      return { kind: 'message_delta', content: String(inner.delta ?? '') };
    }
    if (kind === 'tool_call') {
      return { kind: 'tool_progress', output: String(inner.delta ?? '') };
    }
    return null;
  }
  if (recordEvent === 'item.started' && inner) {
    const tool = inner.tool as Record<string, unknown> | undefined;
    if (!tool) {
      return null;
    }
    return {
      kind: 'tool_started',
      id: String(tool.id ?? ''),
      name: String(tool.name ?? ''),
      input: tool.input,
    };
  }
  if ((recordEvent === 'item.completed' || recordEvent === 'item.failed') && inner) {
    const item = inner.item as Record<string, unknown> | undefined;
    if (!item) {
      return null;
    }
    const kind = String(item.kind ?? '');
    if (kind === 'tool_call' || kind === 'file_change' || kind === 'command_execution') {
      return {
        kind: 'tool_completed',
        id: String(item.id ?? ''),
        success: recordEvent === 'item.completed',
        output: item.detail ?? item.summary ?? '',
      };
    }
    if (kind === 'error') {
      const msg =
        (item.detail as string | undefined) ?? (item.summary as string | undefined) ?? 'error';
      return { kind: 'error', message: msg };
    }
    if (kind === 'status') {
      const msg =
        (item.detail as string | undefined) ??
        (item.summary as string | undefined) ??
        '';
      return { kind: 'status', message: msg };
    }
    return null;
  }
  if (recordEvent === 'approval.required') {
    const p = (inner && typeof inner.id !== 'undefined' ? inner : j) as Record<string, unknown>;
    return {
      kind: 'approval_required',
      id: String(p.id ?? ''),
      toolName: String(p.tool_name ?? ''),
      description: String(p.description ?? ''),
    };
  }
  if (recordEvent === 'turn.completed') {
    return { kind: 'turn_completed' };
  }

  return null;
}
