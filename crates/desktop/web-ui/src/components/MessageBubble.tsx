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
  const likelyInReasoningPhase =
    Boolean(message.isStreaming) &&
    !isUser &&
    !message.content &&
    !(message.tools && message.tools.length > 0);
  const showReasoningBlock = Boolean(message.thinking) || likelyInReasoningPhase;

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
        {showReasoningBlock && (
          <div className="mb-2 p-2 bg-gray-700/50 rounded text-xs text-gray-400 min-h-[3rem]">
            <div className="font-medium text-indigo-400 mb-1">💭 Reasoning</div>
            <div className="whitespace-pre-wrap max-h-[48vh] overflow-y-auto text-gray-300">
              {message.thinking ||
                (message.isStreaming ? '推理中…（内容流式到达后会显示在这里）' : '')}
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
        {message.isStreaming && !likelyInReasoningPhase && (
          <span className="inline-block w-2 h-4 ml-0.5 bg-indigo-400 animate-pulse rounded-sm align-middle" />
        )}
      </div>
    </div>
  );
}
