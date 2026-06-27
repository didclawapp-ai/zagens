/** Normalizes compat (`POST /v1/stream`) and raw (`GET …/events`) SSE payloads for one UI pipeline.
 *
 * A+.3: Only v1 event names from API_DESIGN.md §3.2.1 are mapped; unknown `event:` names return
 * `null` so callers can ignore them without failing the stream.
 */

import { parseAgentListRow, type AgentListRowMeta } from '../lib/agentSpawnMeta';
import {
  normalizeThreadStreamStatus,
  type ThreadStreamStatus,
} from '../lib/chat/threadStatusStore';

export type { ThreadStreamStatus };

/** Stable SSE subset (API_DESIGN.md §3.2.1). Used for tests and documentation. */
export const KNOWN_DESKTOP_SSE_EVENTS = new Set([
  'turn.started',
  'thinking.delta',
  'message.delta',
  'message.segment',
  'tool.progress',
  'tool.started',
  'tool.completed',
  'status',
  'error',
  'approval.required',
  'sandbox.denied',
  'turn.completed',
  'agent.spawned',
  'agent.progress',
  'agent.completed',
  'agent.list',
  'craft.verdict',
  'craft.board_updated',
  'panel.checklist',
  'panel.plan',
  'panel.scratchpad',
  'panel.context',
  'context.usage',
  'harness.task_graph',
  'harness.cycle_advanced',
  'thread.status',
  'done',
]);

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
  | { kind: 'message_segment'; content: string }
  | { kind: 'tool_started'; id: string; name: string; input: unknown }
  | { kind: 'tool_progress'; output: string }
  | { kind: 'tool_completed'; id: string; success: boolean; output: unknown }
  | { kind: 'approval_required'; id: string; toolName: string; description: string }
  | { kind: 'turn_completed'; usage?: TurnUsage }
  | { kind: 'done' }
  | { kind: 'error'; message: string }
  | { kind: 'status'; message: string }
  | { kind: 'agent_spawned'; agentId: string; prompt?: string }
  | { kind: 'agent_progress'; agentId: string; status?: string }
  | { kind: 'agent_completed'; agentId: string; result: string }
  | { kind: 'agent_list'; agents: AgentListRowMeta[] }
  | { kind: 'craft_verdict'; agentId: string; agentType: string; taskId?: string; verdict: string }
  | { kind: 'craft_board_updated'; taskId: string; partition: string; agentId: string }
  | { kind: 'panel_scratchpad'; scratchpad: unknown }
  | { kind: 'panel_checklist'; checklist: unknown }
  | { kind: 'panel_context'; context: unknown }
  | { kind: 'context_usage'; usage: unknown }
  | { kind: 'panel_task_graph'; task_graph: unknown }
  | { kind: 'harness_cycle_advanced'; from: number; to: number }
  | { kind: 'thread_status'; threadId: string; status: ThreadStreamStatus; turnId?: string; seq?: number };

function resolveEventSeq(
  ev: { seq?: number; data: string },
  j: Record<string, unknown>,
): number | undefined {
  if (typeof ev.seq === 'number') {
    return ev.seq;
  }
  if (typeof j.seq === 'number') {
    return j.seq;
  }
  const inner = j.payload as Record<string, unknown> | undefined;
  if (inner && typeof inner.seq === 'number') {
    return inner.seq;
  }
  return undefined;
}

export function normalizeDesktopStreamEvent(
  ev: { event: string; data: string; seq?: number },
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
  if (sse === 'message.segment') {
    return { kind: 'message_segment', content: String(j.content ?? '') };
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
  if (sse === 'thread.status') {
    const inner = j.payload as Record<string, unknown> | undefined;
    const payload = (inner ?? j) as Record<string, unknown>;
    const threadId = String(j.thread_id ?? payload.thread_id ?? '');
    const status = normalizeThreadStreamStatus(payload.status ?? j.status);
    if (!threadId || !status) return null;
    const turnIdRaw = j.turn_id ?? payload.turn_id;
    const turnId = turnIdRaw != null ? String(turnIdRaw) : undefined;
    const seq = resolveEventSeq(ev, j);
    return {
      kind: 'thread_status',
      threadId,
      status,
      ...(turnId ? { turnId } : {}),
      ...(seq != null ? { seq } : {}),
    };
  }

  // —— Compat SSE from `POST /v1/stream` (map_compat_stream_event) ——
  if (sse === 'agent.spawned') {
    const agentId = String(j.agent_id ?? '');
    if (agentId) {
      const prompt = spawnPromptFromPayload(j);
      return { kind: 'agent_spawned', agentId, ...(prompt ? { prompt } : {}) };
    }
  }
  if (sse === 'agent.progress') {
    const agentId = String(j.agent_id ?? '');
    if (agentId) {
      const status = String(j.status ?? '').trim();
      return {
        kind: 'agent_progress',
        agentId,
        ...(status ? { status } : {}),
      };
    }
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
        agents: raw.map((a) => parseAgentListRow(a)),
      };
    }
  }
  if (sse === 'craft.verdict') {
    const agentId = String(j.agent_id ?? '');
    if (!agentId) return null;
    return {
      kind: 'craft_verdict',
      agentId,
      agentType: String(j.agent_type ?? ''),
      taskId: j.task_id != null ? String(j.task_id) : undefined,
      verdict: String(j.verdict ?? ''),
    };
  }
  if (sse === 'craft.board_updated') {
    const taskId = String(j.task_id ?? '');
    if (!taskId) return null;
    return {
      kind: 'craft_board_updated',
      taskId,
      partition: String(j.partition ?? ''),
      agentId: String(j.agent_id ?? ''),
    };
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
  if (sse === 'context.usage' && j.usage != null) {
    return { kind: 'context_usage', usage: j.usage };
  }
  if (sse === 'harness.task_graph' && j.task_graph != null) {
    return { kind: 'panel_task_graph', task_graph: j.task_graph };
  }
  if (sse === 'harness.cycle_advanced') {
    const from = Number(j.from ?? 0);
    const to = Number(j.to ?? 0);
    return { kind: 'harness_cycle_advanced', from, to };
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
    if (kind === 'thinking') {
      // Live stream already applied incremental `item.delta` chunks; replay uses
      // `rebuildMessagesFromThread.applyRawRecord` for the completed item body.
      return null;
    }
    if (kind === 'agent_message') {
      const text =
        (item.detail as string | undefined) ??
        (item.summary as string | undefined) ??
        '';
      const trimmed = text.trim();
      if (!trimmed) {
        return null;
      }
      return { kind: 'message_segment', content: trimmed };
    }
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
    const payload = (inner ?? j) as Record<string, unknown>;
    const agentId = String(payload.agent_id ?? '');
    if (agentId) {
      const prompt = spawnPromptFromPayload(payload);
      return { kind: 'agent_spawned', agentId, ...(prompt ? { prompt } : {}) };
    }
  }
  if (recordEvent === 'agent.progress') {
    const payload = (inner ?? j) as Record<string, unknown>;
    const agentId = String(payload.agent_id ?? '');
    if (agentId) {
      const status = String(payload.status ?? '').trim();
      return {
        kind: 'agent_progress',
        agentId,
        ...(status ? { status } : {}),
      };
    }
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
        agents: raw.map((a) => parseAgentListRow(a)),
      };
    }
  }
  if (recordEvent === 'craft.verdict') {
    const payload = (inner ?? j) as Record<string, unknown>;
    const agentId = String(payload.agent_id ?? '');
    if (agentId) {
      return {
        kind: 'craft_verdict',
        agentId,
        agentType: String(payload.agent_type ?? ''),
        taskId: payload.task_id != null ? String(payload.task_id) : undefined,
        verdict: String(payload.verdict ?? ''),
      };
    }
  }
  if (recordEvent === 'craft.board_updated') {
    const payload = (inner ?? j) as Record<string, unknown>;
    const taskId = String(payload.task_id ?? '');
    if (taskId) {
      return {
        kind: 'craft_board_updated',
        taskId,
        partition: String(payload.partition ?? ''),
        agentId: String(payload.agent_id ?? ''),
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
  if (recordEvent === 'context.usage' && inner) {
    const usage = (inner.usage ?? inner) as unknown;
    return { kind: 'context_usage', usage };
  }
  if (recordEvent === 'harness.task_graph' && inner) {
    const task_graph = (inner.task_graph ?? inner) as unknown;
    return { kind: 'panel_task_graph', task_graph };
  }
  if (recordEvent === 'harness.cycle_advanced' && inner) {
    const from = Number(inner.from ?? 0);
    const to = Number(inner.to ?? 0);
    return { kind: 'harness_cycle_advanced', from, to };
  }
  if (recordEvent === 'thread.status') {
    const payload = (inner ?? j) as Record<string, unknown>;
    const threadId = String(j.thread_id ?? '');
    const status = normalizeThreadStreamStatus(payload.status);
    if (!threadId || !status) return null;
    const turnId = j.turn_id != null ? String(j.turn_id) : undefined;
    const seq = resolveEventSeq(ev, j);
    return {
      kind: 'thread_status',
      threadId,
      status,
      ...(turnId ? { turnId } : {}),
      ...(seq != null ? { seq } : {}),
    };
  }

  return null;
}

function spawnPromptFromPayload(payload: Record<string, unknown>): string | undefined {
  const direct = payload.prompt;
  if (typeof direct === 'string' && direct.trim()) {
    return direct.trim();
  }
  const item = payload.item as Record<string, unknown> | undefined;
  const detail = item?.detail ?? item?.summary;
  if (typeof detail === 'string') {
    const m = detail.match(/spawned:\s*(.+)/i);
    if (m?.[1]?.trim()) {
      return m[1].trim();
    }
  }
  return undefined;
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