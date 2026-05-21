import { useCallback, useEffect, useRef, useState } from 'react';
import { browseComposerWorkspace, browseThreadWorkspace } from '../api/client';
import type { BrowseEntry } from './workspaceBrowse';

export function useWorkspaceDirCache(opts: {
  active: boolean;
  workspaceRoot: string;
  resumedThreadId: string | null;
  runtimeOk: boolean;
  refreshNonce: number;
}) {
  const { active, workspaceRoot, resumedThreadId, runtimeOk, refreshNonce } = opts;
  const [cache, setCache] = useState<Map<string, BrowseEntry[]>>(new Map());
  const [loadingPaths, setLoadingPaths] = useState<Set<string>>(new Set());
  const [browseWorkspace, setBrowseWorkspace] = useState<string | null>(null);
  const [rootLoading, setRootLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inflightRef = useRef<Map<string, Promise<BrowseEntry[]>>>(new Map());
  const cacheRef = useRef(cache);
  cacheRef.current = cache;

  const hasThread = Boolean(resumedThreadId?.length);
  const root = workspaceRoot.trim();

  const fetchDir = useCallback(
    async (relPath: string): Promise<BrowseEntry[]> => {
      const key = relPath.trim();
      const existing = inflightRef.current.get(key);
      if (existing) return existing;

      const promise = (async () => {
        if (hasThread) {
          return browseThreadWorkspace(resumedThreadId!, key || undefined);
        }
        return browseComposerWorkspace(root, key || undefined);
      })().then((res) => {
        setBrowseWorkspace(res.workspace);
        return res.entries ?? [];
      });

      inflightRef.current.set(key, promise);
      try {
        return await promise;
      } finally {
        inflightRef.current.delete(key);
      }
    },
    [hasThread, resumedThreadId, root],
  );

  const loadDir = useCallback(
    async (relPath: string, opts?: { force?: boolean }) => {
      const key = relPath.trim();
      if (!opts?.force && cacheRef.current.has(key)) {
        return cacheRef.current.get(key)!;
      }
      if (opts?.force) {
        setCache((prev) => {
          const next = new Map(prev);
          next.delete(key);
          return next;
        });
      }
      setLoadingPaths((prev) => new Set(prev).add(key));
      if (key === '') setRootLoading(true);
      setError(null);
      try {
        const entries = await fetchDir(key);
        setCache((prev) => {
          const next = new Map(prev);
          next.set(key, entries);
          return next;
        });
        return entries;
      } catch (e) {
        const err = e as Error & { status?: number };
        const msg = err.message ?? String(e);
        setError(msg);
        throw e;
      } finally {
        setLoadingPaths((prev) => {
          const next = new Set(prev);
          next.delete(key);
          return next;
        });
        if (key === '') setRootLoading(false);
      }
    },
    [fetchDir],
  );

  const ensureLoaded = useCallback(
    async (relPath: string) => {
      const key = relPath.trim();
      if (cacheRef.current.has(key)) return cacheRef.current.get(key)!;
      return loadDir(key);
    },
    [loadDir],
  );

  const clearCache = useCallback(() => {
    inflightRef.current.clear();
    setCache(new Map());
    setLoadingPaths(new Set());
    setError(null);
  }, []);

  useEffect(() => {
    clearCache();
    setBrowseWorkspace(null);
  }, [resumedThreadId, workspaceRoot, clearCache]);

  useEffect(() => {
    if (!active || !runtimeOk) return;
    if (!hasThread && root.length === 0) {
      clearCache();
      setRootLoading(false);
      return;
    }
    void loadDir('', { force: true });
    // eslint-disable-next-line react-hooks/exhaustive-deps -- loadDir stable via cacheRef
  }, [active, runtimeOk, hasThread, root, refreshNonce]);

  return {
    cache,
    loadingPaths,
    rootLoading,
    error,
    setError,
    browseWorkspace,
    loadDir,
    ensureLoaded,
    clearCache,
  };
}
