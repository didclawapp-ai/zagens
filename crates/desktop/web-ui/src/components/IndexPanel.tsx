import { useCallback, useEffect, useState } from 'react';
import { useT } from '../i18n';
import {
  fetchSymbolIndexInfo,
  deleteSymbolIndex,
  type SymbolIndexInfo,
} from '../api/client';
import { confirmDialog } from '../lib/confirmDialog';

interface Props {
  workspace: string;
  onRebuild: () => void;
  rebuilding: boolean;
  /** Error from the last rebuild attempt (cleared on next rebuild). */
  rebuildError: string | null;
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

export default function IndexPanel({ workspace, onRebuild, rebuilding, rebuildError }: Props) {
  const { t } = useT();
  const [info, setInfo] = useState<SymbolIndexInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [deleting, setDeleting] = useState(false);

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

  const handleDelete = useCallback(async () => {
    if (!(await confirmDialog(t('indexPanel.deleteConfirm')))) return;
    setDeleting(true);
    try {
      await deleteSymbolIndex(workspace);
      await load();
    } catch (e) {
      alert(`${t('indexPanel.deleteFailed')}: ${e}`);
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
