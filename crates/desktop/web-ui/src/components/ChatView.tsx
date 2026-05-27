import { useLayoutEffect, useRef } from 'react';
import { ChatErrorBoundary } from './ChatErrorBoundary';
import { MessageBubble } from './MessageBubble';
import type { ToolCardModel } from './ToolCard';
import type { AgentState } from '../types/agent';
import { isLastUserMessage } from '../lib/chat/backtrackDepth';
import { useT } from '../i18n';

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
  agentStates?: AgentState[];
  onOpenWorkspacePath: (relPath: string) => void | Promise<void>;
  onRevealWorkspacePath?: (relPath: string) => void;
  onEditMessage?: (messageId: string, content: string) => void;
  onRetryMessage?: (content: string) => void;
  onOpenDiffInPanel?: () => void;
  onBacktrackFromMessage?: (messageId: string, content: string) => void;
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
  agentStates,
  onOpenWorkspacePath,
  onRevealWorkspacePath,
  onEditMessage,
  onRetryMessage,
  onOpenDiffInPanel,
  onBacktrackFromMessage,
}: Props) {
  const { t } = useT();
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
      aria-label={t('a11y.chatLog')}
      aria-live="polite"
      aria-relevant="additions"
    >
      {/* Match Composer: mx-auto max-w-3xl so transcript edges align with the input card */}
      <div className="mx-auto w-full max-w-3xl">
        {messages.length === 0 && (
          <div className="flex min-h-[min(60vh,28rem)] items-center justify-center">
            <div className="text-center">
              <h1 className="text-3xl font-bold mb-2 text-accent font-display">
                {t('app.title')}
              </h1>
              <p className="text-lg text-t-text-secondary">{t('app.heroTagline')}</p>
              <p className="text-sm mt-2 text-t-text-muted">{t('app.emptyPrompt')}</p>
            </div>
          </div>
        )}

        {messages.map((msg) => (
          <ChatErrorBoundary key={msg.id}>
            <MessageBubble
              message={msg}
              workspaceRoot={workspaceRoot}
              desktopHost={desktopHost}
              agentStates={agentStates}
              onOpenWorkspacePath={onOpenWorkspacePath}
              onRevealWorkspacePath={onRevealWorkspacePath}
              onEditMessage={onEditMessage}
              onRetryMessage={onRetryMessage}
              onOpenDiffInPanel={onOpenDiffInPanel}
              onBacktrackFromMessage={onBacktrackFromMessage}
              backtrackEnabled={
                msg.role === 'user' &&
                Boolean(onBacktrackFromMessage) &&
                !isLastUserMessage(messages, msg.id)
              }
            />
          </ChatErrorBoundary>
        ))}

        <div ref={bottomRef} />
      </div>
    </div>
  );
}
