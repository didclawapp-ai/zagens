import { ChatMarkdown } from '../../../ChatMarkdown';
import type { TurnBlock } from '../../../../lib/chat/timeline/turnBlockTypes';

export function TextBlock({
  block,
  workspaceRoot,
  desktopHost,
  isTurnStreaming,
  onOpenWorkspacePath,
  onRevealWorkspacePath,
}: {
  block: Extract<TurnBlock, { kind: 'text' }>;
  workspaceRoot?: string;
  desktopHost?: boolean;
  isTurnStreaming: boolean;
  onOpenWorkspacePath: (relPath: string) => void | Promise<void>;
  onRevealWorkspacePath?: (relPath: string) => void;
}) {
  const streaming = isTurnStreaming && block.streaming !== false;
  if (!block.content.trim() && !streaming) {
    return null;
  }

  return (
    <div className="break-words">
      {block.content.trim() ? (
        <ChatMarkdown
          content={block.content}
          variant="assistant"
          isStreaming={streaming}
          workspaceRoot={workspaceRoot}
          desktopHost={desktopHost}
          onOpenWorkspacePath={onOpenWorkspacePath}
          onRevealWorkspacePath={onRevealWorkspacePath}
        />
      ) : (
        <span className="whitespace-pre-wrap">{streaming ? '' : '...'}</span>
      )}
    </div>
  );
}
