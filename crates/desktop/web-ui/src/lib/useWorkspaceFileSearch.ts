import { useCallback, useEffect, useRef, useState } from 'react';
import { browseComposerWorkspace, browseThreadWorkspace } from '../api/client';
import type { BrowseEntry } from './workspaceBrowse';
import {
  searchWorkspaceFiles,
  type WorkspaceSearchHit,
  type WorkspaceSearchProgress,
} from './workspaceFileSearch';

export function useWorkspaceFileSearch(opts: {
  active: boolean;
  canBrowse: boolean;
  workspaceRoot: string;
  resumedThreadId: string | null;
  query: string;
  showHidden: boolean;
  refreshNonce: number;
}) {
  const { active, canBrowse, workspaceRoot, resumedThreadId, query, showHidden, refreshNonce } =
    opts;

  const [hits, setHits] = useState<WorkspaceSearchHit[]>([]);
  const [searching, setSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [progress, setProgress] = useState<WorkspaceSearchProgress | null>(null);
  const [truncated, setTruncated] = useState(false);
  const abortRef = useRef<AbortController | null>(null);

  const trimmedQuery = query.trim();
  const isSearchMode = trimmedQuery.length > 0;

  const hasThread = Boolean(resumedThreadId?.length);
  const root = workspaceRoot.trim();

  const fetchDir = useCallback(
    async (dirRel: string): Promise<BrowseEntry[]> => {
      const res = hasThread
        ? await browseThreadWorkspace(resumedThreadId!, dirRel || undefined)
        : await browseComposerWorkspace(root, dirRel || undefined);
      return res.entries ?? [];
    },
    [hasThread, resumedThreadId, root],
  );

  useEffect(() => {
    abortRef.current?.abort();
    abortRef.current = null;

    if (!active || !canBrowse || !isSearchMode) {
      setHits([]);
      setSearching(false);
      setSearchError(null);
      setProgress(null);
      setTruncated(false);
      return;
    }

    const ac = new AbortController();
    abortRef.current = ac;
    setSearching(true);
    setSearchError(null);
    setProgress({ scannedDirs: 0, hitCount: 0, truncated: false });
    setTruncated(false);

    void searchWorkspaceFiles({
      query: trimmedQuery,
      showHidden,
      fetchDir,
      signal: ac.signal,
      onProgress: setProgress,
    })
      .then((res) => {
        if (ac.signal.aborted) return;
        setHits(res.hits);
        setTruncated(res.truncated);
      })
      .catch((e) => {
        if (ac.signal.aborted || (e instanceof DOMException && e.name === 'AbortError')) {
          return;
        }
        const err = e as Error;
        setSearchError(err.message ?? String(e));
        setHits([]);
      })
      .finally(() => {
        if (!ac.signal.aborted) {
          setSearching(false);
        }
      });

    return () => {
      ac.abort();
    };
  }, [
    active,
    canBrowse,
    isSearchMode,
    trimmedQuery,
    showHidden,
    fetchDir,
    refreshNonce,
  ]);

  return {
    hits,
    searching,
    searchError,
    progress,
    truncated,
    isSearchMode,
  };
}
