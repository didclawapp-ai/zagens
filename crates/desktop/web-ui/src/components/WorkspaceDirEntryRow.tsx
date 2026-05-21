import type { ReactNode } from 'react';
import {
  formatBrowseEntrySize,
  isSensitiveEntryName,
  type BrowseEntry,
} from '../lib/workspaceBrowse';
import { IconAlert, IconPlus, WorkspaceEntryIcon } from './icons/FlatIcons';

export function WorkspaceDirEntryRow({
  ent,
  rel,
  depth,
  isPreviewed,
  leading,
  onPrimaryClick,
  onContextMenu,
  onAddToChat,
  sensitiveHint,
  addToChatTitle,
}: {
  ent: BrowseEntry;
  rel: string;
  depth: number;
  isPreviewed: boolean;
  /** Chevron or spacer before the file icon. */
  leading: ReactNode;
  onPrimaryClick: () => void;
  onContextMenu: (e: { preventDefault: () => void; clientX: number; clientY: number }) => void;
  onAddToChat?: () => void;
  sensitiveHint: string;
  addToChatTitle: string;
}) {
  const isDir = ent.kind === 'directory';
  const sensitive = isSensitiveEntryName(ent.name);
  const pad = Math.min(depth, 12) * 12;

  return (
    <div
      className={`group flex items-center rounded-md hover:bg-hover ${
        isPreviewed ? 'bg-accent-soft/60 ring-1 ring-accent/25' : ''
      }`}
      style={{ paddingLeft: pad }}
    >
      <button
        type="button"
        className="flex-1 min-w-0 text-left px-2 py-1.5 text-xs text-t-text flex items-center gap-2"
        onClick={onPrimaryClick}
        onContextMenu={onContextMenu}
      >
        {leading}
        <WorkspaceEntryIcon name={ent.name} isDir={isDir} />
        <span className={`truncate ${isDir ? 'font-medium' : ''}`}>{ent.name}</span>
        {sensitive && (
          <span title={sensitiveHint} className="shrink-0 text-amber-500/90">
            <IconAlert className="size-3" />
          </span>
        )}
        {!isDir && ent.size != null && (
          <span className="text-[10px] text-t-text-muted ml-auto shrink-0 tabular-nums">
            {formatBrowseEntrySize(ent.size)}
          </span>
        )}
      </button>
      {!isDir && onAddToChat && (
        <button
          type="button"
          className="shrink-0 p-1.5 mr-0.5 rounded text-t-text-muted opacity-0 group-hover:opacity-100 hover:text-accent hover:bg-hover transition-opacity"
          title={addToChatTitle}
          onClick={(e) => {
            e.stopPropagation();
            onAddToChat();
          }}
        >
          <IconPlus className="size-3.5" />
        </button>
      )}
    </div>
  );
}
