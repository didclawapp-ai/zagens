import {
  isSensitiveEntryName,
  parentWorkspaceRel,
  workspaceRelPathsEqual,
} from '../lib/workspaceBrowse';
import type { WorkspaceSearchHit } from '../lib/workspaceFileSearch';
import { WorkspaceEntryIcon, IconAlert } from './icons/FlatIcons';

export function WorkspaceSearchHitRow({
  hit,
  isPreviewed,
  onPrimaryClick,
  onContextMenu,
  onAddToChat,
  sensitiveHint,
  addToChatTitle,
}: {
  hit: WorkspaceSearchHit;
  isPreviewed: boolean;
  onPrimaryClick: () => void;
  onContextMenu: (e: { preventDefault: () => void; clientX: number; clientY: number }) => void;
  onAddToChat?: () => void;
  sensitiveHint: string;
  addToChatTitle: string;
}) {
  const isDir = hit.kind === 'directory';
  const parent = parentWorkspaceRel(hit.rel);
  const sensitive = isSensitiveEntryName(hit.name);

  return (
    <div
      data-ws-reveal={hit.rel}
      className={`group flex items-center rounded-md hover:bg-hover ${
        isPreviewed ? 'bg-accent-soft/60 ring-1 ring-accent/25' : ''
      }`}
    >
      <button
        type="button"
        className="flex-1 min-w-0 text-left px-2 py-1.5 text-xs flex items-center gap-2"
        onClick={onPrimaryClick}
        onContextMenu={onContextMenu}
      >
        <WorkspaceEntryIcon name={hit.name} isDir={isDir} />
        <span className="min-w-0 flex-1">
          <span className={`block truncate ${isDir ? 'font-medium text-t-text' : 'text-t-text'}`}>
            {hit.name}
          </span>
          {parent ? (
            <span className="block truncate text-[10px] text-t-text-muted font-mono">{parent}</span>
          ) : null}
        </span>
        {sensitive && (
          <span title={sensitiveHint} className="shrink-0 text-amber-500/90">
            <IconAlert className="size-3" />
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
          <span className="text-sm leading-none font-medium">+</span>
        </button>
      )}
    </div>
  );
}

export function isSearchHitPreviewed(
  hit: WorkspaceSearchHit,
  previewRel: string | null,
): boolean {
  return (
    hit.kind === 'file' &&
    previewRel != null &&
    workspaceRelPathsEqual(hit.rel, previewRel)
  );
}
