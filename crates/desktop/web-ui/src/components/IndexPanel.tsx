import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../i18n';
import {
  fetchSymbolIndexInfo,
  fetchSymbolIndexSearch,
  deleteSymbolIndex,
  type SymbolIndexInfo,
  type SymbolSearchHit,
} from '../api/client';
import { confirmDialog } from '../lib/confirmDialog';
import { toast } from '../lib/toast';

interface Props {
  workspace: string;
  onRebuild: () => void;
  rebuilding: boolean;
  /** Error from the last rebuild attempt (cleared on next rebuild). */
  rebuildError: string | null;
  /** Optional: reveal a workspace-relative file in the Files panel. */
  onRevealFile?: (relPath: string) => void;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  let i = 0;
  let size = bytes;
  while (size >= 1024 && i < units.length - 1) {
    size /= 1024;
    i++;
  }
  return `${size.toFixed(1)} ${units[i]}`;
}

export default function IndexPanel({
  workspace,
  onRebuild,
  rebuilding,
  rebuildError,
  onRevealFile,
}: Props) {
  const { t } = useT();
  const [info, setInfo] = useState<SymbolIndexInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [searchQuery, setSearchQuery] = useState('');
  const [searchHits, setSearchHits] = useState<SymbolSearchHit[]>([]);
  const [searchLoading, setSearchLoading] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const searchSeq = useRef(0);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await fetchSymbolIndexInfo(workspace);
      setInfo(data);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [workspace]);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    if (!rebuilding) {
      load();
    }
  }, [rebuilding, load]);

  useEffect(() => {
    const q = searchQuery.trim();
    if (q.length < 2) {
      setSearchHits([]);
      setSearchError(null);
      setSearchLoading(false);
      return;
    }
    const seq = ++searchSeq.current;
    setSearchLoading(true);
    setSearchError(null);
    const timer = window.setTimeout(() => {
      fetchSymbolIndexSearch(q, { limit: 20 })
        .then((res) => {
          if (searchSeq.current !== seq) return;
          setSearchHits(res.hits);
        })
        .catch((e) => {
          if (searchSeq.current !== seq) return;
          setSearchError(String(e));
          setSearchHits([]);
        })
        .finally(() => {
          if (searchSeq.current === seq) setSearchLoading(false);
        });
    }, 250);
    return () => window.clearTimeout(timer);
  }, [searchQuery]);

  const handleDelete = useCallback(async () => {
    if (!(await confirmDialog(t('indexPanel.deleteConfirm')))) return;
    setDeleting(true);
    try {
      await deleteSymbolIndex(workspace);
      await load();
    } catch (e) {
      toast.error(`${t('indexPanel.deleteFailed')}: ${e}`);
    } finally {
      setDeleting(false);
    }
  }, [workspace, load, t]);

  const handleOpenDir = useCallback(() => {
    if (!info) return;
    import('@tauri-apps/api/core').then(({ invoke }) => {
      invoke('open_in_shell', { path: info.dir }).catch(() => {});
    });
  }, [info]);

  const labelCls = 'text-[11px] font-medium text-t-text-secondary';
  const valCls = 'text-xs text-t-text';
  const btnCls = 'py-1.5 px-4 rounded-lg text-xs font-medium transition-colors disabled:opacity-50';
  const btnPrimary = `${btnCls} bg-accent text-white hover:opacity-90`;
  const btnDanger = `${btnCls} border border-red-400/40 text-red-400 hover:bg-red-400/10`;
  const btnSecondary = `${btnCls} border border-divider text-t-text-secondary hover:bg-hover`;

  const statusLabel = (s: string) => {
    switch (s) {
      case 'fresh': return { text: t('indexPanel.statusFresh'), cls: 'text-emerald-600' };
      case 'stale': return { text: t('indexPanel.statusStale'), cls: 'text-amber-600' };
      default: return { text: t('indexPanel.statusMissing'), cls: 'text-t-text-muted' };
    }
  };

  return (
    <div className="p-4 space-y-5 overflow-y-auto h-full">
      <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
        {t('indexPanel._section')}
      </p>

      {loading && (
        <p className="text-xs text-t-text-muted">{t('indexPanel.isLoading')}</p>
      )}

      {(error || rebuildError) && (
        <p className="text-xs text-red-400">{`${t('indexPanel.rebuildFailed')}: ${rebuildError || error}`}</p>
      )}

      {info && (
        <>
          <div className="flex justify-between items-center">
            <span className={labelCls}>{t('indexPanel.status')}</span>
            <span className={`text-xs font-medium ${statusLabel(info.status).cls}`}>
              {statusLabel(info.status).text}
            </span>
          </div>

          <section className="space-y-2.5 py-3 border-y border-divider">
            {[
              [t('indexPanel.schemaLabel'), `V${info.schema_version}`],
              [t('indexPanel.filesLabel'), info.file_count.toLocaleString()],
              [t('indexPanel.symbolsLabel'), info.symbol_count.toLocaleString()],
              [t('indexPanel.sizeLabel'), formatBytes(info.size_bytes)],
            ].map(([label, value]) => (
              <div key={label} className="flex justify-between gap-2">
                <span className={labelCls}>{label}</span>
                <span className={valCls}>{value}</span>
              </div>
            ))}
          </section>

          <section className="space-y-2">
            <p className={labelCls}>{t('indexPanel.searchLabel')}</p>
            <input
              type="search"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={t('indexPanel.searchPlaceholder')}
              className="w-full rounded-lg border border-divider bg-canvas px-2.5 py-1.5 text-xs text-t-text placeholder:text-t-text-muted focus:outline-none focus:ring-1 focus:ring-accent/40"
            />
            {searchLoading && (
              <p className="text-[10px] text-t-text-muted">{t('indexPanel.searchLoading')}</p>
            )}
            {searchError && (
              <p className="text-[10px] text-red-400">{searchError}</p>
            )}
            {!searchLoading && searchQuery.trim().length >= 2 && searchHits.length === 0 && !searchError && (
              <p className="text-[10px] text-t-text-muted">{t('indexPanel.searchEmpty')}</p>
            )}
            {searchHits.length > 0 && (
              <ul className="max-h-48 overflow-y-auto rounded-lg border border-divider divide-y divide-divider">
                {searchHits.map((hit) => (
                  <li key={`${hit.file}:${hit.line}:${hit.name}`}>
                    <button
                      type="button"
                      className="w-full text-left px-2 py-1.5 hover:bg-hover transition-colors"
                      onClick={() => onRevealFile?.(hit.file.replace(/\\/g, '/'))}
                    >
                      <span className="block text-xs text-t-text truncate">
                        {hit.name}
                        <span className="ml-1.5 text-[10px] text-t-text-muted font-normal">
                          {hit.kind}
                        </span>
                      </span>
                      <span className="block text-[10px] text-t-text-muted font-mono truncate">
                        {hit.file}:{hit.line}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className="space-y-2">
            <p className={labelCls}>{t('indexPanel.dirLabel')}</p>
            <p className="text-[10px] text-t-text-muted leading-relaxed break-all font-mono bg-canvas rounded-lg px-2 py-1.5 border border-divider">
              {info.dir}
            </p>
          </section>

          <div className="flex flex-wrap gap-2 pt-2">
            <button
              type="button"
              className={btnPrimary}
              disabled={rebuilding}
              onClick={onRebuild}
            >
              {rebuilding ? t('indexPanel.rebuilding') : t('indexPanel.rebuild')}
            </button>
            <button
              type="button"
              className={btnSecondary}
              onClick={handleOpenDir}
            >
              {t('indexPanel.openDir')}
            </button>
            <button
              type="button"
              className={btnDanger}
              disabled={deleting || rebuilding}
              onClick={handleDelete}
            >
              {deleting ? t('indexPanel.deleting') : t('indexPanel.delete')}
            </button>
          </div>
        </>
      )}
    </div>
  );
}
