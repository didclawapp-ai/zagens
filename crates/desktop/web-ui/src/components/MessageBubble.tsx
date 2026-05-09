import { useEffect, useState } from 'react';
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

  // Streaming: keep reasoning visible. After the turn completes, fold it so the
  // answer stays primary (similar to compact “thought” UI in other clients).
  const [reasoningExpanded, setReasoningExpanded] = useState(
    () => Boolean(message.isStreaming),
  );
  const [toolsExpanded, setToolsExpanded] = useState(() => Boolean(message.isStreaming));

  useEffect(() => {
    const live = Boolean(message.isStreaming);
    setReasoningExpanded(live);
    setToolsExpanded(live);
  }, [message.isStreaming]);

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
          <div className="mb-2 overflow-hidden rounded-lg bg-accent-soft text-xs">
            <button
              type="button"
              onClick={() => setReasoningExpanded((v) => !v)}
              className="flex w-full items-center gap-2 px-2.5 py-2 text-left font-medium text-accent transition-colors hover:bg-accent-soft/80"
              aria-expanded={reasoningExpanded}
            >
              <span className="w-4 shrink-0 select-none text-[10px] text-t-text-muted">
                {reasoningExpanded ? '▼' : '▶'}
              </span>
              <span className="text-base leading-none">💭</span>
              <span>Reasoning</span>
              {!reasoningExpanded && message.thinking?.trim() && (
                <span className="ml-auto truncate text-[11px] font-normal text-t-text-muted">
                  已收起，点击展开
                </span>
              )}
            </button>
            {reasoningExpanded && (
              <div className="max-h-[48vh] overflow-y-auto border-t border-card-border/40 px-2.5 pb-2.5 pt-0 leading-relaxed text-t-text-secondary whitespace-pre-wrap">
                {message.thinking ||
                  (message.isStreaming ? '推理中…（内容流式到达后会显示在这里）' : '')}
              </div>
            )}
          </div>
        )}
        {!isUser && message.tools && message.tools.length > 0 && (
          <div className="mb-2 overflow-hidden rounded-lg border border-card-border bg-canvas-alt text-xs">
            <button
              type="button"
              onClick={() => setToolsExpanded((v) => !v)}
              className="flex w-full items-center gap-2 px-2.5 py-2 text-left font-medium text-t-text transition-colors hover:bg-hover/50"
              aria-expanded={toolsExpanded}
            >
              <span className="w-4 shrink-0 select-none text-[10px] text-t-text-muted">
                {toolsExpanded ? '▼' : '▶'}
              </span>
              <span className="text-base leading-none">🔧</span>
              <span>工具调用</span>
              <span className="text-t-text-muted font-normal">({message.tools.length})</span>
              {!toolsExpanded && (
                <span className="ml-auto truncate text-[11px] font-normal text-t-text-muted">
                  已收起，点击展开
                </span>
              )}
            </button>
            {toolsExpanded && (
              <div className="space-y-1.5 border-t border-divider px-2.5 pb-2.5 pt-2">
                {message.tools.map((t) => (
                  <ToolCard key={t.id} tool={t} />
                ))}
              </div>
            )}
          </div>
        )}
        <div
          className={`text-sm leading-relaxed break-words whitespace-pre-wrap ${
            message.isStreaming ? 'streaming-cursor' : ''
          }`}
        >
          {message.content || (message.isStreaming ? '' : '...')}
        </div>
      </div>
    </div>
  );
}
