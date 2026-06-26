import { useT } from '../i18n';
import type { RuntimeConnectionState } from '../api/client';

type StartupConnectOverlayProps = {
  runtimeConn: RuntimeConnectionState;
  onRetry: () => void;
};

/** Full-screen splash while the local runtime sidecar is still starting or reconnecting. */
export default function StartupConnectOverlay({
  runtimeConn,
  onRetry,
}: StartupConnectOverlayProps) {
  const { t } = useT();
  const showRetry = runtimeConn === 'offline' || runtimeConn === 'auth_mismatch';
  const statusText =
    runtimeConn === 'auth_mismatch'
      ? t('onboarding.connectAuthMismatch')
      : runtimeConn === 'offline'
        ? t('onboarding.connectOffline')
        : t('onboarding.startingWait');

  return (
    <div
      className="fixed inset-0 z-[200] flex flex-col items-center justify-center gap-4 bg-canvas px-6 text-center"
      role="status"
      aria-live="polite"
      aria-busy={runtimeConn === 'checking'}
    >
      {runtimeConn === 'checking' ? (
        <span
          className="inline-block size-5 animate-spin rounded-full border-2 border-divider border-t-accent"
          aria-hidden
        />
      ) : null}
      <p className="max-w-sm text-sm text-t-text-muted leading-relaxed">{statusText}</p>
      {showRetry ? (
        <button
          type="button"
          className="rounded-lg bg-accent px-4 py-2 text-sm font-medium text-accent-text hover:bg-accent-hover transition-colors"
          onClick={onRetry}
        >
          {t('common.retryConnection')}
        </button>
      ) : null}
    </div>
  );
}
