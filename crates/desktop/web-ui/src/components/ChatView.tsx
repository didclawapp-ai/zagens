import { useEffect, useLayoutEffect, useRef } from 'react';
import { ChatErrorBoundary } from './ChatErrorBoundary';
import { MessageBubble } from './MessageBubble';
import type { ToolCardModel } from './ToolCard';

interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking?: string;
  tools?: ToolCardModel[];
  isStreaming?: boolean;
}

interface Props {
  messages: Message[];
  onOpenWorkspacePath: (relPath: string) => void | Promise<void>;
  onEditMessage?: (messageId: string, content: string) => void;
  onRetryMessage?: (content: string) => void;
  onOpenDiffInPanel?: () => void;
}

export default function ChatView({
  messages,
  onOpenWorkspacePath,
  onEditMessage,
  onRetryMessage,
  onOpenDiffInPanel,
}: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const stickBottomRef = useRef(true);

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const thresholdPx = 120;
    stickBottomRef.current =
      el.scrollHeight - el.scrollTop - el.clientHeight <= thresholdPx;
  };

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el || !stickBottomRef.current) return;
    el.scrollTop = el.scrollHeight;
  }, [messages]);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const streaming = messages.some((m) => m.isStreaming);
    if (!streaming) return;
    const id = window.setInterval(() => {
      if (!stickBottomRef.current) return;
      el.scrollTop = el.scrollHeight;
    }, 200);
    return () => window.clearInterval(id);
  }, [messages]);

  return (
    <div
      ref={scrollRef}
      onScroll={onScroll}
      className="flex min-h-[12rem] flex-1 flex-col overflow-y-auto bg-canvas px-4 py-4"
      role="log"
      aria-label="对话记录"
      aria-live="polite"
    >
      {/* Match Composer: mx-auto max-w-3xl so transcript edges align with the input card */}
      <div className="mx-auto w-full max-w-3xl">
        {messages.length === 0 && (
          <div className="flex min-h-[min(60vh,28rem)] items-center justify-center">
            <div className="text-center">
              <h1 className="text-3xl font-bold mb-2 text-accent font-display">
                DS<span className="opacity-85 font-semibold"> Pick</span>
              </h1>
              <p className="text-lg text-t-text-secondary">你的 AI 编码助手</p>
              <p className="text-sm mt-2 text-t-text-muted">在下方输入问题开始对话</p>
            </div>
          </div>
        )}

        {messages.map((msg) => (
          <ChatErrorBoundary key={msg.id}>
            <MessageBubble
              message={msg}
              onOpenWorkspacePath={onOpenWorkspacePath}
              onEditMessage={onEditMessage}
              onRetryMessage={onRetryMessage}
              onOpenDiffInPanel={onOpenDiffInPanel}
            />
          </ChatErrorBoundary>
        ))}

        <div ref={bottomRef} />
      </div>
    </div>
  );
}
