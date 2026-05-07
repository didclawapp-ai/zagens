import { useEffect, useRef } from 'react';
import { MessageBubble } from './MessageBubble';

interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking?: string;
  tools?: any[];
  isStreaming?: boolean;
}

interface Props {
  messages: Message[];
  thinking: string;
  currentText: string;
  streaming: boolean;
}

export default function ChatView({
  messages,
  thinking,
  currentText,
  streaming,
}: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages, currentText]);

  return (
    <div className="flex-1 overflow-y-auto px-4 py-4">
      {messages.length === 0 && (
        <div className="flex items-center justify-center h-full">
          <div className="text-center text-gray-500">
            <h1 className="text-3xl font-bold mb-2 text-indigo-400">
              DeepSeek
            </h1>
            <p className="text-lg">你的 AI 编码助手</p>
            <p className="text-sm mt-2">在下方输入问题开始对话</p>
          </div>
        </div>
      )}

      {messages.map((msg) => (
        <MessageBubble key={msg.id} message={msg} />
      ))}

      {thinking && (
        <div className="my-2 p-3 bg-gray-800/50 rounded-lg border border-gray-700/50">
          <div className="text-xs text-indigo-400 font-medium mb-1">
            💭 Thinking
          </div>
          <div className="text-sm text-gray-400 whitespace-pre-wrap">
            {thinking.slice(-300)}
          </div>
        </div>
      )}

      <div ref={bottomRef} />
    </div>
  );
}
