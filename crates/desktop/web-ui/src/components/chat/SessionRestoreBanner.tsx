import type { SessionRestoreSource } from '../../hooks/useSessionNavigation';
import { useT } from '../../i18n';

export function SessionRestoreBanner({
  loading,
  source,
  onRetry,
}: {
  loading: boolean;
  source: SessionRestoreSource;
  onRetry?: () => void;
}) {
  const { t } = useT();

  if (loading) {
    return (
      <div className="session-restore-banner session-restore-banner--loading" role="status">
        {t('chatRestore.loading')}
      </div>
    );
  }

  if (!source) {
    return null;
  }

  const sourceLabel =
    source === 'cache'
      ? t('chatRestore.sourceCache')
      : source === 'thread'
        ? t('chatRestore.sourceThread')
        : t('chatRestore.sourceSession');

  const degraded = source === 'session';

  return (
    <div
      className={`session-restore-banner ${degraded ? 'session-restore-banner--warn' : 'session-restore-banner--ok'}`}
      role="status"
    >
      <span>{t('chatRestore.sourceLabel', { source: sourceLabel })}</span>
      {degraded ? (
        <>
          <span className="session-restore-banner-hint">{t('chatRestore.toolsMayBeIncomplete')}</span>
          {onRetry ? (
            <button type="button" className="session-restore-banner-action" onClick={() => void onRetry()}>
              {t('chatRestore.retry')}
            </button>
          ) : null}
        </>
      ) : null}
    </div>
  );
}
