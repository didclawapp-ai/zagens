import { useCallback, useEffect, useRef, useState } from 'react';
import {
  postStreamTurn,
  getSessions,
  getSessionDetail,
  resumeSessionThread,
  getThreadDetail,
  startThreadTurn,
  getThreadEvents,
  postResolveApproval,
  deleteSession,
  waitForRuntimeReady,
  probeRuntimeConnection,
  type RuntimeConnectionState,
  type SessionInfo,
  type SseTurnEvent,
} from './api/client';
import { normalizeDesktopStreamEvent, type NormalizedStreamEvent } from './api/streamNormalize';
import ChatView from './components/ChatView';
import Composer from './components/Composer';
import Sidebar from './components/Sidebar';
import ApprovalDialog from './components/ApprovalDialog';
import ApiKeyDialog from './components/ApiKeyDialog';

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

export default function App() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [streaming, setStreaming] = useState(false);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [banner, setBanner] = useState<string | null>(null);
  const [resumedThreadId, setResumedThreadId] = useState<string | null>(null);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [autoApprove, setAutoApprove] = useState(true);
  const [approval, setApproval] = useState<ApprovalState | null>(null);
  const [approvalBusy, setApprovalBusy] = useState(false);
  const [apiKeyDialogOpen, setApiKeyDialogOpen] = useState(false);
  const [desktopHost, setDesktopHost] = useState(false);
  const [desktopApiKeyConfigured, setDesktopApiKeyConfigured] = useState<boolean | null>(null);
  const [runtimeConn, setRuntimeConn] = useState<RuntimeConnectionState>('checking');

  const eventAbortRef = useRef<AbortController | null>(null);
  const threadTurnRef = useRef<{ threadId: string; turnId: string }>({
    threadId: '',
    turnId: '',
  });

  const refreshSessions = useCallback(async () => {
    try {
      const list = await getSessions();
      setSessions(list);
      setBanner(null);
    } catch (e) {
      const err = e as Error & { status?: number };
      if (err.status === 401) {
        setBanner(
          '未授权：运行时 token 无效。请通过桌面应用启动（或检查 sidecar --auth-token 是否一致）。',
        );
      } else {
        setBanner(`无法加载会话列表：${err.message}`);
      }
    }
  }, []);

  const retryConnectAndSessions = useCallback(async () => {
    setBanner(null);
    setRuntimeConn('checking');
    try {
      const ok = await waitForRuntimeReady({ timeoutMs: 60_000, intervalMs: 400 });
      const probed = await probeRuntimeConnection();
      setRuntimeConn(probed);
      if (!ok) {
        setBanner(
          '仍未连接本地运行时（http://127.0.0.1:7878）。请确认已安装带 sidecar 的版本，或重启应用后再试。',
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
            '无法连接本地运行时（http://127.0.0.1:7878）。本地服务可能仍在启动，请点击「重试连接」；若多次失败请重启应用或检查是否已内置 sidecar。',
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
          if (/token|未授权|401|Bearer|运行时 token/i.test(b)) {
            return null;
          }
          return b;
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
      eventAbortRef.current?.abort();
      setBanner(null);
      setActiveSessionId(sessionId);
      try {
        const detail = await getSessionDetail(sessionId);
        setMessages(mapSessionMessages(detail));
        const resumed = await resumeSessionThread(sessionId);
        setResumedThreadId(resumed.thread_id);
        threadTurnRef.current = { threadId: resumed.thread_id, turnId: '' };
        setSidebarOpen(false);
      } catch (e) {
        const err = e as Error & { status?: number };
        if (err.status === 401) {
          setBanner('未授权 (401)：请使用桌面壳启动 sidecar 或提供正确的运行时 token。');
        } else {
          setBanner(`加载会话失败：${err.message}`);
        }
      }
    },
    [mapSessionMessages],
  );

  const handleNewSession = useCallback(() => {
    eventAbortRef.current?.abort();
    setMessages([]);
    setResumedThreadId(null);
    setActiveSessionId(null);
    threadTurnRef.current = { threadId: '', turnId: '' };
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

  const handleSend = useCallback(
    async (text: string) => {
      if (!text.trim() || streaming) return;

      eventAbortRef.current?.abort();
      eventAbortRef.current = new AbortController();
      const signal = eventAbortRef.current.signal;

      const userMsg: Message = {
        id: nextId(),
        role: 'user',
        content: text,
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
        beforeFirstTool: { current: true },
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

      const applyNorm = (norm: NormalizedStreamEvent) => {
        switch (norm.kind) {
          case 'turn_started':
            threadTurnRef.current = {
              threadId: norm.threadId,
              turnId: norm.turnId,
            };
            break;
          case 'message_delta': {
            const isThinking = ctx.beforeFirstTool.current;
            setMessages((prev) =>
              prev.map((m) => {
                if (m.id !== assistantId) return m;
                if (isThinking) {
                  return { ...m, thinking: (m.thinking ?? '') + norm.content };
                }
                return { ...m, content: m.content + norm.content };
              }),
            );
            break;
          }
          case 'tool_started': {
            ctx.beforeFirstTool.current = false;
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
          case 'done':
            finishOnce();
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

      try {
        if (resumedThreadId) {
          const detail = await getThreadDetail(resumedThreadId);
          if (signal.aborted) {
            finishOnce();
            return;
          }
          const sinceSeq = detail.latest_seq ?? 0;
          const { turn } = await startThreadTurn(resumedThreadId, {
            prompt: text,
            mode: 'agent',
            auto_approve: autoApprove,
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
              prompt: text,
              workspace: '.',
              mode: 'agent',
              auto_approve: autoApprove,
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
    [streaming, resumedThreadId, autoApprove],
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
      const err = e as Error;
      setBanner(`审批提交失败：${err.message}`);
    } finally {
      setApprovalBusy(false);
      setApproval(null);
    }
  };

  return (
    <div className="flex h-screen w-screen bg-gray-950">
      <ApprovalDialog
        open={approval != null}
        toolName={approval?.toolName ?? ''}
        description={approval?.description ?? ''}
        busy={approvalBusy}
        onApprove={() => void handleApproveDecision('approve')}
        onDeny={() => void handleApproveDecision('deny')}
      />
      <ApiKeyDialog
        open={apiKeyDialogOpen}
        onClose={() => setApiKeyDialogOpen(false)}
        onSaved={() => {
          refreshApiKeyStatus();
          setBanner(null);
        }}
      />
      <Sidebar
        sessions={sessions}
        isOpen={sidebarOpen}
        onToggle={() => setSidebarOpen(!sidebarOpen)}
        onNewSession={handleNewSession}
        onSelectSession={handleSelectSession}
        onDeleteSession={handleDeleteSession}
      />
      <div className="flex flex-1 flex-col min-w-0">
        {desktopHost && (
          <div className="shrink-0 flex flex-wrap items-center justify-between gap-3 px-3 py-1.5 border-b border-gray-800/80 bg-gray-950/80">
            <div
              className="flex items-center gap-2 min-w-0 text-xs text-gray-400"
              title="与本地 deepseek-tui 运行时 (127.0.0.1:7878) 的连接状态"
            >
              <span
                className={`shrink-0 inline-block w-2 h-2 rounded-full ${
                  runtimeConn === 'connected'
                    ? 'bg-emerald-500'
                    : runtimeConn === 'auth_mismatch'
                      ? 'bg-amber-400 animate-pulse'
                      : runtimeConn === 'offline'
                        ? 'bg-red-500'
                        : 'bg-gray-500'
                }`}
              />
              <span className="truncate">
                {runtimeConn === 'checking' && '正在检测本地运行时…'}
                {runtimeConn === 'connected' && '本地运行时已连接'}
                {runtimeConn === 'offline' && '本地运行时离线或未就绪'}
                {runtimeConn === 'auth_mismatch' &&
                  '会话令牌不一致（应用正在尝试回收旧进程，或请点击「重试连接」）'}
              </span>
            </div>
            <div className="flex flex-wrap items-center justify-end gap-3 shrink-0">
              <button
                type="button"
                onClick={() => setApiKeyDialogOpen(true)}
                className="text-xs text-indigo-400 hover:text-indigo-300"
              >
                API Key 设置
              </button>
              {desktopApiKeyConfigured === false && (
                <span className="text-xs text-amber-500/90">未检测到本地 API Key，发送消息可能失败</span>
              )}
            </div>
          </div>
        )}
        {banner && (
          <div className="shrink-0 px-4 py-2 bg-amber-900/80 text-amber-100 text-sm border-b border-amber-800">
            {banner}
            <button type="button" className="ml-3 underline" onClick={() => setBanner(null)}>
              关闭
            </button>
            <button
              type="button"
              className="ml-3 underline text-amber-50"
              onClick={() => void retryConnectAndSessions()}
            >
              重试连接
            </button>
          </div>
        )}
        {resumedThreadId && (
          <p className="shrink-0 px-4 py-1 text-xs text-gray-500 border-b border-gray-800">
            已恢复线程（runtime）：{resumedThreadId.slice(0, 8)}… · 继续对话将订阅该线程事件流
          </p>
        )}
        <ChatView messages={messages} />
        <Composer
          onSend={handleSend}
          onCancel={handleCancelStream}
          disabled={streaming}
          autoApprove={autoApprove}
          onAutoApproveChange={setAutoApprove}
        />
      </div>
    </div>
  );
}
