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
}

export default function ChatView({ messages, onOpenWorkspacePath }: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  return (
    <div className="flex-1 overflow-y-auto px-4 py-4">
      {messages.length === 0 && (
        <div className="flex items-center justify-center h-full">
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
        <MessageBubble key={msg.id} message={msg} onOpenWorkspacePath={onOpenWorkspacePath} />
      ))}

      <div ref={bottomRef} />
    </div>
  );
}
