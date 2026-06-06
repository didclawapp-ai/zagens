import { replayThreadEvents } from '../../api/client';
import { normalizeDesktopStreamEvent } from '../../api/streamNormalize';
import {
  mergeStreamingToolOutput,
  stringifyToolInput,
  toolOutputString,
} from './toolOutput';
import {
  nextUiMessageId,
  resetUiMessageIdCounter,
  type UiMessage,
  type UiToolCall,
} from './sessionMessages';

interface HistoryState {
  messages: UiMessage[];
  currentAssistantId: string | null;
  assistantContent: string;
  assistantThinking: string;
  tools: UiToolCall[];
  currentToolId: string | null;
}

function flushAssistant(state: HistoryState): void {
  const hasBody =
    state.assistantContent.trim().length > 0 ||
    state.assistantThinking.trim().length > 0 ||
    state.tools.length > 0;
  if (!hasBody) {
    state.currentAssistantId = null;
    state.currentToolId = null;
    return;
  }
  const id = state.currentAssistantId ?? nextUiMessageId('asst');
  state.messages.push({
    id,
    role: 'assistant',
    content: state.assistantContent,
    ...(state.assistantThinking.trim() ? { thinking: state.assistantThinking } : {}),
    ...(state.tools.length > 0 ? { tools: [...state.tools] } : {}),
  });
  state.currentAssistantId = null;
  state.assistantContent = '';
  state.assistantThinking = '';
  state.tools = [];
  state.currentToolId = null;
}

function ensureAssistant(state: HistoryState): void {
  if (!state.currentAssistantId) {
    state.currentAssistantId = nextUiMessageId('asst');
  }
}

function upsertTool(
  state: HistoryState,
  id: string,
  patch: Partial<UiToolCall> & { name?: string; input?: string },
): void {
  ensureAssistant(state);
  const idx = state.tools.findIndex((t) => t.id === id);
  if (idx >= 0) {
    state.tools[idx] = { ...state.tools[idx], ...patch };
    return;
  }
  state.tools.push({
    id,
    name: patch.name ?? 'tool',
    input: patch.input ?? '',
    output: patch.output,
    status: patch.status ?? 'running',
  });
}

function applyNormalized(state: HistoryState, norm: ReturnType<typeof normalizeDesktopStreamEvent>): void {
  if (!norm) return;
  switch (norm.kind) {
    case 'turn_started':
    case 'turn_completed':
      flushAssistant(state);
      break;
    case 'thinking_delta':
      ensureAssistant(state);
      state.assistantThinking += norm.content;
      break;
    case 'message_delta':
      ensureAssistant(state);
      state.assistantContent += norm.content;
      break;
    case 'tool_started':
      ensureAssistant(state);
      state.currentToolId = norm.id;
      upsertTool(state, norm.id, {
        name: norm.name,
        input: stringifyToolInput(norm.input),
        status: 'running',
      });
      break;
    case 'tool_progress': {
      ensureAssistant(state);
      let idx = -1;
      if (state.currentToolId) {
        idx = state.tools.findIndex((t) => t.id === state.currentToolId);
      }
      if (idx < 0) {
        for (let i = state.tools.length - 1; i >= 0; i--) {
          if (state.tools[i].status === 'running') {
            idx = i;
            break;
          }
        }
      }
      if (idx >= 0) {
        const t = state.tools[idx];
        state.tools[idx] = { ...t, output: (t.output ?? '') + norm.output };
      }
      break;
    }
    case 'tool_completed': {
      ensureAssistant(state);
      const outStr = toolOutputString(norm.output);
      let targetId = norm.id;
      let existing = state.tools.find((t) => t.id === targetId);
      if (!existing) {
        for (let i = state.tools.length - 1; i >= 0; i--) {
          if (state.tools[i].status === 'running') {
            existing = state.tools[i];
            targetId = existing.id;
            break;
          }
        }
      }
      const merged = existing
        ? mergeStreamingToolOutput(existing.output ?? '', outStr)
        : outStr;
      upsertTool(state, targetId, {
        output: merged,
        status: norm.success ? 'done' : 'error',
      });
      if (state.currentToolId === targetId || state.currentToolId === norm.id) {
        state.currentToolId = null;
      }
      break;
    }
  }
}

function applyRawRecord(state: HistoryState, ev: { event: string; data: string }): void {
  let j: Record<string, unknown>;
  try {
    j = JSON.parse(ev.data) as Record<string, unknown>;
  } catch {
    return;
  }
  const recordEvent = (j.event as string | undefined) ?? ev.event;
  const inner = j.payload as Record<string, unknown> | undefined;
  if (!inner?.item || typeof inner.item !== 'object') {
    return;
  }
  const item = inner.item as Record<string, unknown>;
  if (recordEvent !== 'item.completed' && recordEvent !== 'item.failed') {
    return;
  }
  const kind = String(item.kind ?? '');
  const detail = typeof item.detail === 'string' ? item.detail : '';
  const summary = typeof item.summary === 'string' ? item.summary : '';

  if (kind === 'user_message') {
    flushAssistant(state);
    const text = (detail || summary).trim();
    if (text) {
      state.messages.push({
        id: nextUiMessageId('user'),
        role: 'user',
        content: text,
      });
    }
    return;
  }

  if (kind === 'agent_message') {
    ensureAssistant(state);
    if (detail.trim()) {
      state.assistantContent = detail;
    }
    flushAssistant(state);
    return;
  }

  if (
    kind === 'tool_call' ||
    kind === 'file_change' ||
    kind === 'command_execution'
  ) {
    const tool = inner.tool as Record<string, unknown> | undefined;
    let id = String(tool?.id ?? item.id ?? nextUiMessageId('tool'));
    const name = String(tool?.name ?? kind);
    const input = tool?.input != null ? stringifyToolInput(tool.input) : '';
    const outStr = (detail || summary).trim();
    let existing = state.tools.find((t) => t.id === id);
    if (!existing) {
      for (let i = state.tools.length - 1; i >= 0; i--) {
        if (state.tools[i].status === 'running') {
          existing = state.tools[i];
          id = existing.id;
          break;
        }
      }
    }
    const merged = existing
      ? mergeStreamingToolOutput(existing.output ?? '', outStr)
      : outStr;
    upsertTool(state, id, {
      name,
      input: input || existing?.input || '',
      output: merged || undefined,
      status: recordEvent === 'item.failed' ? 'error' : 'done',
    });
  }
}

function applyEvent(state: HistoryState, ev: { event: string; data: string }): void {
  applyRawRecord(state, ev);
  const norm = normalizeDesktopStreamEvent(ev);
  applyNormalized(state, norm);
}

/**
 * Rebuild chat UI messages (including tool cards) from persisted runtime thread events.
 * Authoritative for tool output; session JSON only stores text/thinking blocks today.
 */
export async function rebuildMessagesFromThreadEvents(
  threadId: string,
  options?: { signal?: AbortSignal },
): Promise<UiMessage[]> {
  resetUiMessageIdCounter();
  const state: HistoryState = {
    messages: [],
    currentAssistantId: null,
    assistantContent: '',
    assistantThinking: '',
    tools: [],
    currentToolId: null,
  };

  await replayThreadEvents(
    threadId,
    0,
    (ev) => {
      applyEvent(state, ev);
    },
    { signal: options?.signal, waitForStreamClose: true },
  );

  flushAssistant(state);
  return state.messages;
}
