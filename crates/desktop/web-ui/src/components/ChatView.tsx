import { useLayoutEffect, useRef } from 'react';
import { ChatErrorBoundary } from './ChatErrorBoundary';
import type { ToolCardModel } from './ToolCard';
import type { AgentState } from '../types/agent';
import type { TurnBlock } from '../lib/chat/timeline/turnBlockTypes';
import { MessageBubble } from './MessageBubble';
import { AssistantTurnFrame } from './chat/timeline/AssistantTurnFrame';
import { useTurnScroll } from './chat/timeline/useTurnScroll';
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
  blocks?: TurnBlock[];
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
  onRewindWorkspaceFromMessage?: (messageId: string, content: string) => void;
  officeSession?: boolean;
  onOfficeQuickStart?: (prefill: string) => void;
  sessionRestoreLoading?: boolean;
  sessionRestoreSource?: SessionRestoreSource;
  onRetrySessionRestore?: () => void;
}

/** Assistant body scroll: timeline text blocks or legacy content while streaming. */
function delegatesStreamingBodyScroll(messages: Message[]): boolean {
  const last = messages[messages.length - 1];
  if (last?.role !== 'assistant' || !last.isStreaming) return false;
  const textBlocks = last.blocks?.filter((b) => b.kind === 'text');
  const textFromBlocks = textBlocks?.[textBlocks.length - 1];
  if (textFromBlocks?.kind === 'text' && textFromBlocks.content.trim()) return true;
  return Boolean(last.content?.trim());
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
  onRewindWorkspaceFromMessage,
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
  const lastAssistant = [...messages].reverse().find((m) => m.role === 'assistant');
  useTurnScroll(
    scrollRef,
    lastAssistant?.blocks ?? [],
    Boolean(lastAssistant?.isStreaming),
  );

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
            {msg.role === 'assistant' ? (
              <AssistantTurnFrame
                message={msg}
                workspaceRoot={workspaceRoot}
                desktopHost={desktopHost}
                agentStates={agentStates}
                onOpenWorkspacePath={onOpenWorkspacePath}
                onRevealWorkspacePath={onRevealWorkspacePath}
                onOpenDiffInPanel={onOpenDiffInPanel}
              />
            ) : (
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
                onRewindWorkspaceFromMessage={onRewindWorkspaceFromMessage}
                rewindWorkspaceEnabled={msg.role === 'user' && Boolean(onRewindWorkspaceFromMessage)}
              />
            )}
          </ChatErrorBoundary>
        ))}

        <div ref={bottomRef} />
      </div>
    </div>
  );
}
