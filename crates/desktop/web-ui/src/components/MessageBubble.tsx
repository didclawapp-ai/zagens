import { useEffect, useLayoutEffect, useRef, useState, type TransitionEvent } from 'react';
import { ChatMarkdown } from './ChatMarkdown';
import { ToolCard, type ToolCardModel } from './ToolCard';
import TerminalCard from './TerminalCard';
import DiffCard from './DiffCard';
import { AgentSpawnInline } from './AgentSpawnInline';
import { extractUnifiedDiff, parseFileNameFromToolInput } from '../lib/diff/diffEntries';
import { MessageMetaBar } from './chat/MessageMetaBar';
import { IconCopy, IconPencil, IconRefresh, IconSparkle, IconUndo, IconWrench } from './icons/FlatIcons';
import { formatToolsForCopy } from '../lib/formatToolCopy';
import { summarizeToolCalls } from '../lib/chat/summarizeToolCalls';
import { parseAgentIdFromSpawnOutput } from '../lib/chat/toolOutput';
import { isAgentSpawnToolName } from '../lib/agentSpawnMeta';
import type { AgentState } from '../types/agent';
import { useT } from '../i18n';

interface Message {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking?: string;
  tools?: ToolCardModel[];
  isStreaming?: boolean;
}

export function MessageBubble({
  message,
  workspaceRoot,
  desktopHost,
  onOpenWorkspacePath,
  onRevealWorkspacePath,
  onEditMessage,
  onRetryMessage,
  onOpenDiffInPanel,
  agentStates,
  onBacktrackFromMessage,
  backtrackEnabled = false,
}: {
  message: Message;
  workspaceRoot?: string;
  desktopHost?: boolean;
  onOpenWorkspacePath: (relPath: string) => void | Promise<void>;
  onRevealWorkspacePath?: (relPath: string) => void;
  onEditMessage?: (messageId: string, content: string) => void;
  onRetryMessage?: (content: string) => void;
  onOpenDiffInPanel?: () => void;
  agentStates?: AgentState[];
  onBacktrackFromMessage?: (messageId: string, content: string) => void;
  backtrackEnabled?: boolean;
}) {
  const { t } = useT();
  const isUser = message.role === 'user';
  const reasoningCopyText = message.thinking?.trim() ?? '';
  const toolsCopyText = formatToolsForCopy(message.tools ?? []);
  const likelyInReasoningPhase =
    Boolean(message.isStreaming) &&
    !isUser &&
    !message.content &&
    !(message.tools && message.tools.length > 0);
  const showReasoningBlock = Boolean(message.thinking) || likelyInReasoningPhase;

  const defaultReasoningExpanded =
    Boolean(message.isStreaming) &&
    (Boolean(message.thinking?.trim()) || likelyInReasoningPhase);
  /** Tools stay collapsed by default — user expands manually (Codex-style). */
  const defaultToolsExpanded = false;

  const [reasoningExpanded, setReasoningExpanded] = useState(defaultReasoningExpanded);
  const [toolsExpanded, setToolsExpanded] = useState(defaultToolsExpanded);
  const toolsSummaryLabel = summarizeToolCalls(message.tools ?? [], t);

  const runningToolCount =
    message.tools?.filter((t) => t.status === 'running').length ?? 0;

  /** User manually toggled meta sections — don't fight their choice during streaming. */
  const userToggledReasoningRef = useRef(false);
  const userToggledToolsRef = useRef(false);

  const reasoningScrollRef = useRef<HTMLDivElement>(null);
  const bodyScrollRef = useRef<HTMLDivElement>(null);
  /** While streaming, follow new tokens unless the user scrolled up to read earlier text. */
  const stickReasoningBottomRef = useRef(true);
  const stickBodyBottomRef = useRef(true);
  const prevStreamingRef = useRef(false);
  const bodyCapHeightRef = useRef(320);

  const isAssistant = !isUser;
  type BodyScrollMode = 'streaming' | 'expanding' | 'open';
  const [bodyScrollMode, setBodyScrollMode] = useState<BodyScrollMode>(() =>
    isAssistant && message.isStreaming ? 'streaming' : 'open',
  );
  const [bodyMaxPx, setBodyMaxPx] = useState<number | null>(null);

  const bodyHasSectionAbove =
    showReasoningBlock || Boolean(message.tools && message.tools.length > 0);

  useEffect(() => {
    if (!isAssistant) {
      return;
    }
    setBodyScrollMode(message.isStreaming ? 'streaming' : 'open');
    setBodyMaxPx(null);
  }, [message.id, isAssistant, message.isStreaming]);

  useEffect(() => {
    userToggledReasoningRef.current = false;
    userToggledToolsRef.current = false;
    setReasoningExpanded(defaultReasoningExpanded);
    setToolsExpanded(defaultToolsExpanded);
  }, [message.id]);

  useEffect(() => {
    if (!isAssistant || !message.isStreaming) {
      return;
    }
    if (
      !userToggledReasoningRef.current &&
      (Boolean(message.thinking?.trim()) || likelyInReasoningPhase)
    ) {
      setReasoningExpanded(true);
    }
  }, [isAssistant, message.isStreaming, message.thinking, likelyInReasoningPhase]);

  useEffect(() => {
    if (!isAssistant || message.isStreaming) {
      return;
    }
    if (prevStreamingRef.current) {
      setReasoningExpanded(false);
      setToolsExpanded(false);
      userToggledReasoningRef.current = false;
      userToggledToolsRef.current = false;
    }
  }, [message.isStreaming, isAssistant]);

  useEffect(() => {
    const now = Boolean(message.isStreaming);
    if (now && !prevStreamingRef.current) {
      stickReasoningBottomRef.current = true;
      stickBodyBottomRef.current = true;
    }
    if (isAssistant) {
      if (now) {
        setBodyScrollMode('streaming');
        setBodyMaxPx(null);
      } else if (prevStreamingRef.current) {
        const reduceMotion =
          typeof window !== 'undefined' &&
          window.matchMedia('(prefers-reduced-motion: reduce)').matches;
        if (reduceMotion) {
          setBodyScrollMode('open');
          setBodyMaxPx(null);
        } else {
          setBodyMaxPx(bodyCapHeightRef.current);
          setBodyScrollMode('expanding');
        }
      }
    }
    prevStreamingRef.current = now;
  }, [message.isStreaming, isAssistant]);

  const onReasoningScroll = () => {
    const el = reasoningScrollRef.current;
    if (!el || !message.isStreaming) return;
    const thresholdPx = 72;
    stickReasoningBottomRef.current =
      el.scrollHeight - el.scrollTop - el.clientHeight <= thresholdPx;
  };

  const onBodyScroll = () => {
    const el = bodyScrollRef.current;
    if (!el || bodyScrollMode !== 'streaming') return;
    const thresholdPx = 72;
    stickBodyBottomRef.current =
      el.scrollHeight - el.scrollTop - el.clientHeight <= thresholdPx;
  };

  const chatLogScrollEl = (): HTMLDivElement | null =>
    bodyScrollRef.current?.closest<HTMLDivElement>('[role="log"]') ?? null;

  const outerChatSticksToBottom = (): boolean => {
    const outer = chatLogScrollEl();
    if (!outer) return true;
    const thresholdPx = 120;
    return outer.scrollHeight - outer.scrollTop - outer.clientHeight <= thresholdPx;
  };

  const onBodyTransitionEnd = (e: TransitionEvent<HTMLDivElement>) => {
    if (e.propertyName !== 'max-height' || bodyScrollMode !== 'expanding') {
      return;
    }
    setBodyScrollMode('open');
    setBodyMaxPx(null);
  };

  useLayoutEffect(() => {
    if (!reasoningExpanded || !showReasoningBlock) return;
    const el = reasoningScrollRef.current;
    if (!el || !stickReasoningBottomRef.current) return;
    el.scrollTop = el.scrollHeight;
  }, [message.thinking, message.isStreaming, reasoningExpanded, showReasoningBlock]);

  useLayoutEffect(() => {
    if (bodyScrollMode !== 'streaming') return;
    const el = bodyScrollRef.current;
    if (!el) return;
    bodyCapHeightRef.current = el.clientHeight;
    if (!stickBodyBottomRef.current && !outerChatSticksToBottom()) return;

    const innerOverflows = el.scrollHeight > el.clientHeight + 2;
    if (innerOverflows) {
      el.scrollTop = el.scrollHeight;
      return;
    }
    const outer = chatLogScrollEl();
    if (outer && outerChatSticksToBottom()) {
      outer.scrollTop = outer.scrollHeight;
    } else if (stickBodyBottomRef.current) {
      el.scrollTop = el.scrollHeight;
    }
  }, [message.content, bodyScrollMode, message.tools]);

  useLayoutEffect(() => {
    if (bodyScrollMode !== 'expanding') return;
    const el = bodyScrollRef.current;
    if (!el) {
      setBodyScrollMode('open');
      setBodyMaxPx(null);
      return;
    }
    const fullPx = el.scrollHeight;
    const startPx = bodyMaxPx ?? el.clientHeight;
    if (fullPx <= startPx + 6) {
      setBodyScrollMode('open');
      setBodyMaxPx(null);
      return;
    }
    const id = requestAnimationFrame(() => {
      requestAnimationFrame(() => setBodyMaxPx(fullPx));
    });
    const fallback = window.setTimeout(() => {
      setBodyScrollMode((m) => (m === 'expanding' ? 'open' : m));
      setBodyMaxPx(null);
    }, 700);
    return () => {
      cancelAnimationFrame(id);
      window.clearTimeout(fallback);
    };
  }, [bodyScrollMode, message.content, bodyMaxPx]);

  const reasoningHint =
    message.isStreaming && !message.thinking?.trim()
      ? t('message.reasoningStreaming')
      : message.thinking?.trim()
        ? t('message.reasoningCollapsed')
        : likelyInReasoningPhase
          ? t('message.reasoningStreaming')
          : t('message.reasoningCollapsed');

  const toolsHint =
    runningToolCount > 0
      ? t('message.toolsRunning', { count: String(runningToolCount) })
      : t('message.toolsCollapsed');

  return (
    <div className={`flex ${isUser ? 'my-3 justify-end' : 'my-5 justify-start'}`}>
      <div
        className={
          isUser
            ? 'message-bubble message-bubble--user max-w-[min(85%,42rem)] px-4 py-2.5 text-t-text'
            : 'message-bubble message-bubble--assistant w-full min-w-0 text-t-text'
        }
      >
        {showReasoningBlock && (
          <MessageMetaBar
            icon={<IconSparkle className="size-3.5" />}
            label={t('message.reasoning')}
            hint={reasoningHint}
            expanded={reasoningExpanded}
            onToggle={() => {
              userToggledReasoningRef.current = true;
              setReasoningExpanded((v) => !v);
            }}
            copyText={reasoningCopyText}
            copyTitle={t('chatMarkdown.copyReasoning')}
            copyDisabled={!reasoningCopyText}
          >
            <div
              ref={reasoningScrollRef}
              onScroll={onReasoningScroll}
              className="max-h-[40vh] overflow-y-auto whitespace-pre-wrap"
            >
              {message.thinking ||
                (message.isStreaming ? t('message.reasoningStreamingPlaceholder') : '')}
            </div>
          </MessageMetaBar>
        )}
        {!isUser && message.tools && message.tools.length > 0 && (
          <MessageMetaBar
            icon={<IconWrench className="size-3.5" />}
            label={toolsSummaryLabel}
            hint={toolsHint}
            expanded={toolsExpanded}
            onToggle={() => {
              userToggledToolsRef.current = true;
              setToolsExpanded((v) => !v);
            }}
            copyText={toolsCopyText}
            copyTitle={t('chatMarkdown.copyTools')}
            copyDisabled={!toolsCopyText}
          >
            <div className="space-y-1.5">
              {message.tools.map((tool) => (
                <div key={tool.id} className="tool-stream-item">
                  {renderToolCard(tool, onOpenDiffInPanel, t('chatMarkdown.copyTool'), agentStates)}
                </div>
              ))}
            </div>
          </MessageMetaBar>
        )}
        {isUser && (
          <div className="message-user-actions mb-1 flex justify-end gap-0.5">
            <button
              type="button"
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(message.content);
                } catch {
                  /* clipboard write failed */
                }
              }}
              className="message-user-action"
              title={t('message.copyMessage')}
            >
              <IconCopy className="size-3.5" />
              <span>{t('message.copyAction')}</span>
            </button>
            {onRetryMessage && (
              <button
                type="button"
                onClick={() => onRetryMessage(message.content)}
                className="message-user-action"
                title={t('message.retryMessage')}
              >
                <IconRefresh className="size-3.5" />
                <span>{t('message.retryAction')}</span>
              </button>
            )}
            {onEditMessage && (
              <button
                type="button"
                onClick={() => onEditMessage(message.id, message.content)}
                className="message-user-action"
                title={t('chat.editTitle')}
              >
                <IconPencil className="size-3.5" />
                <span>{t('chat.editTitle')}</span>
              </button>
            )}
            {backtrackEnabled && onBacktrackFromMessage && (
              <button
                type="button"
                onClick={() => onBacktrackFromMessage(message.id, message.content)}
                className="message-user-action"
                title={t('chat.backtrackTitle')}
              >
                <IconUndo className="size-3.5" />
                <span>{t('chat.backtrackAction')}</span>
              </button>
            )}
          </div>
        )}
        <div
          ref={isAssistant ? bodyScrollRef : undefined}
          onScroll={bodyScrollMode === 'streaming' ? onBodyScroll : undefined}
          onTransitionEnd={onBodyTransitionEnd}
          style={bodyMaxPx != null ? { maxHeight: bodyMaxPx } : undefined}
          className={`break-words ${isUser ? 'text-sm leading-relaxed text-t-text' : ''} ${
            bodyScrollMode === 'streaming' ? 'message-body-scroll--streaming' : ''
          } ${bodyScrollMode === 'expanding' ? 'message-body-scroll--expanding' : ''}${
            bodyHasSectionAbove && bodyScrollMode !== 'open'
              ? ' border-t border-card-border/60 pt-2'
              : ''
          }`}
        >
          {message.content.trim() ? (
            <ChatMarkdown
              content={message.content}
              variant={message.role === 'user' ? 'user' : message.role === 'system' ? 'system' : 'assistant'}
              isStreaming={message.isStreaming}
              workspaceRoot={workspaceRoot}
              desktopHost={desktopHost}
              onOpenWorkspacePath={onOpenWorkspacePath}
              onRevealWorkspacePath={onRevealWorkspacePath}
            />
          ) : (
            <span className="whitespace-pre-wrap">{!message.isStreaming ? '...' : ''}</span>
          )}
        </div>
        {isAssistant && message.isStreaming && (
          <div className="streaming-status-line" aria-live="polite">
            {t('message.generating')}
          </div>
        )}
      </div>
    </div>
  );
}

const ANSI_CSI = /\x1B\[/;

/** Route tool cards to specialized renderers based on tool name. */
function renderToolCard(
  tool: ToolCardModel,
  onOpenDiffInPanel?: () => void,
  copyToolTitle?: string,
  agentStates?: AgentState[],
) {
  const outputHasAnsi = Boolean(tool.output && ANSI_CSI.test(tool.output));

  // Shell tools, or any tool whose output carries terminal SGR sequences (avoids “black slab” in <pre>)
  if (
    tool.name === 'exec_shell' ||
    tool.name === 'task_shell_start' ||
    tool.name === 'task_shell_wait' ||
    outputHasAnsi
  ) {
    return (
      <TerminalCard
        key={tool.id}
        output={tool.output ?? ''}
        command={tryParseCommand(tool.input) ?? tool.name}
        status={tool.status}
      />
    );
  }

  // Diff-producing tools → DiffCard
  if (
    tool.name === 'edit_file' ||
    tool.name === 'apply_patch' ||
    tool.name === 'write_file'
  ) {
    const diffText = extractUnifiedDiff(tool.output ?? '');
    const fileName = parseFileNameFromToolInput(tool.input);

    if (diffText) {
      return (
        <DiffCard
          key={tool.id}
          diffText={diffText}
          fileName={fileName}
          onOpenInPanel={onOpenDiffInPanel}
        />
      );
    }
  }

  if (isAgentSpawnToolName(tool.name)) {
    const agentId = parseAgentIdFromSpawnOutput(tool.output ?? '');
    const linkedAgent =
      agentId != null ? agentStates?.find((a) => a.agentId === agentId) : undefined;
    return (
      <div key={tool.id}>
        <ToolCard tool={tool} copyTitle={copyToolTitle} />
        {linkedAgent ? <AgentSpawnInline agent={linkedAgent} /> : null}
      </div>
    );
  }

  // Default: plain ToolCard
  return <ToolCard key={tool.id} tool={tool} copyTitle={copyToolTitle} />;
}

function tryParseCommand(input: string): string | undefined {
  try {
    const j = JSON.parse(input) as Record<string, unknown>;
    return typeof j.command === 'string' ? j.command : undefined;
  } catch {
    return undefined;
  }
}
