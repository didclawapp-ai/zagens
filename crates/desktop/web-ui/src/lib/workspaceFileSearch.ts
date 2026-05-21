import {
  joinWorkspaceRel,
  isDeniedDirName,
  type BrowseEntry,
} from './workspaceBrowse';

export const WORKSPACE_SEARCH_MAX_RESULTS = 200;
export const WORKSPACE_SEARCH_MAX_DIRS = 1200;
export const WORKSPACE_SEARCH_CONCURRENCY = 6;
/** Flat list / search results use virtual scroll at or above this count. */
export const WORKSPACE_LIST_VIRTUAL_MIN = 48;
export const WORKSPACE_LIST_ROW_PX = 34;

export interface WorkspaceSearchHit {
  rel: string;
  name: string;
  kind: 'file' | 'directory';
}

export function pathMatchesWorkspaceSearch(rel: string, name: string, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  const relLower = rel.toLowerCase();
  const nameLower = name.toLowerCase();
  const tokens = q.split(/\s+/).filter(Boolean);
  return tokens.every((t) => relLower.includes(t) || nameLower.includes(t));
}

export type WorkspaceSearchProgress = {
  scannedDirs: number;
  hitCount: number;
  truncated: boolean;
};

/**
 * Breadth-first workspace walk via browse API; skips denylisted dirs unless `showHidden`.
 */
export async function searchWorkspaceFiles(options: {
  query: string;
  showHidden: boolean;
  fetchDir: (dirRel: string) => Promise<BrowseEntry[]>;
  signal?: AbortSignal;
  onProgress?: (p: WorkspaceSearchProgress) => void;
}): Promise<{ hits: WorkspaceSearchHit[]; truncated: boolean; scannedDirs: number }> {
  const query = options.query.trim();
  if (!query) {
    return { hits: [], truncated: false, scannedDirs: 0 };
  }

  const hits: WorkspaceSearchHit[] = [];
  const queue: string[] = [''];
  const visited = new Set<string>(['']);
  let scannedDirs = 0;
  let truncated = false;

  const report = () => {
    options.onProgress?.({
      scannedDirs,
      hitCount: hits.length,
      truncated,
    });
  };

  while (queue.length > 0 && hits.length < WORKSPACE_SEARCH_MAX_RESULTS) {
    if (options.signal?.aborted) {
      break;
    }
    if (scannedDirs >= WORKSPACE_SEARCH_MAX_DIRS) {
      truncated = true;
      break;
    }

    const batch: string[] = [];
    while (batch.length < WORKSPACE_SEARCH_CONCURRENCY && queue.length > 0) {
      batch.push(queue.shift()!);
    }

    const batchResults = await Promise.all(
      batch.map(async (dirRel) => {
        if (options.signal?.aborted) {
          return [] as WorkspaceSearchHit[];
        }
        const entries = await options.fetchDir(dirRel);
        const local: WorkspaceSearchHit[] = [];
        for (const ent of entries) {
          const rel = joinWorkspaceRel(dirRel, ent.name);
          const isDir = ent.kind === 'directory';
          if (pathMatchesWorkspaceSearch(rel, ent.name, query)) {
            local.push({
              rel,
              name: ent.name,
              kind: isDir ? 'directory' : 'file',
            });
          }
          if (isDir) {
            const skip = !options.showHidden && isDeniedDirName(ent.name, false);
            if (!skip && !visited.has(rel)) {
              visited.add(rel);
              queue.push(rel);
            }
          }
        }
        return local;
      }),
    );

    for (const dirRel of batch) {
      scannedDirs += 1;
      if (scannedDirs >= WORKSPACE_SEARCH_MAX_DIRS) {
        truncated = true;
      }
    }

    for (const local of batchResults) {
      for (const h of local) {
        if (hits.length >= WORKSPACE_SEARCH_MAX_RESULTS) {
          truncated = true;
          break;
        }
        hits.push(h);
      }
    }
    report();
    if (truncated) break;
  }

  hits.sort((a, b) => a.rel.localeCompare(b.rel));
  return { hits, truncated, scannedDirs };
}
