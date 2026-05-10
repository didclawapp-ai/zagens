import { useEffect, useRef } from 'react';
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
}

export default function ChatView({ messages, onOpenWorkspacePath, onEditMessage }: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  return (
    <div
      className="flex min-h-0 flex-1 flex-col overflow-y-auto bg-canvas px-4 py-4"
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
          <MessageBubble
            key={msg.id}
            message={msg}
            onOpenWorkspacePath={onOpenWorkspacePath}
            onEditMessage={onEditMessage}
          />
        ))}

        <div ref={bottomRef} />
      </div>
    </div>
  );
}
