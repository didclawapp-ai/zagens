/** Normalizes compat (`POST /v1/stream`) and raw (`GET …/events`) SSE payloads for one UI pipeline. */

function normalizeSubAgentStatus(status: unknown): string {
  if (typeof status === 'string') {
    return status;
  }
  if (status && typeof status === 'object') {
    const keys = Object.keys(status as Record<string, unknown>);
    if (keys.length === 1) {
      return keys[0] ?? 'Running';
    }
  }
  return 'Running';
}

/** Per-turn token usage from the runtime API `turn.completed` event. */
export interface TurnUsage {
  input_tokens: number;
  output_tokens: number;
  prompt_cache_hit_tokens?: number;
  prompt_cache_miss_tokens?: number;
  reasoning_tokens?: number;
  reasoning_replay_tokens?: number;
}

export type NormalizedStreamEvent =
  | { kind: 'turn_started'; threadId: string; turnId: string }
  | { kind: 'thinking_delta'; content: string }
  | { kind: 'message_delta'; content: string }
  | { kind: 'tool_started'; id: string; name: string; input: unknown }
  | { kind: 'tool_progress'; output: string }
  | { kind: 'tool_completed'; id: string; success: boolean; output: unknown }
  | { kind: 'approval_required'; id: string; toolName: string; description: string }
  | { kind: 'turn_completed'; usage?: TurnUsage }
  | { kind: 'done' }
  | { kind: 'error'; message: string }
  | { kind: 'status'; message: string }
  | { kind: 'agent_spawned'; agentId: string }
  | { kind: 'agent_progress'; agentId: string }
  | { kind: 'agent_completed'; agentId: string; result: string }
  | { kind: 'agent_list'; agents: Array<{ id: string; status: string }> }
  | { kind: 'panel_scratchpad'; scratchpad: unknown }
  | { kind: 'panel_checklist'; checklist: unknown }
  | { kind: 'panel_context'; context: unknown };

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
  if (sse === 'thinking.delta') {
    return { kind: 'thinking_delta', content: String(j.content ?? '') };
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
    const usage = parseTurnUsage(j.usage as Record<string, unknown> | undefined);
    return { kind: 'turn_completed', usage };
  }
  if (sse === 'error') {
    return { kind: 'error', message: String(j.message ?? '') };
  }
  if (sse === 'status') {
    return { kind: 'status', message: String(j.message ?? '') };
  }

  // —— Compat SSE from `POST /v1/stream` (map_compat_stream_event) ——
  if (sse === 'agent.spawned') {
    const agentId = String(j.agent_id ?? '');
    if (agentId) return { kind: 'agent_spawned', agentId };
  }
  if (sse === 'agent.progress') {
    const agentId = String(j.agent_id ?? '');
    if (agentId) return { kind: 'agent_progress', agentId };
  }
  if (sse === 'agent.completed') {
    const agentId = String(j.agent_id ?? '');
    if (agentId) {
      return { kind: 'agent_completed', agentId, result: String(j.result ?? '') };
    }
  }
  if (sse === 'agent.list') {
    const raw = j.agents as Array<Record<string, unknown>> | undefined;
    if (raw) {
      return {
        kind: 'agent_list',
        agents: raw.map((a) => ({
          id: String(a.agent_id ?? a.id ?? ''),
          status: normalizeSubAgentStatus(a.status),
        })),
      };
    }
  }

  if (sse === 'panel.scratchpad' && j.scratchpad != null) {
    return { kind: 'panel_scratchpad', scratchpad: j.scratchpad };
  }
  if (sse === 'panel.checklist' && j.checklist != null) {
    return { kind: 'panel_checklist', checklist: j.checklist };
  }
  if (sse === 'panel.context' && j.context != null) {
    return { kind: 'panel_context', context: j.context };
  }

  // —— Raw runtime records from `GET /v1/threads/{id}/events` ——
  const inner = j.payload as Record<string, unknown> | undefined;
  const recordEvent = (j.event as string | undefined) ?? sse;

  if (recordEvent === 'item.delta' && inner) {
    const kind = String(inner.kind ?? '');
    if (kind === 'agent_message') {
      return { kind: 'message_delta', content: String(inner.delta ?? '') };
    }
    if (kind === 'thinking') {
      return { kind: 'thinking_delta', content: String(inner.delta ?? '') };
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
    const toolName = String(tool.name ?? '');
    const spawnNames = new Set(['agent_spawn', 'spawn_agent', 'delegate_to_agent']);
    if (spawnNames.has(toolName)) {
      const input = tool.input as Record<string, unknown> | undefined;
      const agentId = String(
        tool.id ?? input?.agent_id ?? input?.id ?? j.agent_id ?? '',
      );
      if (agentId) return { kind: 'agent_spawned', agentId };
    }
    return {
      kind: 'tool_started',
      id: String(tool.id ?? ''),
      name: toolName,
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
      const tool = inner.tool as Record<string, unknown> | undefined;
      return {
        kind: 'tool_completed',
        id: String(tool?.id ?? item.id ?? ''),
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
    const rawUsage = (inner?.turn as Record<string, unknown> | undefined)
      ?.usage as Record<string, unknown> | undefined;
    const usage = parseTurnUsage(rawUsage);
    return { kind: 'turn_completed', usage };
  }

  // —— agent.* events ———
  if (recordEvent === 'agent.spawned') {
    const agentId = String((inner ?? j).agent_id ?? '');
    if (agentId) return { kind: 'agent_spawned', agentId };
  }
  if (recordEvent === 'agent.progress') {
    const agentId = String((inner ?? j).agent_id ?? '');
    if (agentId) return { kind: 'agent_progress', agentId };
  }
  if (recordEvent === 'agent.completed') {
    const agentId = String((inner ?? j).agent_id ?? '');
    if (agentId) {
      const item = inner?.item as Record<string, unknown> | undefined;
      const result = String(item?.detail ?? item?.summary ?? '');
      return { kind: 'agent_completed', agentId, result };
    }
  }
  if (recordEvent === 'agent.list') {
    const raw = (inner ?? j).agents as Array<Record<string, unknown>> | undefined;
    if (raw) {
      return {
        kind: 'agent_list',
        agents: raw.map((a) => ({
          id: String(a.agent_id ?? a.id ?? ''),
          status: normalizeSubAgentStatus(a.status),
        })),
      };
    }
  }

  if (recordEvent === 'panel.scratchpad' && inner) {
    const scratchpad = (inner.scratchpad ?? inner) as unknown;
    return { kind: 'panel_scratchpad', scratchpad };
  }
  if (recordEvent === 'panel.checklist' && inner) {
    const checklist = (inner.checklist ?? inner) as unknown;
    return { kind: 'panel_checklist', checklist };
  }
  if (recordEvent === 'panel.context' && inner) {
    const context = (inner.context ?? inner) as unknown;
    return { kind: 'panel_context', context };
  }

  // Detect tool calls that are agent_spawn and emit agent_spawned
  if (recordEvent === 'item.started' && inner) {
    const tool = inner.tool as Record<string, unknown> | undefined;
    const toolName = String(tool?.name ?? '');
    if (toolName === 'agent_spawn' || toolName === 'spawn_agent' || toolName === 'delegate_to_agent') {
      const agentId = String(j.agent_id ?? tool?.id ?? '');
      if (agentId) return { kind: 'agent_spawned', agentId };
    }
  }

  return null;
}

/** Extract a TurnUsage from a raw JSON object, handling type coercion. */
function parseTurnUsage(raw: Record<string, unknown> | undefined): TurnUsage | undefined {
  if (!raw) return undefined;
  const input = Number(raw.input_tokens);
  const output = Number(raw.output_tokens);
  if (!Number.isFinite(input) && !Number.isFinite(output)) return undefined;
  return {
    input_tokens: Number.isFinite(input) ? input : 0,
    output_tokens: Number.isFinite(output) ? output : 0,
    prompt_cache_hit_tokens: toOptionalNum(raw.prompt_cache_hit_tokens),
    prompt_cache_miss_tokens: toOptionalNum(raw.prompt_cache_miss_tokens),
    reasoning_tokens: toOptionalNum(raw.reasoning_tokens),
    reasoning_replay_tokens: toOptionalNum(raw.reasoning_replay_tokens),
  };
}

function toOptionalNum(v: unknown): number | undefined {
  if (v == null) return undefined;
  const n = Number(v);
  return Number.isFinite(n) ? n : undefined;
}