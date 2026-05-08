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
    <div className={`my-3 flex ${isUser ? 'justify-end' : 'justify-start'}`}>
      <div
        className={`max-w-[80%] rounded-xl px-4 py-3 ${
          isUser
            ? 'bg-accent text-accent-text rounded-br-sm'
            : 'bg-card text-t-text rounded-bl-sm border border-card-border shadow-sm'
        }`}
      >
        {showReasoningBlock && (
          <div className="mb-2 p-2.5 bg-accent-soft rounded-lg text-xs min-h-[3rem]">
            <div className="flex items-center gap-2 font-medium text-accent mb-1.5">
              <span className="text-base">💭</span>
              <span>Reasoning</span>
            </div>
            <div className="whitespace-pre-wrap max-h-[48vh] overflow-y-auto text-t-text-secondary leading-relaxed">
              {message.thinking ||
                (message.isStreaming ? '推理中…（内容流式到达后会显示在这里）' : '')}
            </div>
          </div>
        )}
        {!isUser && message.tools && message.tools.length > 0 && (
          <div className="space-y-1.5 mb-2">
            {message.tools.map((t) => (
              <ToolCard key={t.id} tool={t} />
            ))}
          </div>
        )}
        <div
          className={`text-sm whitespace-pre-wrap break-words leading-relaxed ${
            message.isStreaming ? 'streaming-cursor' : ''
          }`}
        >
          {message.content || (message.isStreaming ? '' : '...')}
        </div>
      </div>
    </div>
  );
}
