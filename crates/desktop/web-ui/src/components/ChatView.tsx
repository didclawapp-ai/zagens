import { useLayoutEffect, useRef } from 'react';
import { ChatErrorBoundary } from './ChatErrorBoundary';
import { MessageBubble } from './MessageBubble';
import type { ToolCardModel } from './ToolCard';
import type { AgentState } from '../types/agent';
import { isLastUserMessage } from '../lib/chat/backtrackDepth';
import { useT } from '../i18n';
import { ChatEmptyState } from './ChatEmptyState';
import { OfficeEmptyState } from './OfficeEmptyState';
import { SessionRestoreBanner } from './chat/SessionRestoreBanner';
import type { SessionRestoreSource } from '../hooks/useSessionNavigation';

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
  officeSession?: boolean;
  onOfficeQuickStart?: (prefill: string) => void;
  sessionRestoreLoading?: boolean;
  sessionRestoreSource?: SessionRestoreSource;
  onRetrySessionRestore?: () => void;
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
  officeSession = false,
  onOfficeQuickStart,
  sessionRestoreLoading = false,
  sessionRestoreSource = null,
  onRetrySessionRestore,
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
      className="flex min-h-[12rem] flex-1 flex-col overflow-y-auto bg-canvas px-4 py-4"
      role="log"
      aria-label={t('a11y.chatLog')}
      aria-live="polite"
      aria-relevant="additions"
    >
      {/* Match Composer: mx-auto max-w-3xl so transcript edges align with the input card */}
      <div className="mx-auto w-full max-w-3xl">
        <SessionRestoreBanner
          loading={sessionRestoreLoading}
          source={sessionRestoreSource}
          onRetry={onRetrySessionRestore}
        />
        {messages.length === 0 &&
          (officeSession && onOfficeQuickStart ? (
            <OfficeEmptyState onPick={onOfficeQuickStart} />
          ) : (
            <ChatEmptyState />
          ))}

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
