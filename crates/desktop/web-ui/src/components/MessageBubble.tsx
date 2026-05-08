import { ToolCard, type ToolCardModel } from './ToolCard';

interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking?: string;
  tools?: ToolCardModel[];
  isStreaming?: boolean;
}

export function MessageBubble({ message }: { message: Message }) {
  const isUser = message.role === 'user';

  return (
    <div
      className={`my-3 flex ${isUser ? 'justify-end' : 'justify-start'}`}
    >
      <div
        className={`max-w-[80%] rounded-xl px-4 py-3 ${
          isUser
            ? 'bg-indigo-600 text-white rounded-br-sm'
            : 'bg-gray-800 text-gray-100 rounded-bl-sm border border-gray-700/50'
        }`}
      >
        {message.thinking && (
          <div className="mb-2 p-2 bg-gray-700/50 rounded text-xs text-gray-400">
            <div className="font-medium text-indigo-400 mb-1">
              💭 Reasoning
            </div>
            <div className="whitespace-pre-wrap max-h-40 overflow-y-auto">
              {message.thinking}
            </div>
          </div>
        )}
        {!isUser && message.tools && message.tools.length > 0 && (
          <div className="space-y-1 mb-2">
            {message.tools.map((t) => (
              <ToolCard key={t.id} tool={t} />
            ))}
          </div>
        )}
        <div
          className={`text-sm whitespace-pre-wrap break-words ${
            message.isStreaming ? 'streaming-cursor' : ''
          }`}
        >
          {message.content || (message.isStreaming ? '' : '...')}
        </div>
        {message.isStreaming && (
          <span className="inline-block w-2 h-4 ml-0.5 bg-indigo-400 animate-pulse rounded-sm align-middle" />
        )}
      </div>
    </div>
  );
}
