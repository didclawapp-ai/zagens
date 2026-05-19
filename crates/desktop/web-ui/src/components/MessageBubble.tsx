import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { ChatMarkdown } from './ChatMarkdown';
import { ToolCard, type ToolCardModel } from './ToolCard';
import TerminalCard from './TerminalCard';
import DiffCard from './DiffCard';
import { extractUnifiedDiff, parseFileNameFromToolInput } from '../lib/diff/diffEntries';

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
  onOpenWorkspacePath,
  onEditMessage,
  onRetryMessage,
  onOpenDiffInPanel,
}: {
  message: Message;
  onOpenWorkspacePath: (relPath: string) => void | Promise<void>;
  onEditMessage?: (messageId: string, content: string) => void;
  onRetryMessage?: (content: string) => void;
  onOpenDiffInPanel?: () => void;
}) {
  const isUser = message.role === 'user';
  const likelyInReasoningPhase =
    Boolean(message.isStreaming) &&
    !isUser &&
    !message.content &&
    !(message.tools && message.tools.length > 0);
  const showReasoningBlock = Boolean(message.thinking) || likelyInReasoningPhase;

  // Collapsed by default; user expands when they want the transcript (streaming or done).
  const [reasoningExpanded, setReasoningExpanded] = useState(false);
  const [toolsExpanded, setToolsExpanded] = useState(false);

  const runningToolCount =
    message.tools?.filter((t) => t.status === 'running').length ?? 0;

  const reasoningScrollRef = useRef<HTMLDivElement>(null);
  /** While streaming, follow new tokens unless the user scrolled up to read earlier text. */
  const stickReasoningBottomRef = useRef(true);
  const prevStreamingRef = useRef(false);

  useEffect(() => {
    const now = Boolean(message.isStreaming);
    if (now && !prevStreamingRef.current) {
      stickReasoningBottomRef.current = true;
    }
    prevStreamingRef.current = now;
  }, [message.isStreaming]);

  const onReasoningScroll = () => {
    const el = reasoningScrollRef.current;
    if (!el || !message.isStreaming) return;
    const thresholdPx = 72;
    stickReasoningBottomRef.current =
      el.scrollHeight - el.scrollTop - el.clientHeight <= thresholdPx;
  };

  useLayoutEffect(() => {
    if (!reasoningExpanded || !showReasoningBlock) return;
    const el = reasoningScrollRef.current;
    if (!el || !stickReasoningBottomRef.current) return;
    el.scrollTop = el.scrollHeight;
  }, [message.thinking, message.isStreaming, reasoningExpanded, showReasoningBlock]);

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
              <span>工具调用</span>
              <span className="text-t-text-muted font-normal">({message.tools.length})</span>
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
                {message.tools.map((t) => renderToolCard(t, onOpenDiffInPanel))}
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
                title="编辑此消息"
              >
                ✎ 编辑
              </button>
            )}
          </div>
        )}
        <div className="text-sm leading-relaxed break-words">
          {message.content.trim() ? (
            <ChatMarkdown
              content={message.content}
              variant={message.role === 'user' ? 'user' : message.role === 'system' ? 'system' : 'assistant'}
              isStreaming={message.isStreaming}
              onOpenWorkspacePath={onOpenWorkspacePath}
            />
          ) : (
            <span className={`whitespace-pre-wrap ${message.isStreaming ? 'streaming-cursor' : ''}`}>
              {!message.isStreaming ? '...' : ''}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}

const ANSI_CSI = /\x1B\[/;

/** Route tool cards to specialized renderers based on tool name. */
function renderToolCard(tool: ToolCardModel, onOpenDiffInPanel?: () => void) {
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

    if (diffText && tool.status === 'done') {
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

  // Default: plain ToolCard
  return <ToolCard key={tool.id} tool={tool} />;
}

function tryParseCommand(input: string): string | undefined {
  try {
    const j = JSON.parse(input) as Record<string, unknown>;
    return typeof j.command === 'string' ? j.command : undefined;
  } catch {
    return undefined;
  }
}

