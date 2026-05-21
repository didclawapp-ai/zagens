import type { McpServersResponse, McpToolsResponse, McpServerConfigPayload } from '../types/mcp';
import type { UsageAggregation, UsageParams } from '../types/usage';
import type {
  TaskSummary,
  TasksResponse,
  AutomationRecord,
  TaskRecord,
  SkillsApiResponse,
  CreateTaskRequest,
  CreateSkillRequest,
  ImportSkillLocalRequest,
  InstallSkillRemoteRequest,
  CreateSkillResponse,
} from '../types/automation';
import type { RoutingRulesResponse, RoutingRule } from '../types/routing';
import { normalizeWorkspaceForApi } from '../lib/defaultWorkspace';
import { coalescePollFetch } from '../lib/pollFetch';
import { listenRuntimeSseEvent } from '../lib/runtimeSseListen';
import { normalizeDesktopStreamEvent } from './streamNormalize';

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
  /** When set, runtime matches `routing_rules.json` intent → model (see RoutingPanel). */
  route_intent?: string;
  /** `auto` | `office` | `code` — resolved when the stream thread is created. */
  task_type?: string;
}

export interface RuntimeThreadSummary {
  id: string;
  task_type?: string;
  mode?: string;
  workspace?: string;
}

export interface SessionInfo {
  id: string;
  name: string;
  created_at?: number;
  updated_at?: number;
  /** From runtime session metadata when available. */
  workspace?: string;
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
/** DS Pick shell: REST/SSE via Tauri; Bearer stays in Rust (H06). */
let useTauriRuntimeProxy = false;

/** Call before render when running inside Tauri; no-op in plain Vite dev. */
export async function initRuntimeConfig(): Promise<void> {
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const port = await invoke<number>('get_runtime_port');
    runtimeBase = `http://127.0.0.1:${port}`;
    useTauriRuntimeProxy = true;
  } catch {
    useTauriRuntimeProxy = false;
  }
}

async function runtimeRequest(path: string, init: RequestInit = {}): Promise<Response> {
  if (!useTauriRuntimeProxy) {
    return fetch(`${runtimeBase}${path}`, {
      ...init,
      headers: {
        'Content-Type': 'application/json',
        ...(init.headers as Record<string, string> | undefined),
      },
    });
  }
  const { invoke } = await import('@tauri-apps/api/core');
  let body: string | null = null;
  if (init.body != null) {
    body =
      typeof init.body === 'string' ? init.body : await new Response(init.body).text();
  }
  const res = await invoke<{ status: number; body: string }>('runtime_http', {
    request: {
      method: (init.method ?? 'GET').toUpperCase(),
      path,
      body,
    },
  });
  return new Response(res.body, {
    status: res.status,
    headers: { 'Content-Type': 'application/json' },
  });
}

export function getRuntimeBase(): string {
  return runtimeBase;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Per-probe ceiling so a wedged socket does not stall the UI for the browser default. */
const RUNTIME_PROBE_FETCH_TIMEOUT_MS = 2_500;
/** Panel polls (context / checklist / scratchpad) — allow sidecar queue time during tool I/O. */
const RUNTIME_POLL_FETCH_TIMEOUT_MS = 45_000;

function runtimeProbeInit(): RequestInit {
  const init: RequestInit = { method: 'GET' };
  if (typeof AbortSignal !== 'undefined' && typeof AbortSignal.timeout === 'function') {
    init.signal = AbortSignal.timeout(RUNTIME_PROBE_FETCH_TIMEOUT_MS);
  }
  return init;
}

/** Single attempt: `/health` then (if authed desktop) `/v1/sessions` — sequential while the port is down avoids doubling refused connections in DevTools. */
async function tryRuntimeFullyReady(): Promise<boolean> {
  try {
    const h = useTauriRuntimeProxy
      ? await runtimeRequest('/health', { method: 'GET', signal: runtimeProbeInit().signal })
      : await fetch(`${runtimeBase}/health`, runtimeProbeInit());
    if (!h.ok) {
      return false;
    }
    if (!useTauriRuntimeProxy) {
      return true;
    }
    const s = await runtimeRequest('/v1/sessions', {
      method: 'GET',
      signal: runtimeProbeInit().signal,
    });
    return s.ok;
  } catch {
    return false;
  }
}

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
  const intervalMs = options?.intervalMs ?? 150;
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await tryRuntimeFullyReady()) {
      return true;
    }
    await sleep(intervalMs);
  }
  return false;
}

/**
 * First boot only: React StrictMode runs mount effects twice in dev — share one wait so we do not
 * double poll the sidecar. Invalidate on explicit reconnect (`retry`) or sidecar restart.
 */
let bootRuntimeReadyPromise: Promise<boolean> | null = null;

export function invalidateRuntimeBootReadyCache(): void {
  bootRuntimeReadyPromise = null;
}

export function waitForRuntimeBootReady(options?: {
  timeoutMs?: number;
  intervalMs?: number;
}): Promise<boolean> {
  bootRuntimeReadyPromise ??= waitForRuntimeReady({
    timeoutMs: options?.timeoutMs ?? 90_000,
    intervalMs: options?.intervalMs ?? 150,
  });
  return bootRuntimeReadyPromise;
}

export type RuntimeConnectionState =
  | 'checking'
  | 'connected'
  | 'offline'
  | 'auth_mismatch';

/**
 * Single probe for UI status indicator.
 * `light: true` — only `/health` (use while a turn is streaming: sidecar may be busy and
 * `/v1/sessions` can exceed the probe timeout without the runtime being dead).
 */
export async function probeRuntimeConnection(options?: {
  light?: boolean;
}): Promise<Exclude<RuntimeConnectionState, 'checking'>> {
  if (!useTauriRuntimeProxy) {
    try {
      const r = await fetch(`${runtimeBase}/health`, runtimeProbeInit());
      return r.ok ? 'connected' : 'offline';
    } catch {
      return 'offline';
    }
  }
  try {
    const probe = runtimeProbeInit();
    const h = await runtimeRequest('/health', { method: 'GET', signal: probe.signal });
    if (!h.ok) {
      return 'offline';
    }
    if (options?.light) {
      return 'connected';
    }
    const ar = await runtimeRequest('/v1/sessions', { method: 'GET', signal: probe.signal });
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
  if (useTauriRuntimeProxy) {
    return postStreamTurnViaTauri(req, onEvent, onDone, onError, options);
  }
  try {
    const response = await fetch(`${runtimeBase}/v1/stream`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
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

async function postStreamTurnViaTauri(
  req: StreamTurnRequest,
  onEvent: (event: SseTurnEvent) => void,
  onDone: () => void,
  onError: (err: Error) => void,
  options?: { signal?: AbortSignal },
): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');

  let buffer = '';
  const unsubs: Array<() => void> = [];
  const abort = options?.signal;

  const cleanup = () => {
    for (const u of unsubs) {
      u();
    }
  };

  if (abort?.aborted) {
    onDone();
    return;
  }

  const onAbort = () => {
    cleanup();
    onDone();
  };
  abort?.addEventListener('abort', onAbort, { once: true });

  try {
    unsubs.push(
      await listenRuntimeSseEvent<string>('runtime://stream-chunk', (payload) => {
        buffer += payload;
        const { drained, rest } = drainSseBlocks(buffer);
        buffer = rest;
        for (const block of drained) {
          onEvent(block);
        }
      }),
    );
    unsubs.push(
      await listenRuntimeSseEvent<unknown>('runtime://stream-done', () => {
        cleanup();
        abort?.removeEventListener('abort', onAbort);
        const { drained: tail } = drainSseBlocks(buffer + '\n\n');
        for (const block of tail) {
          onEvent(block);
        }
        onDone();
      }),
    );
    unsubs.push(
      await listenRuntimeSseEvent<string>('runtime://stream-error', (payload) => {
        cleanup();
        abort?.removeEventListener('abort', onAbort);
        onError(new Error(payload));
      }),
    );

    await invoke('runtime_post_stream', { body: JSON.stringify(req) });
  } catch (err) {
    cleanup();
    abort?.removeEventListener('abort', onAbort);
    if (isAbortError(err)) {
      onDone();
      return;
    }
    onError(err instanceof Error ? err : new Error(String(err)));
  }
}

export async function fetchJson<T>(path: string): Promise<T> {
  const res = await fetchResponseWithBackoff(
    () => runtimeRequest(path, { method: 'GET' }),
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

function runtimePollInit(): RequestInit {
  const init: RequestInit = { method: 'GET' };
  if (typeof AbortSignal !== 'undefined' && typeof AbortSignal.timeout === 'function') {
    init.signal = AbortSignal.timeout(RUNTIME_POLL_FETCH_TIMEOUT_MS);
  }
  return init;
}

/** GET used by poll timers — skips a new fetch while the same path is still in flight. */
function fetchJsonPoll<T>(path: string): Promise<T> {
  return coalescePollFetch(`GET:${path}`, async () => {
    const res = await fetchResponseWithBackoff(
      () => runtimeRequest(path, { method: 'GET', ...runtimePollInit() }),
      `GET ${path}`,
    );
    if (!res.ok) {
      const text = await res.text();
      const err = new Error(`HTTP ${res.status}: ${text}`);
      (err as Error & { status?: number }).status = res.status;
      throw err;
    }
    return res.json() as Promise<T>;
  });
}

export async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetchResponseWithBackoff(
    () =>
      runtimeRequest(path, {
        method: 'POST',
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
      runtimeRequest(path, {
        method: 'PATCH',
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

export async function putJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetchResponseWithBackoff(
    () =>
      runtimeRequest(path, {
        method: 'PUT',
        body: JSON.stringify(body ?? {}),
      }),
    `PUT ${path}`,
  );
  if (!res.ok) {
    const text = await res.text();
    const err = new Error(`HTTP ${res.status}: ${text}`);
    (err as Error & { status?: number }).status = res.status;
    throw err;
  }
  return res.json();
}

export async function deleteJson(path: string): Promise<void> {
  const res = await fetchResponseWithBackoff(
    () => runtimeRequest(path, { method: 'DELETE' }),
    `DELETE ${path}`,
  );
  if (!res.ok) {
    const text = await res.text();
    const err = new Error(`HTTP ${res.status}: ${text}`);
    (err as Error & { status?: number }).status = res.status;
    throw err;
  }
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
    route_intent?: string;
    task_type?: string;
  },
): Promise<{ thread: unknown; turn: TurnRecord }> {
  return postJson(`/v1/threads/${encodeURIComponent(threadId)}/turns`, body);
}

/** Stop an in-flight turn (`engine.cancel()` on the runtime). */
export async function interruptThreadTurn(
  threadId: string,
  turnId: string,
): Promise<TurnRecord> {
  return postJson<TurnRecord>(
    `/v1/threads/${encodeURIComponent(threadId)}/turns/${encodeURIComponent(turnId)}/interrupt`,
    {},
  );
}

/** Minimal thread fields used by desktop UI; backend returns full `ThreadRecord`. */
export interface RuntimeThreadRecord {
  id: string;
  workspace: string;
  model?: string;
  trust_mode?: boolean;
  task_type?: string;
  scratchpad_run_id?: string | null;
}

/** One inventory row from `GET /v1/threads/{id}/scratchpad/status` (Phase D1). */
export interface ScratchpadInventoryArea {
  id: string;
  path: string;
  status: 'pending' | 'in_progress' | 'done' | 'deferred' | string;
  notes_count?: number;
}

/** `GET /v1/threads/{id}/scratchpad/status` (audit progress, read-only). */
export interface ScratchpadStatus {
  run_id?: string;
  path?: string;
  areas_total?: number;
  areas_done?: number;
  areas_deferred?: number;
  areas_in_progress?: number;
  areas_pending?: number;
  resume_area_id?: string | null;
  notes_total?: number;
  findings_verified?: number;
  findings_open?: number;
  findings_verified_high?: number;
  findings_open_high?: number;
  findings_open_medium?: number;
  findings_open_low?: number;
  notes_per_area?: Record<string, number>;
  areas?: ScratchpadInventoryArea[];
  checklist_completed?: number;
  checklist_total?: number;
  contract_warnings?: string[];
  subagents_running?: number;
}

export async function fetchThreadScratchpadStatus(
  threadId: string,
): Promise<ScratchpadStatus | null> {
  const raw = await fetchJsonPoll<ScratchpadStatus | null>(
    `/v1/threads/${encodeURIComponent(threadId)}/scratchpad/status`,
  );
  if (raw == null || typeof raw !== 'object' || !('run_id' in raw)) {
    return null;
  }
  return raw;
}

/** Turn row included in full GET /v1/threads/{id} (ThreadDetail). */
export interface ThreadTurnRecord {
  id: string;
  usage?: {
    input_tokens?: number;
    output_tokens?: number;
    reasoning_tokens?: number;
  } | null;
}

export interface ThreadDetailResponse {
  thread: RuntimeThreadRecord;
  latest_seq: number;
  /** Present on wire; `usage.output_tokens` can restore last-turn hint; context % uses transcript estimate. */
  turns?: ThreadTurnRecord[];
}

export async function getThreadDetail(threadId: string): Promise<ThreadDetailResponse> {
  return fetchJson(`/v1/threads/${encodeURIComponent(threadId)}`);
}

export async function getThreadContext(threadId: string): Promise<ThreadContextSnapshot> {
  return fetchJsonPoll(`/v1/threads/${encodeURIComponent(threadId)}/context`);
}

export async function fetchThreadChecklist(threadId: string): Promise<any> {
  return fetchJsonPoll(`/v1/threads/${encodeURIComponent(threadId)}/checklist`);
}

/** Side-git snapshots for a runtime thread (`GET /v1/threads/{id}/snapshots`). */
export interface ThreadSnapshotEntry {
  n: number;
  id: string;
  label: string;
  timestamp: number;
}

export interface SnapshotsListResponse {
  workspace: string;
  snapshots: ThreadSnapshotEntry[];
}

export async function getThreadSnapshots(
  threadId: string,
  options?: { limit?: number },
): Promise<SnapshotsListResponse> {
  const lim = options?.limit;
  const q = lim != null ? `?limit=${encodeURIComponent(String(lim))}` : '';
  return fetchJson(`/v1/threads/${encodeURIComponent(threadId)}/snapshots${q}`);
}

export async function restoreThreadSnapshot(
  threadId: string,
  n: number,
): Promise<{ restored: boolean; label: string; id: string }> {
  return postJson(`/v1/threads/${encodeURIComponent(threadId)}/snapshots/restore`, { n });
}

export interface BrowseWorkspaceEntry {
  name: string;
  kind: string;
  size?: number;
}

export interface BrowseWorkspaceListResponse {
  workspace: string;
  path: string;
  entries: BrowseWorkspaceEntry[];
}

export async function browseThreadWorkspace(
  threadId: string,
  relativePath?: string,
): Promise<BrowseWorkspaceListResponse> {
  const trimmed = relativePath?.trim() ?? '';
  const q = trimmed.length > 0 ? `?path=${encodeURIComponent(trimmed)}` : '';
  return fetchJson(`/v1/threads/${encodeURIComponent(threadId)}/workspace/browse${q}`);
}

/** List directory under Composer workspace path (no runtime thread yet). */
export async function browseComposerWorkspace(
  workspaceRoot: string,
  relativePath?: string,
): Promise<BrowseWorkspaceListResponse> {
  const root = normalizeWorkspaceForApi(workspaceRoot);
  if (!root) {
    throw new Error('workspace root required');
  }
  const rel = relativePath?.trim() ?? '';
  const pathQ = rel.length > 0 ? `&path=${encodeURIComponent(rel)}` : '';
  return fetchJson(`/v1/workspace/browse?workspace=${encodeURIComponent(root)}${pathQ}`);
}

export interface WorkspaceFileResponse {
  path: string;
  content: string;
  truncated: boolean;
  language_hint?: string | null;
}

export async function readThreadWorkspaceFile(
  threadId: string,
  relativePath: string,
): Promise<WorkspaceFileResponse> {
  return fetchJson(
    `/v1/threads/${encodeURIComponent(threadId)}/workspace/file?path=${encodeURIComponent(relativePath)}`,
  );
}

export async function readComposerWorkspaceFile(
  workspaceRoot: string,
  relativePath: string,
): Promise<WorkspaceFileResponse> {
  const root = normalizeWorkspaceForApi(workspaceRoot);
  if (!root) {
    throw new Error('workspace root required');
  }
  return fetchJson(
    `/v1/workspace/file?workspace=${encodeURIComponent(root)}&path=${encodeURIComponent(relativePath)}`,
  );
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

// ========== Usage ==========

export async function fetchUsage(params?: UsageParams): Promise<UsageAggregation> {
  const qs = new URLSearchParams();
  if (params?.since) qs.set('since', params.since);
  if (params?.until) qs.set('until', params.until);
  if (params?.group_by) qs.set('group_by', params.group_by);
  const suffix = qs.toString();
  return fetchJson<UsageAggregation>(`/v1/usage${suffix ? `?${suffix}` : ''}`);
}

// ========== Tasks / Automations / Skills ==========

export async function fetchTasks(): Promise<TaskSummary[]> {
  const res = await fetchJson<TasksResponse>('/v1/tasks');
  return res.tasks;
}

export async function createTask(body: CreateTaskRequest): Promise<TaskRecord> {
  return postJson<TaskRecord>('/v1/tasks', body);
}

export async function cancelTask(taskId: string): Promise<TaskRecord> {
  return postJson<TaskRecord>(`/v1/tasks/${encodeURIComponent(taskId)}/cancel`, {});
}

export async function clearFinishedTasks(): Promise<{ removed: number }> {
  return postJson<{ removed: number }>('/v1/tasks/clear', {});
}

export async function fetchAutomations(): Promise<AutomationRecord[]> {
  return fetchJson<AutomationRecord[]>('/v1/automations');
}

export async function fetchSkills(): Promise<SkillsApiResponse> {
  return fetchJson<SkillsApiResponse>('/v1/skills');
}

export async function createSkill(body: CreateSkillRequest): Promise<CreateSkillResponse> {
  return postJson<CreateSkillResponse>('/v1/skills', body);
}

export async function importSkillLocal(
  body: ImportSkillLocalRequest,
): Promise<CreateSkillResponse> {
  return postJson<CreateSkillResponse>('/v1/skills/import', body);
}

export async function installSkillRemote(
  body: InstallSkillRemoteRequest,
): Promise<CreateSkillResponse> {
  return postJson<CreateSkillResponse>('/v1/skills/install', body);
}

// ========== Routing ==========

export async function fetchRoutingRules(): Promise<RoutingRulesResponse> {
  return fetchJson<RoutingRulesResponse>('/v1/apps/routing/rules');
}

export async function setRoutingRules(rules: RoutingRule[]): Promise<RoutingRulesResponse> {
  return putJson<RoutingRulesResponse>('/v1/apps/routing/rules', { rules });
}

// ========== MCP ==========

export async function fetchMcpServers(): Promise<McpServersResponse> {
  return fetchJson<McpServersResponse>('/v1/apps/mcp/servers');
}

export async function getMcpServer(name: string): Promise<McpServerConfigPayload> {
  return fetchJson<McpServerConfigPayload>(`/v1/apps/mcp/servers/${encodeURIComponent(name)}`);
}

export async function putMcpServer(name: string, body: McpServerConfigPayload): Promise<{ ok: boolean }> {
  return putJson<{ ok: boolean }>(`/v1/apps/mcp/servers/${encodeURIComponent(name)}`, body);
}

export async function deleteMcpServer(name: string): Promise<void> {
  await deleteJson(`/v1/apps/mcp/servers/${encodeURIComponent(name)}`);
}

export async function fetchMcpTools(server?: string): Promise<McpToolsResponse> {
  const qs = server ? `?server=${encodeURIComponent(server)}` : '';
  return fetchJson<McpToolsResponse>(`/v1/apps/mcp/tools${qs}`);
}

export interface AddMcpServerRequest {
  name: string;
  command?: string;
  url?: string;
  args?: string[];
}

export async function addMcpServer(req: AddMcpServerRequest): Promise<void> {
  const res = await fetchResponseWithBackoff(
    () =>
      runtimeRequest('/v1/apps/mcp/servers', {
        method: 'POST',
        body: JSON.stringify(req),
      }),
    'POST /v1/apps/mcp/servers',
  );
  if (!res.ok) {
    const text = await res.text();
    const err = new Error(`HTTP ${res.status}: ${text}`);
    (err as Error & { status?: number }).status = res.status;
    throw err;
  }
}

/** Merge MCP servers (and optional timeouts) from a JSON fragment into ~/.deepseek/mcp.json. */
export async function mergeMcpConfigJson(fragmentText: string): Promise<{ merged_servers: number }> {
  let parsed: unknown;
  try {
    parsed = JSON.parse(fragmentText.trim());
  } catch {
    const err = new Error('JSON 语法无效，请检查括号与引号') as Error & { status?: number };
    throw err;
  }
  return postJson<{ merged_servers: number }>('/v1/apps/mcp/config/merge', parsed);
}

// ========== Sessions ==========

export async function deleteSession(sessionId: string): Promise<void> {
  const res = await fetchResponseWithBackoff(
    () =>
      runtimeRequest(`/v1/sessions/${encodeURIComponent(sessionId)}`, {
        method: 'DELETE',
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
  const data = await fetchJson<{
    sessions: Array<{ id: string; title: string; workspace?: string }>;
  }>('/v1/sessions');
  const rows = data.sessions ?? [];
  return rows.map((s) => ({
    id: s.id,
    name: s.title,
    workspace: s.workspace?.trim() || undefined,
  }));
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

const THREAD_TURN_POLL_MS = 120;

function sleepMs(ms: number, signal?: AbortSignal): Promise<void> {
  if (signal?.aborted) {
    return Promise.resolve();
  }
  return new Promise((resolve) => {
    const id = window.setTimeout(resolve, ms);
    signal?.addEventListener(
      'abort',
      () => {
        window.clearTimeout(id);
        resolve();
      },
      { once: true },
    );
  });
}

/**
 * Poll `replay_only` thread events until the turn completes. Avoids open-ended
 * `GET …/events` SSE (multiple Tauri `runtime_get_sse` invokes can duplicate chunks).
 */
export async function pollThreadTurnEvents(
  threadId: string,
  sinceSeq: number,
  onEvent: (ev: SseTurnEvent & { seq?: number }) => void,
  options?: { signal?: AbortSignal; turnId?: string },
): Promise<void> {
  let cursor = sinceSeq;
  const filter = options?.turnId ? { turnId: options.turnId } : undefined;
  while (!options?.signal?.aborted) {
    let maxSeq = cursor;
    let turnDone = false;
    await replayThreadEvents(
      threadId,
      cursor,
      (ev) => {
        if (ev.seq != null && ev.seq > maxSeq) {
          maxSeq = ev.seq;
        }
        const norm = normalizeDesktopStreamEvent(ev, filter);
        if (norm?.kind === 'turn_completed' || norm?.kind === 'done') {
          turnDone = true;
        }
        onEvent(ev);
      },
      { signal: options?.signal },
    );
    cursor = maxSeq;
    if (turnDone || options?.signal?.aborted) {
      return;
    }
    await sleepMs(THREAD_TURN_POLL_MS, options?.signal);
  }
}

/**
 * Replay persisted thread events only (closes after backlog). Use for session restore —
 * do not use open-ended `getThreadEvents` for live turns (use `pollThreadTurnEvents`).
 */
export async function replayThreadEvents(
  threadId: string,
  sinceSeq: number,
  onEvent: (ev: SseTurnEvent & { seq?: number }) => void,
  options?: { signal?: AbortSignal },
): Promise<void> {
  const controller = new AbortController();
  if (options?.signal) {
    if (options.signal.aborted) {
      controller.abort();
    } else {
      options.signal.addEventListener('abort', () => controller.abort(), { once: true });
    }
  }
  let sawEvent = false;
  let lastEventMs = 0;
  const idleGuard = window.setInterval(() => {
    if (sawEvent && Date.now() - lastEventMs > 750) {
      controller.abort();
    }
  }, 200);
  const maxGuard = window.setTimeout(() => controller.abort(), 120_000);
  const path = `/v1/threads/${encodeURIComponent(threadId)}/events?since_seq=${sinceSeq}&replay_only=true`;
  try {
    if (useTauriRuntimeProxy) {
      await consumeThreadEventsSse(path, onEvent, {
        signal: controller.signal,
        onChunk: () => {
          sawEvent = true;
          lastEventMs = Date.now();
        },
      });
    } else {
      const res = await fetch(`${runtimeBase}${path}`, {
        headers: { 'Content-Type': 'application/json' },
        signal: controller.signal,
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
          sawEvent = true;
          lastEventMs = Date.now();
          onEvent({ ...ev, seq });
        }
      }
    }
  } catch (e) {
    if (sawEvent && (e as Error).name === 'AbortError') {
      return;
    }
    throw e;
  } finally {
    window.clearInterval(idleGuard);
    window.clearTimeout(maxGuard);
  }
}

/**
 * Subscribe to thread event stream (GET SSE). Updates `sinceSeq` from payload `seq` when present.
 * Stays open for live events until the connection closes or `signal` aborts.
 */
async function consumeThreadEventsSse(
  path: string,
  onEvent: (ev: SseTurnEvent & { seq?: number }) => void,
  options?: { signal?: AbortSignal; onChunk?: () => void },
): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  const abort = options?.signal;
  let buffer = '';
  const unsubs: Array<() => void> = [];
  let settled = false;

  const flushTail = () => {
    const { drained: tail } = drainSseBlocks(buffer + '\n\n');
    for (const block of tail) {
      let seq: number | undefined;
      try {
        const p = JSON.parse(block.data);
        if (typeof p.seq === 'number') {
          seq = p.seq;
        }
      } catch {
        /* ignore */
      }
      onEvent({ ...block, seq });
    }
  };

  if (abort?.aborted) {
    return;
  }

  await new Promise<void>((resolve, reject) => {
    const finish = () => {
      if (settled) return;
      settled = true;
      for (const u of unsubs) {
        u();
      }
      abort?.removeEventListener('abort', onAbort);
    };

    const onAbort = () => {
      void invoke('runtime_cancel_sse').catch(() => {
        /* sidecar may already be done */
      });
      finish();
      resolve();
    };
    abort?.addEventListener('abort', onAbort, { once: true });

    void (async () => {
      try {
        unsubs.push(
          await listenRuntimeSseEvent<string>('runtime://events-chunk', (payload) => {
            if (abort?.aborted) return;
            buffer += payload;
            const { drained, rest } = drainSseBlocks(buffer);
            buffer = rest;
            for (const block of drained) {
              let seq: number | undefined;
              try {
                const p = JSON.parse(block.data);
                if (typeof p.seq === 'number') {
                  seq = p.seq;
                }
              } catch {
                /* ignore */
              }
              options?.onChunk?.();
              onEvent({ ...block, seq });
            }
          }),
        );
        unsubs.push(
          await listenRuntimeSseEvent<unknown>('runtime://events-done', () => {
            flushTail();
            finish();
            resolve();
          }),
        );
        unsubs.push(
          await listenRuntimeSseEvent<string>('runtime://events-error', (payload) => {
            finish();
            reject(new Error(payload));
          }),
        );
        await invoke('runtime_get_sse', { path });
      } catch (err) {
        finish();
        reject(err instanceof Error ? err : new Error(String(err)));
      }
    })();
  });
}

export async function getThreadEvents(
  threadId: string,
  sinceSeq: number,
  onEvent: (ev: SseTurnEvent & { seq?: number }) => void,
  options?: { signal?: AbortSignal },
): Promise<void> {
  const path = `/v1/threads/${encodeURIComponent(threadId)}/events?since_seq=${sinceSeq}`;
  if (useTauriRuntimeProxy) {
    return consumeThreadEventsSse(path, onEvent, { signal: options?.signal });
  }
  const res = await fetch(`${runtimeBase}${path}`, {
    headers: { 'Content-Type': 'application/json' },
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

// ========== System Settings (Desktop Tauri) ==========

export interface SystemSettings {
  default_model: string;
  reasoning_effort: string;
  cost_currency: string;
  allow_shell: boolean;
  approval_policy: string;
  sandbox_mode: string;
  max_subagents: number;
  /** Per-step sub-agent LLM API timeout (seconds), `[subagents] step_timeout_secs`. */
  subagent_step_timeout_secs: number;
  web_search: boolean;
  subagents_enabled: boolean;
  exec_policy: boolean;
  memory_enabled: boolean;
  lsp_enabled: boolean;
  snapshots_enabled: boolean;
  notify_method: string;
  session_file_mb: number;
  auto_compact: boolean;
  compaction_threshold_tokens: number;
  compaction_threshold_default: number;
}

export type { ThreadContextSnapshot } from '../lib/contextUsage';
import type { ThreadContextSnapshot } from '../lib/contextUsage';

export async function fetchSystemSettings(): Promise<SystemSettings> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<SystemSettings>('get_system_settings');
}

export async function saveSystemSettings(settings: SystemSettings): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('save_system_settings', { settings });
}

// ========== Symbol Index Management ==========

export interface SymbolIndexInfo {
  status: string;
  path: string;
  dir: string;
  size_bytes: number;
  schema_version: number;
  file_count: number;
  symbol_count: number;
}

export async function fetchSymbolIndexInfo(workspace: string): Promise<SymbolIndexInfo> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<SymbolIndexInfo>('get_symbol_index_info', { workspace });
}

export async function deleteSymbolIndex(workspace: string): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  await invoke('delete_symbol_index', { workspace });
}