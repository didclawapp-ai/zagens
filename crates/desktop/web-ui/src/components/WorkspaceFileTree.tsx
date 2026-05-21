import { useCallback } from 'react';
import { useT } from '../i18n';
import {
  filterBrowseEntries,
  joinWorkspaceRel,
  workspaceRelPathsEqual,
  type BrowseEntry,
} from '../lib/workspaceBrowse';
import { IconChevronRight } from './icons/FlatIcons';
import { WorkspaceDirEntryRow } from './WorkspaceDirEntryRow';

export interface WorkspaceFileTreeProps {
  cache: Map<string, BrowseEntry[]>;
  loadingPaths: Set<string>;
  expanded: Set<string>;
  onToggleExpanded: (dirRel: string) => void;
  showHidden: boolean;
  searchQuery: string;
  previewRel: string | null;
  ensureLoaded: (dirRel: string) => Promise<BrowseEntry[]>;
  onOpenFile: (relPath: string, title: string) => void;
  onOpenContextMenu: (
    e: { preventDefault: () => void; clientX: number; clientY: number },
    ent: BrowseEntry,
    rel: string,
  ) => void;
}

function TreeDir({
  dirRel,
  depth,
  props,
}: {
  dirRel: string;
  depth: number;
  props: WorkspaceFileTreeProps;
}) {
  const { cache, loadingPaths, expanded, showHidden, searchQuery } = props;
  const entries = cache.get(dirRel) ?? [];
  const visible = filterBrowseEntries(entries, searchQuery, showHidden);
  const isLoading = loadingPaths.has(dirRel);

  if (!expanded.has(dirRel)) return null;

  return (
    <ul className="space-y-0.5" role="group">
      {isLoading && visible.length === 0 && (
        <li
          className="text-[10px] text-t-text-muted py-0.5"
          style={{ paddingLeft: depth * 12 + 24 }}
        >
          …
        </li>
      )}
      {visible.map((ent) => (
        <TreeEntry
          key={joinWorkspaceRel(dirRel, ent.name)}
          ent={ent}
          parentRel={dirRel}
          depth={depth}
          props={props}
        />
      ))}
    </ul>
  );
}

function TreeEntry({
  ent,
  parentRel,
  depth,
  props,
}: {
  ent: BrowseEntry;
  parentRel: string;
  depth: number;
  props: WorkspaceFileTreeProps;
}) {
  const {
    expanded,
    onToggleExpanded,
    previewRel,
    ensureLoaded,
    onOpenFile,
    onOpenContextMenu,
    loadingPaths,
  } = props;

  const rel = joinWorkspaceRel(parentRel, ent.name);
  const isDir = ent.kind === 'directory';
  const isExpanded = expanded.has(rel);
  const isLoading = loadingPaths.has(rel);
  const isPreviewed =
    !isDir && previewRel != null && workspaceRelPathsEqual(rel, previewRel);
  const { t } = useT();

  const handleToggle = useCallback(() => {
    const next = !isExpanded;
    onToggleExpanded(rel);
    if (next) {
      void ensureLoaded(rel);
    }
  }, [isExpanded, onToggleExpanded, rel, ensureLoaded]);

  const leading = isDir ? (
    <button
      type="button"
      className="shrink-0 p-0.5 rounded hover:bg-hover text-t-text-muted"
      aria-expanded={isExpanded}
      onClick={(e) => {
        e.stopPropagation();
        handleToggle();
      }}
    >
      <IconChevronRight
        className={`size-3 transition-transform duration-150 ${isExpanded ? 'rotate-90' : ''} ${isLoading ? 'opacity-40' : ''}`}
      />
    </button>
  ) : (
    <span className="size-4 shrink-0" />
  );

  return (
    <li role="treeitem" aria-expanded={isDir ? isExpanded : undefined}>
      <WorkspaceDirEntryRow
        ent={ent}
        rel={rel}
        depth={depth}
        isPreviewed={isPreviewed}
        leading={leading}
        sensitiveHint={t('workspaceFiles.sensitiveHint')}
        addToChatTitle={t('workspaceFiles.addToChat')}
        onPrimaryClick={() => {
          if (isDir) {
            handleToggle();
          } else {
            void onOpenFile(rel, ent.name);
          }
        }}
        onContextMenu={(e) => onOpenContextMenu(e, ent, rel)}
        onAddToChat={!isDir ? () => void onOpenFile(rel, ent.name) : undefined}
      />
      {isDir && <TreeDir dirRel={rel} depth={depth + 1} props={props} />}
    </li>
  );
}

export default function WorkspaceFileTree(props: WorkspaceFileTreeProps) {
  const { t } = useT();
  const { cache, searchQuery, showHidden, loadingPaths } = props;

  const rootEntries = cache.get('') ?? [];
  const visibleRoot = filterBrowseEntries(rootEntries, searchQuery, showHidden);

  return (
    <ul className="space-y-0.5" role="tree" aria-label={t('workspaceFiles.treeAria')}>
      {loadingPaths.has('') && visibleRoot.length === 0 && (
        <li className="text-xs text-t-text-muted px-2">{t('workspaceFiles.loading')}</li>
      )}
      {visibleRoot.map((ent) => (
        <TreeEntry key={ent.name} ent={ent} parentRel="" depth={0} props={props} />
      ))}
      {visibleRoot.length === 0 && !loadingPaths.has('') && (
        <li className="text-[11px] text-t-text-muted px-2 py-1">
          {searchQuery.trim()
            ? t('workspaceFiles.noSearchMatch')
            : t('workspaceFiles.emptyDir')}
        </li>
      )}
    </ul>
  );
}
