import { useCallback, useEffect, useRef, useState } from 'react';
import {
  postStreamTurn,
  getSessions,
  getSessionDetail,
  resumeSessionThread,
  getThreadDetail,
  patchThread,
  startThreadTurn,
  getThreadEvents,
  postResolveApproval,
  deleteSession,
  persistThreadSession,
  waitForRuntimeReady,
  probeRuntimeConnection,
  initRuntimeConfig,
  getRuntimeBase,
  type RuntimeConnectionState,
  type SessionInfo,
  type SseTurnEvent,
} from './api/client';
import { normalizeDesktopStreamEvent, type NormalizedStreamEvent } from './api/streamNormalize';
import ChatView from './components/ChatView';
import Composer, { type ComposerOutboundMessage } from './components/Composer';
import Sidebar from './components/Sidebar';
import ApprovalDialog from './components/ApprovalDialog';
import RightPanel, { type RightPanelView } from './components/RightPanel';
import { loadWorkspaceFileIntoPreview } from './lib/openWorkspaceFile';
import type { PreviewState } from './components/preview/types';
import { streamFlagsForRunMode } from './lib/runtimeMode';
import {
  type DesktopModelId,
  type DesktopRunModeId,
  parseDesktopModelId,
  parseDesktopRunModeId,
} from './types/desktop';

/**
 * When `/health` + `/v1/sessions` probe is `connected`, these banners are usually stale:
 * the failure was a transient fetch or a race before the sidecar finished starting.
 * (We do not clear generic HTTP 4xx/5xx bodies — those may still be actionable.)
 */
function shouldClearBannerWhenRuntimeConnected(banner: string): boolean {
  if (/token|未授权|401|Bearer|运行时 token/i.test(banner)) {
    return true;
  }
  if (/无法连接本地运行时|仍未连接本地运行时/.test(banner)) {
    return true;
  }
  const transport = /failed to fetch|load failed|networkerror|network request failed|econnrefused|若刚重启应用|127\.0\.0\.1:\d+/i.test(
    banner,
  );
  if (transport) {
    return true;
  }
  return false;
}

interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking?: string;
  tools?: ToolCall[];
  isStreaming?: boolean;
}

interface ToolCall {
  id: string;
  name: string;
  input: string;
  output?: string;
  status: 'running' | 'done' | 'error';
}

interface ApprovalState {
  toolCallId: string;
  toolName: string;
  description: string;
}

let msgId = 0;
function nextId() {
  return `msg-${++msgId}`;
}

function stringifyInput(input: unknown): string {
  if (input == null || input === '') {
    return '';
  }
  if (typeof input === 'string') {
    return input;
  }
  try {
    return JSON.stringify(input, null, 2);
  } catch {
    return String(input);
  }
}

function toolOutputString(output: unknown): string {
  if (output == null) {
    return '';
  }
  if (typeof output === 'string') {
    return output;
  }
  try {
    return JSON.stringify(output, null, 2);
  } catch {
    return String(output);
  }
}

type Theme = 'light' | 'dark';

function loadRunModePreference(): DesktopRunModeId {
  try {
    return parseDesktopRunModeId(localStorage.getItem('deepseek-desktop-run-mode')) ?? 'agent';
  } catch {
    return 'agent';
  }
}

function loadComposerPrefs(): {
  model: DesktopModelId;
  workspace: string;
} {
  try {
    const wm = parseDesktopModelId(localStorage.getItem('deepseek-desktop-model'));
    const ws = localStorage.getItem('deepseek-desktop-workspace');
    return {
      model: wm ?? 'deepseek-v4-pro',
      workspace: ws != null && ws.trim().length > 0 ? ws.trim() : '.',
    };
  } catch {
    return { model: 'deepseek-v4-pro', workspace: '.' };
  }
}

function loadTheme(): Theme {
  try {
    const stored = localStorage.getItem('deepseek-theme');
    if (stored === 'dark' || stored === 'light') return stored;
  } catch {
    /* ignore */
  }
  return 'light';
}

const ACTIVE_SESSION_STORAGE_KEY = 'deepseek-desktop-active-session-id';
const ACTIVE_INSPECTOR_STORAGE_KEY = 'deepseek-desktop-active-inspector';

function loadStoredActiveSessionId(): string | null {
  try {
    const s = localStorage.getItem(ACTIVE_SESSION_STORAGE_KEY)?.trim();
    return s && s.length > 0 ? s : null;
  } catch {
    return null;
  }
}

function loadStoredInspector(): RightPanelView {
  try {
    const s = localStorage.getItem(ACTIVE_INSPECTOR_STORAGE_KEY);
    if (s === 'workspace' || s === 'api-key' || s === 'settings') {
      return s;
    }
  } catch {
    /* ignore */
  }
  return 'workspace';
}

function applyTheme(theme: Theme) {
  const root = document.documentElement;
  if (theme === 'dark') {
    root.classList.add('dark');
  } else {
    root.classList.remove('dark');
  }
}

export default function App() {
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const [selectedModel, setSelectedModel] = useState<DesktopModelId>(() => loadComposerPrefs().model);
  const [selectedWorkspace, setSelectedWorkspace] = useState(() => loadComposerPrefs().workspace);
  const [messages, setMessages] = useState<Message[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [activeInspector, setActiveInspector] = useState<RightPanelView>(() => loadStoredInspector());
  const [banner, setBanner] = useState<string | null>(null);
  const [resumedThreadId, setResumedThreadId] = useState<string | null>(null);
  const [threadTrustMode, setThreadTrustMode] = useState(false);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [autoApprove, setAutoApprove] = useState(true);
  const [runMode, setRunMode] = useState<DesktopRunModeId>(() => loadRunModePreference());
  const [approval, setApproval] = useState<ApprovalState | null>(null);
  const [approvalBusy, setApprovalBusy] = useState(false);
  const [panelPreview, setPanelPreview] = useState<PreviewState | null>(null);
  const [focusWorkspaceFilesNonce, setFocusWorkspaceFilesNonce] = useState(0);
  const [desktopHost, setDesktopHost] = useState(false);
  const [desktopApiKeyConfigured, setDesktopApiKeyConfigured] = useState<boolean | null>(null);
  const [runtimeConn, setRuntimeConn] = useState<RuntimeConnectionState>('checking');

  const eventAbortRef = useRef<AbortController | null>(null);
  const threadTurnRef = useRef<{ threadId: string; turnId: string }>({
    threadId: '',
    turnId: '',
  });
  const activeSessionIdRef = useRef<string | null>(null);
  const lastPersistedTurnRef = useRef<string>('');
  const selectSessionGenerationRef = useRef(0);
  const startupSessionRestoredRef = useRef(false);

  useEffect(() => {
    activeSessionIdRef.current = activeSessionId;
  }, [activeSessionId]);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  useEffect(() => {
    try {
      localStorage.setItem('deepseek-desktop-model', selectedModel);
    } catch {
      /* ignore */
    }
  }, [selectedModel]);

  useEffect(() => {
    try {
      localStorage.setItem('deepseek-desktop-workspace', selectedWorkspace);
    } catch {
      /* ignore */
    }
  }, [selectedWorkspace]);

  useEffect(() => {
    try {
      localStorage.setItem('deepseek-desktop-run-mode', runMode);
    } catch {
      /* ignore */
    }
  }, [runMode]);

  useEffect(() => {
    try {
      localStorage.setItem(ACTIVE_INSPECTOR_STORAGE_KEY, activeInspector);
    } catch {
      /* ignore */
    }
  }, [activeInspector]);

  const toggleTheme = useCallback(() => {
    setTheme((prev) => {
      const next: Theme = prev === 'light' ? 'dark' : 'light';
      try {
        localStorage.setItem('deepseek-theme', next);
      } catch {
        /* ignore */
      }
      return next;
    });
  }, []);

  /** Re-sync sidebar runtime dot; if probe is OK, drop stale transport-level error banners. */
  const reconcileRuntimeAfterFetchFailure = useCallback(() => {
    void probeRuntimeConnection().then((s) => {
      setRuntimeConn(s);
      if (s === 'connected') {
        setBanner((b) => (!b ? null : shouldClearBannerWhenRuntimeConnected(b) ? null : b));
      }
    });
  }, []);

  const refreshSessions = useCallback(async () => {
    try {
      const list = await getSessions();
      setSessions(list);
      setBanner(null);
    } catch (e) {
      const err = e as Error & { status?: number };
      if (err.status === 401) {
        setBanner(
          '未授权：运行时 token 无效。请通过桌面应用启动（或检查 DEEPSEEK_RUNTIME_TOKEN / sidecar 与 WebView 是否同一会话）。',
        );
      } else {
        setBanner(`无法加载会话列表：${err.message}`);
      }
      reconcileRuntimeAfterFetchFailure();
    }
  }, [reconcileRuntimeAfterFetchFailure]);

  const retryConnectAndSessions = useCallback(async () => {
    setBanner(null);
    setRuntimeConn('checking');
    try {
      await initRuntimeConfig();
      const runtimeUrl = getRuntimeBase();
      const ok = await waitForRuntimeReady({ timeoutMs: 60_000, intervalMs: 400 });
      const probed = await probeRuntimeConnection();
      setRuntimeConn(probed);
      if (!ok) {
        setBanner(
          `仍未连接本地运行时（${runtimeUrl}）。请确认已安装带 sidecar 的版本，或重启应用后再试。`,
        );
        return;
      }
      await refreshSessions();
    } catch (e) {
      setBanner(`重试失败：${(e as Error).message}`);
      setRuntimeConn('offline');
    }
  }, [refreshSessions]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const ok = await waitForRuntimeReady({ timeoutMs: 90_000, intervalMs: 400 });
        if (!cancelled) {
          const probed = await probeRuntimeConnection();
          setRuntimeConn(probed);
        }
        if (cancelled) {
          return;
        }
        if (!ok) {
          setBanner(
            `无法连接本地运行时（${getRuntimeBase()}）。本地服务可能仍在启动，请点击「重试连接」；若多次失败请重启应用或检查是否已内置 sidecar。`,
          );
          return;
        }
        await refreshSessions();
      } catch (e) {
        if (!cancelled) {
          setBanner(`启动检查失败：${(e as Error).message}`);
          setRuntimeConn('offline');
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [refreshSessions]);

  const refreshApiKeyStatus = useCallback(() => {
    void (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const s = await invoke<{ configured: boolean }>('get_api_key_status');
        setDesktopHost(true);
        setDesktopApiKeyConfigured(s.configured);
      } catch {
        setDesktopHost(false);
        setDesktopApiKeyConfigured(null);
      }
    })();
  }, []);

  useEffect(() => {
    refreshApiKeyStatus();
  }, [refreshApiKeyStatus]);

  useEffect(() => {
    let cancelled = false;
    const tick = async () => {
      const s = await probeRuntimeConnection();
      if (cancelled) {
        return;
      }
      setRuntimeConn(s);
      if (s === 'connected') {
        setBanner((b) => {
          if (!b) {
            return null;
          }
          return shouldClearBannerWhenRuntimeConnected(b) ? null : b;
        });
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), 8000);
    return () => {
      cancelled = true;
      window.clearInterval(id);
    };
  }, []);

  const mapSessionMessages = useCallback((detail: Awaited<ReturnType<typeof getSessionDetail>>) => {
    const out: Message[] = [];
    for (const m of detail.messages) {
      const role = m.role === 'user' || m.role === 'assistant' ? m.role : 'assistant';
      const parts: string[] = [];
      for (const b of m.content || []) {
        if (b.type === 'text' && b.text) {
          parts.push(b.text);
        } else if (b.type === 'thinking' && b.text) {
          parts.push(b.text);
        }
      }
      const text = parts.join('\n').trim();
      if (text) {
        out.push({ id: nextId(), role, content: text });
      }
    }
    return out;
  }, []);

  const handleSelectSession = useCallback(
    async (sessionId: string) => {
      const gen = ++selectSessionGenerationRef.current;
      eventAbortRef.current?.abort();
      setBanner(null);
      setActiveSessionId(sessionId);
      setResumedThreadId(null);
      setThreadTrustMode(false);
      setPanelPreview(null);
      lastPersistedTurnRef.current = '';
      try {
        const detail = await getSessionDetail(sessionId);
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        setMessages(mapSessionMessages(detail));
        const resumed = await resumeSessionThread(sessionId);
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        setResumedThreadId(resumed.thread_id);
        threadTurnRef.current = { threadId: resumed.thread_id, turnId: '' };
        try {
          const threadDetail = await getThreadDetail(resumed.thread_id);
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          setSelectedWorkspace(threadDetail.thread.workspace);
          setThreadTrustMode(Boolean(threadDetail.thread.trust_mode));
        } catch (syncErr) {
          if (gen !== selectSessionGenerationRef.current) {
            return;
          }
          const errMsg = syncErr instanceof Error ? syncErr.message : String(syncErr);
          setBanner(`已恢复运行时线程，但读取线程工作区失败：${errMsg}`);
          reconcileRuntimeAfterFetchFailure();
        }
        try {
          localStorage.setItem(ACTIVE_SESSION_STORAGE_KEY, sessionId);
        } catch {
          /* ignore */
        }
      } catch (e) {
        if (gen !== selectSessionGenerationRef.current) {
          return;
        }
        const err = e as Error & { status?: number };
        if (err.status === 401) {
          setBanner('未授权 (401)：请使用桌面壳启动 sidecar 或提供正确的运行时 token。');
        } else {
          setBanner(`加载会话失败：${err.message}`);
        }
        reconcileRuntimeAfterFetchFailure();
      }
    },
    [mapSessionMessages, reconcileRuntimeAfterFetchFailure],
  );

  /** After the sidebar session list loads, re-open the last desktop session (if still present). */
  useEffect(() => {
    if (sessions.length === 0 || startupSessionRestoredRef.current) {
      return;
    }
    const stored = loadStoredActiveSessionId();
    if (!stored) {
      startupSessionRestoredRef.current = true;
      return;
    }
    if (!sessions.some((s) => s.id === stored)) {
      try {
        localStorage.removeItem(ACTIVE_SESSION_STORAGE_KEY);
      } catch {
        /* ignore */
      }
      startupSessionRestoredRef.current = true;
      return;
    }
    startupSessionRestoredRef.current = true;
    void handleSelectSession(stored);
  }, [sessions, handleSelectSession]);

  const handleNewSession = useCallback(() => {
    eventAbortRef.current?.abort();
    selectSessionGenerationRef.current += 1;
    setMessages([]);
    setResumedThreadId(null);
    setThreadTrustMode(false);
    setPanelPreview(null);
    setActiveSessionId(null);
    try {
      localStorage.removeItem(ACTIVE_SESSION_STORAGE_KEY);
    } catch {
      /* ignore */
    }
    threadTurnRef.current = { threadId: '', turnId: '' };
    lastPersistedTurnRef.current = '';
    setApproval(null);
  }, []);

  const handleDeleteSession = useCallback(
    async (sessionId: string) => {
      if (!confirm('确定删除此会话？')) return;
      setBanner(null);
      try {
        await deleteSession(sessionId);
        if (activeSessionId === sessionId) {
          handleNewSession();
        }
        await refreshSessions();
      } catch (e) {
        const err = e as Error & { status?: number };
        setBanner(`删除会话失败：${err.message}`);
      }
    },
    [activeSessionId, handleNewSession, refreshSessions],
  );

  const handleCancelStream = useCallback(() => {
    eventAbortRef.current?.abort();
  }, []);

  const handleComposerWorkspaceChange = useCallback(
    async (next: string) => {
      const trimmed = next.trim();
      if (!trimmed) {
        throw new Error('工作区不能为空');
      }
      if (!resumedThreadId) {
        setSelectedWorkspace(trimmed);
        return;
      }
      try {
        const updated = await patchThread(resumedThreadId, { workspace: trimmed });
        setSelectedWorkspace(typeof updated.workspace === 'string' ? updated.workspace : trimmed);
      } catch (e) {
        const err = e as Error & { status?: number };
        let msg = err.message ?? String(e);
        if (/active turn|finish or interrupt/i.test(msg)) {
          setBanner(
            '当前线程有进行中的回合，暂无法切换工作区。请先停止生成或等待该回合结束后再试。',
          );
        } else {
          setBanner(`更新线程工作区失败：${msg}`);
        }
        throw err;
      }
    },
    [resumedThreadId],
  );

  const closePanelPreview = useCallback(() => {
    setPanelPreview(null);
  }, []);

  const openWorkspaceFileForPreview = useCallback(
    async (relPath: string, title?: string) => {
      if (runtimeConn !== 'connected') {
        throw new Error('本地运行时未连接');
      }
      setActiveInspector('workspace');
      setFocusWorkspaceFilesNonce((n) => n + 1);
      const state = await loadWorkspaceFileIntoPreview({
        relPath,
        title,
        workspaceRoot: selectedWorkspace,
        resumedThreadId,
        desktopHost,
      });
      setPanelPreview(state);
    },
    [runtimeConn, selectedWorkspace, resumedThreadId, desktopHost],
  );

  const handleChatOpenWorkspacePath = useCallback(
    async (relPath: string) => {
      try {
        await openWorkspaceFileForPreview(relPath);
      } catch (e) {
        const err = e instanceof Error ? e.message : String(e);
        setBanner(`无法打开文件：${err}`);
      }
    },
    [openWorkspaceFileForPreview],
  );

  const handleSend = useCallback(
    async (outbound: ComposerOutboundMessage) => {
      if (!outbound.apiPrompt.trim() || streaming) return;

      eventAbortRef.current?.abort();
      eventAbortRef.current = new AbortController();
      const signal = eventAbortRef.current.signal;

      const userMsg: Message = {
        id: nextId(),
        role: 'user',
        content: outbound.displayContent,
      };
      setMessages((prev) => [...prev, userMsg]);

      const assistantId = nextId();
      const assistantMsg: Message = {
        id: assistantId,
        role: 'assistant',
        content: '',
        isStreaming: true,
      };
      setMessages((prev) => [...prev, assistantMsg]);

      setStreaming(true);
      setBanner(null);

      const ctx = {
        currentToolId: { current: null as string | null },
      };

      let finished = false;
      const finishOnce = () => {
        if (finished) return;
        finished = true;
        setStreaming(false);
        setMessages((prev) =>
          prev.map((m) => (m.id === assistantId ? { ...m, isStreaming: false } : m)),
        );
      };

      const maybePersistCompletedTurn = () => {
        const { threadId, turnId } = threadTurnRef.current;
        if (!threadId || !turnId || turnId === lastPersistedTurnRef.current) {
          return;
        }
        lastPersistedTurnRef.current = turnId;
        void (async () => {
          try {
            const res = await persistThreadSession(threadId, activeSessionIdRef.current);
            setActiveSessionId(res.session_id);
            try {
              localStorage.setItem(ACTIVE_SESSION_STORAGE_KEY, res.session_id);
            } catch {
              /* ignore */
            }
            await refreshSessions();
          } catch (e) {
            setBanner(`会话未写入 ~/.deepseek/sessions：${(e as Error).message}`);
          }
        })();
      };

      const applyNorm = (norm: NormalizedStreamEvent) => {
        switch (norm.kind) {
          case 'turn_started':
            threadTurnRef.current = {
              threadId: norm.threadId,
              turnId: norm.turnId,
            };
            if (norm.threadId) {
              setResumedThreadId(norm.threadId);
            }
            break;
          case 'thinking_delta': {
            setMessages((prev) =>
              prev.map((m) => {
                if (m.id !== assistantId) return m;
                return { ...m, thinking: (m.thinking ?? '') + norm.content };
              }),
            );
            break;
          }
          case 'message_delta': {
            setMessages((prev) =>
              prev.map((m) => {
                if (m.id !== assistantId) return m;
                return { ...m, content: m.content + norm.content };
              }),
            );
            break;
          }
          case 'tool_started': {
            ctx.currentToolId.current = norm.id;
            const inputStr = stringifyInput(norm.input);
            setMessages((prev) =>
              prev.map((m) => {
                if (m.id !== assistantId) return m;
                const tools = [
                  ...(m.tools ?? []),
                  { id: norm.id, name: norm.name, input: inputStr, status: 'running' as const },
                ];
                return { ...m, tools };
              }),
            );
            break;
          }
          case 'tool_progress': {
            setMessages((prev) =>
              prev.map((m) => {
                if (m.id !== assistantId) return m;
                const tools = [...(m.tools ?? [])];
                let idx = -1;
                if (ctx.currentToolId.current) {
                  idx = tools.findIndex((t) => t.id === ctx.currentToolId.current);
                }
                if (idx < 0) {
                  for (let i = tools.length - 1; i >= 0; i--) {
                    if (tools[i].status === 'running') {
                      idx = i;
                      break;
                    }
                  }
                }
                if (idx >= 0) {
                  const t = tools[idx];
                  tools[idx] = { ...t, output: (t.output ?? '') + norm.output };
                }
                return { ...m, tools };
              }),
            );
            break;
          }
          case 'tool_completed': {
            const outStr = toolOutputString(norm.output);
            setMessages((prev) =>
              prev.map((m) => {
                if (m.id !== assistantId) return m;
                const tools = (m.tools ?? []).map((t) =>
                  t.id === norm.id
                    ? {
                        ...t,
                        output: outStr || t.output,
                        status: norm.success ? ('done' as const) : ('error' as const),
                      }
                    : t,
                );
                return { ...m, tools };
              }),
            );
            if (ctx.currentToolId.current === norm.id) {
              ctx.currentToolId.current = null;
            }
            break;
          }
          case 'approval_required':
            setApproval({
              toolCallId: norm.id,
              toolName: norm.toolName,
              description: norm.description,
            });
            break;
          case 'turn_completed':
            finishOnce();
            maybePersistCompletedTurn();
            break;
          case 'done':
            finishOnce();
            maybePersistCompletedTurn();
            break;
          case 'error':
            finishOnce();
            setMessages((prev) =>
              prev.map((m) =>
                m.id === assistantId
                  ? { ...m, content: m.content || `Error: ${norm.message}`, isStreaming: false }
                  : m,
              ),
            );
            setBanner(norm.message || '流式错误');
            break;
          default:
            break;
        }
      };

      const onSseEvent = (ev: SseTurnEvent, filter?: { turnId: string }) => {
        if (signal.aborted) return;
        const norm = normalizeDesktopStreamEvent(ev, filter);
        if (norm) {
          applyNorm(norm);
        }
      };

      const handleHttpError = (err: Error & { status?: number }) => {
        const msg = err.message || String(err);
        const status = err.status;
        if (status === 401) {
          setBanner('未授权 (401)：运行时 Bearer token 与 sidecar 不一致。');
        } else if (/api\s*key|DEEPSEEK_API_KEY|401|unauthorized/i.test(msg)) {
          setBanner(
            '可能缺少或无效的 DeepSeek API Key。请在 ~/.deepseek/config.toml 或环境变量 DEEPSEEK_API_KEY 中配置后再试。',
          );
        }
        setMessages((prev) =>
          prev.map((m) =>
            m.id === assistantId
              ? { ...m, content: m.content || `Error: ${msg}`, isStreaming: false }
              : m,
          ),
        );
        finishOnce();
      };

      const streamOpts = streamFlagsForRunMode(runMode, autoApprove);

      try {
        if (resumedThreadId) {
          const detail = await getThreadDetail(resumedThreadId);
          if (signal.aborted) {
            finishOnce();
            return;
          }
          const sinceSeq = detail.latest_seq ?? 0;
          const { turn } = await startThreadTurn(resumedThreadId, {
            prompt: outbound.apiPrompt,
            model: selectedModel,
            mode: streamOpts.mode,
            allow_shell: streamOpts.allow_shell,
            trust_mode: streamOpts.trust_mode,
            auto_approve: streamOpts.auto_approve,
          });
          if (signal.aborted) {
            finishOnce();
            return;
          }
          const turnId = turn.id;
          threadTurnRef.current = {
            threadId: resumedThreadId,
            turnId,
          };

          await getThreadEvents(
            resumedThreadId,
            sinceSeq,
            (ev) => onSseEvent(ev, { turnId }),
            { signal },
          );
          finishOnce();
        } else {
          await postStreamTurn(
            {
              prompt: outbound.apiPrompt,
              workspace: selectedWorkspace,
              mode: streamOpts.mode,
              model: selectedModel,
              allow_shell: streamOpts.allow_shell,
              trust_mode: streamOpts.trust_mode,
              auto_approve: streamOpts.auto_approve,
            },
            (ev) => onSseEvent(ev),
            () => finishOnce(),
            (err) => handleHttpError(err as Error & { status?: number }),
            { signal },
          );
        }
      } catch (e) {
        if ((e as Error).name === 'AbortError') {
          finishOnce();
          return;
        }
        handleHttpError(e as Error & { status?: number });
      }
    },
    [streaming, resumedThreadId, autoApprove, runMode, selectedModel, selectedWorkspace, refreshSessions],
  );

  const handleApproveDecision = async (decision: 'approve' | 'deny') => {
    if (!approval) return;
    const { threadId, turnId } = threadTurnRef.current;
    if (!threadId || !turnId) {
      setBanner('无法解析审批：缺少 thread / turn。请等待 turn.started 后重试。');
      setApproval(null);
      return;
    }
    setApprovalBusy(true);
    try {
      await postResolveApproval(threadId, turnId, approval.toolCallId, decision);
    } catch (e) {
      const err = e as Error & { status?: number };
      if (err.status === 409) {
        setBanner(
          '该工具审批已失效（可能已自动批准/拒绝或已超时）。若未勾选「自动批准」，请在时限内操作；可关闭此提示并继续对话。',
        );
      } else {
        setBanner(`审批提交失败：${err.message}`);
      }
    } finally {
      setApprovalBusy(false);
      setApproval(null);
    }
  };

  return (
    <div className="flex h-screen w-screen bg-canvas">
      <ApprovalDialog
        open={approval != null}
        toolName={approval?.toolName ?? ''}
        description={approval?.description ?? ''}
        busy={approvalBusy}
        onApprove={() => void handleApproveDecision('approve')}
        onDeny={() => void handleApproveDecision('deny')}
      />
      <Sidebar
        sessions={sessions}
        activeSessionId={activeSessionId}
        onNewSession={handleNewSession}
        onSelectSession={handleSelectSession}
        onDeleteSession={handleDeleteSession}
        desktopHost={desktopHost}
        runtimeConn={runtimeConn}
        apiKeyConfigured={desktopApiKeyConfigured}
        activeInspector={activeInspector}
        onInspectorChange={setActiveInspector}
        theme={theme}
        onToggleTheme={toggleTheme}
      />
      <div className="flex flex-1 flex-col min-w-0 border-r border-divider">
        {banner && (
          <div className="shrink-0 border-b border-divider bg-amber-bg px-4 py-2 text-sm text-amber-text">
            {banner}
            <button type="button" className="ml-3 underline" onClick={() => setBanner(null)}>
              关闭
            </button>
            <button
              type="button"
              className="ml-3 underline"
              onClick={() => void retryConnectAndSessions()}
            >
              重试连接
            </button>
          </div>
        )}
        {resumedThreadId && (
          <p className="shrink-0 px-4 py-1 text-xs text-t-text-muted border-b border-divider">
            已恢复线程（runtime）：{resumedThreadId.slice(0, 8)}… · 继续对话将订阅该线程事件流
          </p>
        )}
        <ChatView messages={messages} onOpenWorkspacePath={handleChatOpenWorkspacePath} />
        <Composer
          onSend={handleSend}
          onCancel={handleCancelStream}
          disabled={streaming}
          autoApprove={autoApprove}
          onAutoApproveChange={setAutoApprove}
          runMode={runMode}
          onRunModeChange={setRunMode}
          model={selectedModel}
          onModelChange={setSelectedModel}
          workspace={selectedWorkspace}
          onWorkspaceChange={handleComposerWorkspaceChange}
          resumedThreadActive={resumedThreadId != null && resumedThreadId.length > 0}
        />
      </div>
      <RightPanel
        view={activeInspector}
        desktopHost={desktopHost}
        runtimeConn={runtimeConn}
        apiKeyConfigured={desktopApiKeyConfigured}
        onSavedApiKey={() => {
          refreshApiKeyStatus();
          setBanner(null);
        }}
        theme={theme}
        onToggleTheme={toggleTheme}
        workspaceRoot={selectedWorkspace}
        resumedThreadId={resumedThreadId}
        threadTrustMode={threadTrustMode}
        onEnableTrust={async () => {
          if (!resumedThreadId) return;
          try {
            await patchThread(resumedThreadId, { trust_mode: true });
            setThreadTrustMode(true);
            setBanner(null);
          } catch (e) {
            const err = e as Error & { status?: number };
            setBanner(`启用信任模式失败：${err.message}`);
          }
        }}
        preview={panelPreview}
        onClosePreview={closePanelPreview}
        openWorkspaceFile={openWorkspaceFileForPreview}
        focusFilesNonce={focusWorkspaceFilesNonce}
      />
    </div>
  );
}
