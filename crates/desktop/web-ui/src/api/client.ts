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
  trust_mode?: boolean;
  allow_shell?: boolean;
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
    // Token stays module-private (closure scope); never on window.
  } catch {
    /* Not in Tauri webview — open API (no auth). */
  }
}

export function getRuntimeBase(): string {
  return runtimeBase;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function isAbortError(err: unknown): boolean {
  if (err instanceof DOMException) {
    return err.name === 'AbortError' || err.code === DOMException.ABORT_ERR;
  }
  return err instanceof Error && err.name === 'AbortError';
}

const RUNTIME_FETCH_ATTEMPTS = 5;
const RUNTIME_FETCH_BASE_DELAY_MS = 350;

/** True when `fetch` failed before an HTTP response (sidecar still starting, RST, etc.). */
export function isTransientRuntimeFetchError(err: unknown): boolean {
  if (isAbortError(err)) {
    return false;
  }
  if (err instanceof TypeError) {
    return true;
  }
  const msg = (err instanceof Error ? err.message : String(err)).toLowerCase();
  return (
    msg.includes('failed to fetch') ||
    msg.includes('networkerror') ||
    msg.includes('load failed') ||
    msg.includes('network request failed')
  );
}

function enrichRuntimeNetworkError(original: unknown): Error {
  const base = original instanceof Error ? original : new Error(String(original));
  if (base.message.includes('若刚重启应用')) {
    return base;
  }
  return new Error(
    `${base.message} （${runtimeBase}）若刚重启应用，本地 sidecar 可能仍在启动：请稍后点击横幅中的「重试连接」。`,
  );
}

async function fetchResponseWithBackoff(
  run: () => Promise<Response>,
  context: string,
): Promise<Response> {
  let last: unknown;
  for (let attempt = 0; attempt < RUNTIME_FETCH_ATTEMPTS; attempt++) {
    try {
      return await run();
    } catch (e) {
      last = e;
      if (!isTransientRuntimeFetchError(e) || attempt === RUNTIME_FETCH_ATTEMPTS - 1) {
        break;
      }
      await sleep(RUNTIME_FETCH_BASE_DELAY_MS * 2 ** attempt);
    }
  }
  console.warn(`[ds-pick] ${context}: fetch failed after ${RUNTIME_FETCH_ATTEMPTS} attempts`, last);
  throw enrichRuntimeNetworkError(last);
}

/** Poll until `/health` and (when `runtimeToken` is set) `/v1/sessions` with Bearer succeed. */
export async function waitForRuntimeReady(options?: {
  timeoutMs?: number;
  intervalMs?: number;
}): Promise<boolean> {
  const timeoutMs = options?.timeoutMs ?? 90_000;
  const intervalMs = options?.intervalMs ?? 400;
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const r = await fetch(`${runtimeBase}/health`, { method: 'GET' });
      if (r.ok) {
        if (!runtimeToken.trim()) {
          return true;
        }
        const ar = await fetch(`${runtimeBase}/v1/sessions`, { headers: authHeaders() });
        if (ar.ok) {
          return true;
        }
        /* health up but API not ready yet or stale sidecar being replaced */
      }
    } catch {
      /* ECONNREFUSED until sidecar binds */
    }
    await sleep(intervalMs);
  }
  return false;
}

export type RuntimeConnectionState =
  | 'checking'
  | 'connected'
  | 'offline'
  | 'auth_mismatch';

/** Single probe for UI status indicator (lighter than full session list). */
export async function probeRuntimeConnection(): Promise<Exclude<RuntimeConnectionState, 'checking'>> {
  try {
    const r = await fetch(`${runtimeBase}/health`, { method: 'GET' });
    if (!r.ok) {
      return 'offline';
    }
  } catch {
    return 'offline';
  }
  if (!runtimeToken.trim()) {
    return 'connected';
  }
  try {
    const ar = await fetch(`${runtimeBase}/v1/sessions`, { headers: authHeaders() });
    if (ar.status === 401) {
      return 'auth_mismatch';
    }
    if (ar.ok) {
      return 'connected';
    }
    return 'offline';
  } catch {
    return 'offline';
  }
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
  options?: { signal?: AbortSignal },
): Promise<void> {
  try {
    const response = await fetch(`${runtimeBase}/v1/stream`, {
      method: 'POST',
      headers: authHeaders(),
      body: JSON.stringify(req),
      signal: options?.signal,
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
    if (isAbortError(err)) {
      onDone();
      return;
    }
    onError(err instanceof Error ? err : new Error(String(err)));
  }
}

export async function fetchJson<T>(path: string): Promise<T> {
  const res = await fetchResponseWithBackoff(
    () =>
      fetch(`${runtimeBase}${path}`, {
        headers: authHeaders(),
      }),
    `GET ${path}`,
  );
  if (!res.ok) {
    const text = await res.text();
    const err = new Error(`HTTP ${res.status}: ${text}`);
    (err as Error & { status?: number }).status = res.status;
    throw err;
  }
  return res.json();
}

export async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetchResponseWithBackoff(
    () =>
      fetch(`${runtimeBase}${path}`, {
        method: 'POST',
        headers: authHeaders(),
        body: JSON.stringify(body ?? {}),
      }),
    `POST ${path}`,
  );
  if (!res.ok) {
    const text = await res.text();
    const err = new Error(`HTTP ${res.status}: ${text}`);
    (err as Error & { status?: number }).status = res.status;
    throw err;
  }
  return res.json();
}

export async function patchJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetchResponseWithBackoff(
    () =>
      fetch(`${runtimeBase}${path}`, {
        method: 'PATCH',
        headers: authHeaders(),
        body: JSON.stringify(body ?? {}),
      }),
    `PATCH ${path}`,
  );
  if (!res.ok) {
    const text = await res.text();
    const err = new Error(`HTTP ${res.status}: ${text}`);
    (err as Error & { status?: number }).status = res.status;
    throw err;
  }
  return res.json();
}

export async function postResolveApproval(
  threadId: string,
  turnId: string,
  toolCallId: string,
  decision: 'approve' | 'deny',
): Promise<unknown> {
  return postJson(
    `/v1/threads/${encodeURIComponent(threadId)}/turns/${encodeURIComponent(turnId)}/resolve-approval`,
    { tool_call_id: toolCallId, decision },
  );
}

export interface TurnRecord {
  id: string;
  thread_id: string;
  status: string;
}

export async function startThreadTurn(
  threadId: string,
  body: {
    prompt: string;
    model?: string;
    mode?: string;
    allow_shell?: boolean;
    trust_mode?: boolean;
    auto_approve?: boolean;
  },
): Promise<{ thread: unknown; turn: TurnRecord }> {
  return postJson(`/v1/threads/${encodeURIComponent(threadId)}/turns`, body);
}

/** Minimal thread fields used by desktop UI; backend returns full `ThreadRecord`. */
export interface RuntimeThreadRecord {
  id: string;
  workspace: string;
}

export interface ThreadDetailResponse {
  thread: RuntimeThreadRecord;
  latest_seq: number;
}

export async function getThreadDetail(threadId: string): Promise<ThreadDetailResponse> {
  return fetchJson(`/v1/threads/${encodeURIComponent(threadId)}`);
}

export type PatchThreadBody = Partial<{
  archived: boolean;
  allow_shell: boolean;
  trust_mode: boolean;
  auto_approve: boolean;
  model: string;
  mode: string;
  title: string;
  system_prompt: string;
  workspace: string;
}>;

export async function patchThread(threadId: string, body: PatchThreadBody): Promise<RuntimeThreadRecord> {
  return patchJson(`/v1/threads/${encodeURIComponent(threadId)}`, body);
}

export async function persistThreadSession(
  threadId: string,
  sessionId?: string | null,
): Promise<{ session_id: string; message_count: number }> {
  return postJson(`/v1/threads/${encodeURIComponent(threadId)}/persist-session`, {
    session_id: sessionId?.trim() || undefined,
  });
}

export async function deleteSession(sessionId: string): Promise<void> {
  const res = await fetchResponseWithBackoff(
    () =>
      fetch(`${runtimeBase}/v1/sessions/${encodeURIComponent(sessionId)}`, {
        method: 'DELETE',
        headers: authHeaders(),
      }),
    `DELETE /v1/sessions/${sessionId}`,
  );
  if (res.ok || res.status === 204) {
    return;
  }
  const text = await res.text();
  const err = new Error(`HTTP ${res.status}: ${text}`) as Error & { status?: number };
  err.status = res.status;
  throw err;
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
