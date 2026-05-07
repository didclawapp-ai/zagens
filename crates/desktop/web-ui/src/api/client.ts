export interface SseTurnEvent {
  event: string;
  data: string;
}

export interface StreamTurnRequest {
  prompt: string;
  workspace: string;
  mode: string;
  model?: string;
  auto_approve?: boolean;
}

export interface SessionInfo {
  id: string;
  name: string;
  created_at?: number;
  updated_at?: number;
}

export interface SessionDetailMessage {
  role: string;
  content: Array<{ type: string; text?: string }>;
}

export interface SessionDetail {
  metadata: SessionInfo & { title?: string };
  messages: SessionDetailMessage[];
  system_prompt?: string | null;
}

let runtimeBase = 'http://127.0.0.1:7878';
let runtimeToken = '';

/** Call before render when running inside Tauri; no-op in plain Vite dev. */
export async function initRuntimeConfig(): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const [port, token] = await Promise.all([
      invoke<number>('get_runtime_port'),
      invoke<string>('get_runtime_token'),
    ]);
    runtimeBase = `http://127.0.0.1:${port}`;
    runtimeToken = token;
    (window as unknown as { __DEEPSEEK_RUNTIME_TOKEN__?: string }).__DEEPSEEK_RUNTIME_TOKEN__ =
      token;
  } catch {
    /* Not in Tauri webview — rely on optional window global or open API (no auth). */
    runtimeToken =
      (window as unknown as { __DEEPSEEK_RUNTIME_TOKEN__?: string }).__DEEPSEEK_RUNTIME_TOKEN__ ||
      '';
  }
}

export function getRuntimeBase(): string {
  return runtimeBase;
}

function authHeaders(): Record<string, string> {
  if (!runtimeToken) {
    return { 'Content-Type': 'application/json' };
  }
  return {
    'Content-Type': 'application/json',
    Authorization: `Bearer ${runtimeToken}`,
  };
}

/** Drain complete SSE blocks (`\n\n` delimited); `rest` is incomplete tail. */
function drainSseBlocks(buffer: string): { drained: SseTurnEvent[]; rest: string } {
  const drained: SseTurnEvent[] = [];
  const sep = '\n\n';
  let rest = buffer;
  let idx: number;
  while ((idx = rest.indexOf(sep)) !== -1) {
    const block = rest.slice(0, idx);
    rest = rest.slice(idx + sep.length);
    let eventName = 'message';
    const dataLines: string[] = [];
    for (const line of block.split(/\r?\n/)) {
      if (line.startsWith('event:')) {
        eventName = line.slice(6).trim();
      } else if (line.startsWith('data:')) {
        dataLines.push(line.slice(5).trimStart());
      }
    }
    if (dataLines.length) {
      drained.push({ event: eventName, data: dataLines.join('\n') });
    }
  }
  return { drained, rest };
}

export async function postStreamTurn(
  req: StreamTurnRequest,
  onEvent: (event: SseTurnEvent) => void,
  onDone: () => void,
  onError: (err: Error) => void,
): Promise<void> {
  try {
    const response = await fetch(`${runtimeBase}/v1/stream`, {
      method: 'POST',
      headers: authHeaders(),
      body: JSON.stringify(req),
    });

    if (!response.ok) {
      const text = await response.text();
      const err = new Error(`HTTP ${response.status}: ${text}`);
      (err as Error & { status?: number }).status = response.status;
      throw err;
    }

    const reader = response.body?.getReader();
    if (!reader) {
      throw new Error('No response body');
    }

    const decoder = new TextDecoder();
    let buffer = '';

    while (true) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }
      buffer += decoder.decode(value, { stream: true });
      const { drained, rest } = drainSseBlocks(buffer);
      buffer = rest;
      for (const ev of drained) {
        onEvent(ev);
      }
    }

    const { drained: tail } = drainSseBlocks(buffer + '\n\n');
    for (const ev of tail) {
      onEvent(ev);
    }

    onDone();
  } catch (err) {
    onError(err instanceof Error ? err : new Error(String(err)));
  }
}

export async function fetchJson<T>(path: string): Promise<T> {
  const res = await fetch(`${runtimeBase}${path}`, {
    headers: authHeaders(),
  });
  if (!res.ok) {
    const text = await res.text();
    const err = new Error(`HTTP ${res.status}: ${text}`);
    (err as Error & { status?: number }).status = res.status;
    throw err;
  }
  return res.json();
}

export async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${runtimeBase}${path}`, {
    method: 'POST',
    headers: authHeaders(),
    body: JSON.stringify(body ?? {}),
  });
  if (!res.ok) {
    const text = await res.text();
    const err = new Error(`HTTP ${res.status}: ${text}`);
    (err as Error & { status?: number }).status = res.status;
    throw err;
  }
  return res.json();
}

export async function getSessions(): Promise<SessionInfo[]> {
  const data = await fetchJson<{ sessions: Array<{ id: string; title: string }> }>('/v1/sessions');
  const rows = data.sessions ?? [];
  return rows.map((s) => ({ id: s.id, name: s.title }));
}

/** Restore saved session into a runtime thread (Phase 2: seeds server-side history). */
export async function resumeSessionThread(sessionId: string): Promise<{
  thread_id: string;
  session_id: string;
  message_count: number;
  summary: string;
}> {
  return postJson(`/v1/sessions/${sessionId}/resume-thread`, {});
}

export async function getSessionDetail(sessionId: string): Promise<SessionDetail> {
  return fetchJson(`/v1/sessions/${sessionId}`);
}

/**
 * Subscribe to thread event stream (GET SSE). Updates `sinceSeq` from payload `seq` when present.
 */
export async function getThreadEvents(
  threadId: string,
  sinceSeq: number,
  onEvent: (ev: SseTurnEvent & { seq?: number }) => void,
  options?: { signal?: AbortSignal },
): Promise<void> {
  const url = `${runtimeBase}/v1/threads/${encodeURIComponent(threadId)}/events?since_seq=${sinceSeq}`;
  const res = await fetch(url, {
    headers: authHeaders(),
    signal: options?.signal,
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`HTTP ${res.status}: ${text}`);
  }
  const reader = res.body?.getReader();
  if (!reader) {
    throw new Error('No response body');
  }
  const decoder = new TextDecoder();
  let buffer = '';
  while (true) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    buffer += decoder.decode(value, { stream: true });
    const { drained, rest } = drainSseBlocks(buffer);
    buffer = rest;
    for (const ev of drained) {
      let seq: number | undefined;
      try {
        const p = JSON.parse(ev.data);
        if (typeof p.seq === 'number') {
          seq = p.seq;
        }
      } catch {
        /* ignore */
      }
      onEvent({ ...ev, seq });
    }
  }
}
