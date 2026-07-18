import { useEffect, useRef, useState } from 'react';
import { getWorkspaceStatus, type WorkspaceStatusResponse } from '../../api/client';
import { useT } from '../../i18n';

/** Idle poll after first successful probe; skip entirely for non-git workspaces. */
const POLL_MS = 20_000;
const FIRST_PROBE_DELAY_MS = 2_500;

interface Props {
  workspaceRoot: string;
  /** Open Diff workspace tab. */
  onOpenDiff?: () => void;
  disabled?: boolean;
  /** When turn ends (streaming false), refresh counts immediately. */
  streaming?: boolean;
}

/** Light branch + dirty-count chip; click focuses Diff (not a SCM panel). */
export default function ComposerGitBadge({
  workspaceRoot,
  onOpenDiff,
  disabled = false,
  streaming = false,
}: Props) {
  const { t } = useT();
  const [status, setStatus] = useState<WorkspaceStatusResponse | null>(null);
  const wasStreamingRef = useRef(streaming);

  useEffect(() => {
    const root = workspaceRoot.trim();
    if (!root) {
      setStatus(null);
      return;
    }
    let cancelled = false;
    let intervalId: number | undefined;

    const tick = async () => {
      try {
        const s = await getWorkspaceStatus(root);
        if (cancelled) return;
        setStatus(s);
        if (!s.git_repo && intervalId != null) {
          window.clearInterval(intervalId);
          intervalId = undefined;
        }
      } catch {
        if (!cancelled) setStatus(null);
      }
    };

    const delayId = window.setTimeout(() => {
      void tick();
      intervalId = window.setInterval(() => {
        if (document.visibilityState === 'visible') void tick();
      }, POLL_MS);
    }, FIRST_PROBE_DELAY_MS);

    const onVis = () => {
      if (document.visibilityState === 'visible') void tick();
    };
    document.addEventListener('visibilitychange', onVis);

    return () => {
      cancelled = true;
      window.clearTimeout(delayId);
      if (intervalId != null) window.clearInterval(intervalId);
      document.removeEventListener('visibilitychange', onVis);
    };
  }, [workspaceRoot]);

  useEffect(() => {
    if (wasStreamingRef.current && !streaming && workspaceRoot.trim()) {
      void getWorkspaceStatus(workspaceRoot.trim())
        .then(setStatus)
        .catch(() => setStatus(null));
    }
    wasStreamingRef.current = streaming;
  }, [streaming, workspaceRoot]);

  if (!status?.git_repo) return null;

  const dirty = status.staged + status.unstaged + status.untracked;
  const branch = status.branch?.trim() || 'HEAD';
  const label =
    dirty > 0
      ? t('composer.gitBadgeDirty', { branch, n: String(dirty) })
      : t('composer.gitBadgeClean', { branch });

  return (
    <button
      type="button"
      className="composer-chip shrink-0 max-w-[12rem]"
      disabled={disabled || !onOpenDiff}
      title={t('composer.gitBadgeTitle')}
      onClick={() => onOpenDiff?.()}
    >
      <GitBranchIcon />
      <span className="truncate font-mono text-[11px]">{label}</span>
    </button>
  );
}

function GitBranchIcon() {
  return (
    <svg viewBox="0 0 24 24" className="size-3.5 shrink-0" fill="none" stroke="currentColor" strokeWidth="2">
      <path d="M6 3v12" />
      <circle cx="6" cy="18" r="3" />
      <circle cx="18" cy="6" r="3" />
      <path d="M18 9a9 9 0 01-9 9" />
    </svg>
  );
}
