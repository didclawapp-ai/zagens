import { useCallback, useEffect, useState } from 'react';
import { forceSidecarRestartNow } from '../api/client';
import { confirmDialog } from '../lib/confirmDialog';
import { subscribeCurrentWebviewEvent } from '../lib/tauriListen';
import { useT } from '../i18n';

type PendingState = {
  activeCount: number;
};

export default function SidecarRestartPendingBanner() {
  const { t } = useT();
  const [pending, setPending] = useState<PendingState | null>(null);
  const [forcing, setForcing] = useState(false);

  useEffect(() => {
    const unlistenPending = subscribeCurrentWebviewEvent<{ active_count?: number }>(
      'sidecar://restart-pending',
      (payload) => {
        const count =
          typeof payload?.active_count === 'number' && payload.active_count > 0
            ? payload.active_count
            : 1;
        setPending({ activeCount: count });
      },
    );
    const unlistenCleared = subscribeCurrentWebviewEvent(
      'sidecar://restart-pending-cleared',
      () => {
        setPending(null);
        setForcing(false);
      },
    );
    const unlistenRestart = subscribeCurrentWebviewEvent('sidecar://restarting', () => {
      setPending(null);
      setForcing(false);
    });
    return () => {
      unlistenPending();
      unlistenCleared();
      unlistenRestart();
    };
  }, []);

  const handleForceRestart = useCallback(async () => {
    if (forcing) return;
    const count = pending?.activeCount ?? 1;
    if (
      !(await confirmDialog(
        t('settings.sidecarRestartForceConfirm', { count: String(count) }),
      ))
    ) {
      return;
    }
    setForcing(true);
    try {
      await forceSidecarRestartNow();
    } catch {
      setForcing(false);
    }
  }, [forcing, pending?.activeCount, t]);

  if (!pending) return null;

  return (
    <div
      role="alert"
      className="border-b border-amber-500/30 bg-amber-950/70 px-4 py-2.5 text-sm text-amber-50 flex flex-wrap items-center justify-between gap-3"
    >
      <p className="text-[13px] leading-snug">
        {t('settings.sidecarRestartPending', { count: String(pending.activeCount) })}
      </p>
      <button
        type="button"
        disabled={forcing}
        onClick={() => void handleForceRestart()}
        className="shrink-0 rounded-md border border-amber-200/40 bg-amber-900/60 px-3 py-1 text-[12px] font-medium hover:bg-amber-900 disabled:opacity-50"
      >
        {forcing ? t('settings.restarting') : t('settings.sidecarRestartNow')}
      </button>
    </div>
  );
}
