import { useCallback, useEffect, useState } from 'react';
import {
  postStreamTurn,
  getSessions,
  getSessionDetail,
  resumeSessionThread,
  type SessionInfo,
} from './api/client';
import type { SseTurnEvent } from './api/client';
import ChatView from './components/ChatView';
import Composer from './components/Composer';
import Sidebar from './components/Sidebar';

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

let msgId = 0;
function nextId() {
  return `msg-${++msgId}`;
}

export default function App() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [thinking, setThinking] = useState('');
  const [currentText, setCurrentText] = useState('');
  const [streaming, setStreaming] = useState(false);
  const [sessions, setSessions] = useState<SessionInfo[]>([]);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [banner, setBanner] = useState<string | null>(null);
  const [resumedThreadId, setResumedThreadId] = useState<string | null>(null);

  useEffect(() => {
    getSessions()
      .then(setSessions)
      .catch((e: Error & { status?: number }) => {
        if (e.status === 401) {
          setBanner('未授权：运行时 token 无效。请通过桌面应用启动（或检查 sidecar --auth-token 是否一致）。');
        } else {
          setBanner(`无法加载会话列表：${e.message}`);
        }
      });
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
      setBanner(null);
      try {
        const detail = await getSessionDetail(sessionId);
        setMessages(mapSessionMessages(detail));
        const resumed = await resumeSessionThread(sessionId);
        setResumedThreadId(resumed.thread_id);
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

  const handleSend = useCallback(
    async (text: string) => {
      if (!text.trim() || streaming) return;

      const userMsg: Message = {
        id: nextId(),
        role: 'user',
        content: text,
      };
      setMessages((prev) => [...prev, userMsg]);
      setCurrentText('');
      setThinking('');
      setStreaming(true);

      const assistantId = nextId();
      const assistantMsg: Message = {
        id: assistantId,
        role: 'assistant',
        content: '',
        isStreaming: true,
      };
      setMessages((prev) => [...prev, assistantMsg]);

      await postStreamTurn(
        {
          prompt: text,
          workspace: '.',
          mode: 'agent',
          auto_approve: true,
        },
        (event: SseTurnEvent) => {
          try {
            const payload = JSON.parse(event.data);

            switch (event.event) {
              case 'message.delta':
                const content = payload.content || '';
                setCurrentText((prev) => prev + content);
                setMessages((prev) =>
                  prev.map((m) =>
                    m.id === assistantId
                      ? { ...m, content: m.content + content }
                      : m,
                  ),
                );
                break;

              case 'turn.completed':
                // nothing special on the frontend
                break;

              case 'tool.started':
              case 'tool.completed':
              case 'tool.progress':
              case 'status':
              case 'error':
              case 'approval.required':
                break;

              default:
                break;
            }
          } catch (_) {}
        },
        () => {
          setStreaming(false);
          setMessages((prev) =>
            prev.map((m) =>
              m.id === assistantId ? { ...m, isStreaming: false } : m,
            ),
          );
        },
        (err) => {
          setStreaming(false);
          const msg = err.message || String(err);
          const status = (err as Error & { status?: number }).status;
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
                ? { ...m, content: `Error: ${msg}`, isStreaming: false }
                : m,
            ),
          );
        },
      );
    },
    [streaming],
  );

  const handleNewSession = () => {
    setMessages([]);
    setCurrentText('');
    setThinking('');
    setResumedThreadId(null);
  };

  return (
    <div className="flex h-screen w-screen bg-gray-950">
      <Sidebar
        sessions={sessions}
        isOpen={sidebarOpen}
        onToggle={() => setSidebarOpen(!sidebarOpen)}
        onNewSession={handleNewSession}
        onSelectSession={handleSelectSession}
      />
      <div className="flex flex-1 flex-col min-w-0">
        {banner && (
          <div className="shrink-0 px-4 py-2 bg-amber-900/80 text-amber-100 text-sm border-b border-amber-800">
            {banner}
            <button
              type="button"
              className="ml-3 underline"
              onClick={() => setBanner(null)}
            >
              关闭
            </button>
          </div>
        )}
        {resumedThreadId && (
          <p className="shrink-0 px-4 py-1 text-xs text-gray-500 border-b border-gray-800">
            已恢复线程（runtime）：{resumedThreadId.slice(0, 8)}… · 新消息仍走兼容流式接口
          </p>
        )}
        <ChatView
          messages={messages}
          thinking={thinking}
          currentText={currentText}
          streaming={streaming}
        />
        <Composer onSend={handleSend} disabled={streaming} />
      </div>
    </div>
  );
}
