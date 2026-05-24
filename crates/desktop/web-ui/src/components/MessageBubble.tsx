import { useEffect, useLayoutEffect, useRef, useState, type TransitionEvent } from 'react';
import { ChatMarkdown } from './ChatMarkdown';
import { ToolCard, type ToolCardModel } from './ToolCard';
import TerminalCard from './TerminalCard';
import DiffCard from './DiffCard';
import { AgentSpawnInline } from './AgentSpawnInline';
import { extractUnifiedDiff, parseFileNameFromToolInput } from '../lib/diff/diffEntries';
import CopyTextButton from './CopyTextButton';
import { formatToolsForCopy } from '../lib/formatToolCopy';
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

  const [reasoningExpanded, setReasoningExpanded] = useState(true);
  const [toolsExpanded, setToolsExpanded] = useState(false);
  const toolsSummaryLabel = summarizeToolCalls(message.tools ?? []);

  const runningToolCount =
    message.tools?.filter((t) => t.status === 'running').length ?? 0;

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
  }, [message.id, isAssistant]);

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

  return (
    <div className={`my-3 flex ${isUser ? 'justify-end' : 'justify-start'}`}>
      <div
        className={`rounded-xl px-4 py-3 ${
          isUser
            ? 'max-w-[80%] bg-msg-user text-msg-user-text rounded-br-sm border border-msg-user-border shadow-sm'
            : 'w-full min-w-0 bg-msg-assistant text-t-text rounded-bl-sm border border-msg-assistant-border shadow-sm'
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
              <CopyTextButton
                getText={() => reasoningCopyText}
                title={t('chatMarkdown.copyReasoning')}
                disabled={!reasoningCopyText}
                className="ml-1"
              />
              {!reasoningExpanded && (
                <span className="ml-auto truncate text-[11px] font-normal text-t-text-muted">
                  {message.isStreaming && !message.thinking?.trim()
                    ? '推理中…'
                    : message.thinking?.trim()
                      ? '已收起，点击展开'
                      : likelyInReasoningPhase
                        ? '推理中…'
                        : '已收起，点击展开'}
                </span>
              )}
            </button>
            {reasoningExpanded && (
              <div
                ref={reasoningScrollRef}
                onScroll={onReasoningScroll}
                className="max-h-[48vh] overflow-y-auto border-t border-card-border px-2.5 pb-2.5 pt-0 leading-relaxed text-t-text-secondary whitespace-pre-wrap"
              >
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
              <span className="min-w-0 truncate font-mono text-[11px] sm:text-xs sm:font-sans">
                {toolsSummaryLabel}
              </span>
              <CopyTextButton
                getText={() => toolsCopyText}
                title={t('chatMarkdown.copyTools')}
                disabled={!toolsCopyText}
                className="ml-1"
              />
              {!toolsExpanded && (
                <span className="ml-auto truncate text-[11px] font-normal text-t-text-muted">
                  {runningToolCount > 0
                    ? `${runningToolCount} 个进行中 · 点击展开`
                    : '已收起，点击展开'}
                </span>
              )}
            </button>
            {toolsExpanded && (
              <div className="space-y-1.5 border-t border-divider px-2.5 pb-2.5 pt-2">
                {message.tools.map((tool) =>
                  renderToolCard(tool, onOpenDiffInPanel, t('chatMarkdown.copyTool'), agentStates),
                )}
              </div>
            )}
          </div>
        )}
        {isUser && (
          <div className="flex justify-end gap-0.5 mb-1 opacity-0 hover:opacity-100 transition-opacity">
            <button
              type="button"
              onClick={async () => {
                try {
                  await navigator.clipboard.writeText(message.content);
                } catch {
                  /* clipboard write failed */
                }
              }}
              className="text-[10px] text-t-text-muted hover:text-t-text px-2 py-0.5 rounded"
              title="复制消息"
            >
              📋 复制
            </button>
            {onRetryMessage && (
              <button
                type="button"
                onClick={() => onRetryMessage(message.content)}
                className="text-[10px] text-t-text-muted hover:text-accent px-2 py-0.5 rounded"
                title="重新发送此消息"
              >
                🔄 重试
              </button>
            )}
            {onEditMessage && (
              <button
                type="button"
                onClick={() => onEditMessage(message.id, message.content)}
                className="text-[10px] text-t-text-muted hover:text-accent px-2 py-0.5 rounded"
                title={t('chat.editTitle')}
              >
                ✎ {t('chat.editTitle')}
              </button>
            )}
            {backtrackEnabled && onBacktrackFromMessage && (
              <button
                type="button"
                onClick={() => onBacktrackFromMessage(message.id, message.content)}
                className="text-[10px] text-t-text-muted hover:text-accent px-2 py-0.5 rounded"
                title={t('chat.backtrackTitle')}
              >
                ↩ {t('chat.backtrackAction')}
              </button>
            )}
          </div>
        )}
        <div
          ref={isAssistant ? bodyScrollRef : undefined}
          onScroll={bodyScrollMode === 'streaming' ? onBodyScroll : undefined}
          onTransitionEnd={onBodyTransitionEnd}
          style={bodyMaxPx != null ? { maxHeight: bodyMaxPx } : undefined}
          className={`text-sm leading-relaxed break-words ${isUser ? 'text-msg-user-text' : ''} ${
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
            生成中
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

/** Collapsed tools header: show actual tool name(s) instead of generic "工具调用". */
function summarizeToolCalls(tools: ToolCardModel[]): string {
  if (tools.length === 0) return '工具调用';

  const running = tools.filter((t) => t.status === 'running');
  if (running.length === 1) {
    const name = running[0].name;
    return tools.length === 1 ? name : `${name} 等 ${tools.length} 项`;
  }

  const uniqueNames = [...new Set(tools.map((t) => t.name))];
  if (uniqueNames.length === 1) {
    return tools.length === 1 ? uniqueNames[0] : `${uniqueNames[0]} ×${tools.length}`;
  }

  const head = uniqueNames.slice(0, 2).join(' · ');
  if (uniqueNames.length > 2 || tools.length > 2) {
    return `${head} 等 ${tools.length} 项`;
  }
  return head;
}

