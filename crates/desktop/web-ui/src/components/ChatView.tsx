import { useLayoutEffect, useRef } from 'react';
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
  workspaceRoot?: string;
  desktopHost?: boolean;
  onOpenWorkspacePath: (relPath: string) => void | Promise<void>;
  onEditMessage?: (messageId: string, content: string) => void;
  onRetryMessage?: (content: string) => void;
  onOpenDiffInPanel?: () => void;
}

/** Assistant body scroll cap handles follow-scroll while tokens arrive. */
function delegatesStreamingBodyScroll(messages: Message[]): boolean {
  const last = messages[messages.length - 1];
  return (
    last?.role === 'assistant' &&
    Boolean(last.isStreaming) &&
    Boolean(last.content?.trim())
  );
}

export default function ChatView({
  messages,
  workspaceRoot,
  desktopHost,
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

  const delegateBodyScroll = delegatesStreamingBodyScroll(messages);

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el || !stickBottomRef.current || delegateBodyScroll) return;
    el.scrollTop = el.scrollHeight;
  }, [messages, delegateBodyScroll]);

  return (
    <div
      ref={scrollRef}
      onScroll={onScroll}
      className="flex min-h-[12rem] flex-1 flex-col overflow-y-auto bg-card px-4 py-4"
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
              workspaceRoot={workspaceRoot}
              desktopHost={desktopHost}
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
