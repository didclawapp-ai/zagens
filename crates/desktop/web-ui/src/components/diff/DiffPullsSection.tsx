import { useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { getWorkspacePulls, type WorkspacePullEntry } from '../../api/client';
import { openExternalUrl } from '../../lib/openExternalUrl';

const COVERAGE_GATE_DOCS =
  'https://github.com/didclawapp-ai/zagens/blob/main/docs/desktop/GITHUB_ACTION.md';

interface Props {
  workspaceRoot: string;
  active: boolean;
  refreshNonce?: number;
}

/** Collapsed by default; fetches `gh pr list` only when expanded (avoids startup/sidecar stalls). */
export default function DiffPullsSection({
  workspaceRoot,
  active,
  refreshNonce = 0,
}: Props) {
  const { t } = useT();
  const [open, setOpen] = useState(false);
  const [pulls, setPulls] = useState<WorkspacePullEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const [selected, setSelected] = useState<WorkspacePullEntry | null>(null);
  const [loading, setLoading] = useState(false);
  const [loadedOnce, setLoadedOnce] = useState(false);

  useEffect(() => {
    // Lazy: do not call gh until the user expands PRs.
    if (!active || !open || !workspaceRoot.trim()) return;
    let cancelled = false;
    setLoading(true);
    void getWorkspacePulls(workspaceRoot, 'open')
      .then((res) => {
        if (cancelled) return;
        setPulls(res.pulls ?? []);
        setErrorCode(res.error ?? null);
        setError(res.error_message ?? null);
        setLoadedOnce(true);
        setSelected((prev) => {
          if (!prev) return null;
          return res.pulls?.find((p) => p.number === prev.number) ?? null;
        });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setPulls([]);
        setErrorCode('gh_failed');
        setError(err instanceof Error ? err.message : String(err));
        setLoadedOnce(true);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [active, open, workspaceRoot, refreshNonce]);

  const count = pulls.length;
  const label = !loadedOnce
    ? ''
    : errorCode === 'gh_missing'
      ? t('diff.prsGhMissing')
      : errorCode === 'gh_auth'
        ? t('diff.prsGhAuth')
        : t('diff.prsToggle', { n: String(count) });

  return (
    <div className="shrink-0 border-b border-divider bg-canvas">
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[10px] text-t-text-secondary hover:bg-hover transition-colors"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        <span className="font-medium uppercase tracking-wide text-t-text-muted">
          {t('diff.prsHeading')}
        </span>
        {label ? <span className="font-mono opacity-80">{label}</span> : null}
        <span className="ml-auto opacity-60">{open ? '▾' : '▸'}</span>
      </button>

      {open ? (
        <div className="px-2 pb-2 space-y-1.5">
          {loading ? (
            <p className="px-1 text-[11px] text-t-text-muted">{t('diff.prsLoading')}</p>
          ) : null}

          {errorCode && !loading ? (
            <div className="rounded-md border border-divider px-2 py-1.5 text-[11px] text-t-text-muted space-y-1">
              <p>{error ?? label}</p>
              {(errorCode === 'gh_missing' || errorCode === 'gh_auth') && (
                <p className="opacity-80">{t('diff.prsGhHint')}</p>
              )}
              <button
                type="button"
                className="text-accent hover:underline"
                onClick={() => void openExternalUrl(COVERAGE_GATE_DOCS)}
              >
                {t('diff.prsCoverageGateDocs')}
              </button>
            </div>
          ) : null}

          {!loading && !errorCode && loadedOnce && count === 0 ? (
            <p className="px-1 text-[11px] text-t-text-muted">{t('diff.prsEmpty')}</p>
          ) : null}

          <ul className="max-h-36 overflow-y-auto space-y-0.5" role="listbox">
            {pulls.map((p) => {
              const isSel = selected?.number === p.number;
              return (
                <li key={p.number}>
                  <button
                    type="button"
                    role="option"
                    aria-selected={isSel}
                    className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] transition-colors ${
                      isSel
                        ? 'bg-accent-soft text-accent'
                        : 'text-t-text-secondary hover:bg-hover'
                    }`}
                    onClick={() => setSelected(p)}
                  >
                    <ChecksDot checks={p.checks} />
                    <span className="shrink-0 font-mono opacity-70">#{p.number}</span>
                    <span className="min-w-0 flex-1 truncate">{p.title}</span>
                    {p.is_draft ? (
                      <span className="shrink-0 rounded px-1 text-[9px] uppercase bg-hover text-t-text-muted">
                        {t('diff.prsDraft')}
                      </span>
                    ) : null}
                  </button>
                </li>
              );
            })}
          </ul>

          {selected ? (
            <div className="rounded-md border border-divider px-2 py-1.5 text-[11px] space-y-1">
              <p className="font-mono text-t-text-muted">
                {selected.head_ref_name} ← {selected.base_ref_name}
              </p>
              <p className="text-t-text-secondary">
                {t('diff.prsChecks')}: <ChecksLabel checks={selected.checks} />
              </p>
              <div className="flex flex-wrap gap-2 pt-0.5">
                <button
                  type="button"
                  className="rounded-md border border-divider px-2 py-1 text-[10px] text-accent hover:bg-hover"
                  onClick={() => void openExternalUrl(selected.url)}
                  disabled={!selected.url}
                >
                  {t('diff.prsOpenBrowser')}
                </button>
              </div>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function ChecksDot({ checks }: { checks: string }) {
  const color =
    checks === 'success'
      ? 'bg-emerald-500'
      : checks === 'failure'
        ? 'bg-red-500'
        : checks === 'pending'
          ? 'bg-amber-400'
          : 'bg-t-text-muted/40';
  return (
    <span className={`inline-block size-1.5 shrink-0 rounded-full ${color}`} aria-hidden />
  );
}

function ChecksLabel({ checks }: { checks: string }) {
  const { t } = useT();
  switch (checks) {
    case 'success':
      return <>{t('diff.prsCheckSuccess')}</>;
    case 'pending':
      return <>{t('diff.prsCheckPending')}</>;
    case 'failure':
      return <>{t('diff.prsCheckFailure')}</>;
    case 'neutral':
      return <>{t('diff.prsCheckNeutral')}</>;
    default:
      return <>{t('diff.prsCheckUnknown')}</>;
  }
}
