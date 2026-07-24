import { useCallback, useEffect, useState } from 'react';
import { useT } from '../i18n';
import {
  fetchAppUpdateStatus,
  installAppUpdate,
  subscribeAppUpdateProgress,
  type AppUpdateStatus,
} from '../lib/appUpdate';
import { openExternalUrl } from '../lib/openExternalUrl';
import { UPDATE_DOWNLOAD_BASE } from '../lib/updateConfig';

const APP_VERSION = '0.8.9';
const SUPPORT_EMAIL = 'didclawapp@gmail.com';
const WEBSITE_URL = 'https://zagens.com/';
const DOWNLOAD_PAGE_URL = `${WEBSITE_URL}download`;

export default function AboutPanel() {
  const { t } = useT();
  const [version, setVersion] = useState('…');
  const [updateStatus, setUpdateStatus] = useState<AppUpdateStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [downloadPct, setDownloadPct] = useState<number | null>(null);

  const refreshUpdate = useCallback(async () => {
    setChecking(true);
    try {
      const status = await fetchAppUpdateStatus();
      setUpdateStatus(status);
    } catch (e) {
      setUpdateStatus({
        ready: false,
        currentVersion: version,
        status: 'error',
        downloadPageUrl: UPDATE_DOWNLOAD_BASE,
        error: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setChecking(false);
    }
  }, [version]);

  useEffect(() => {
    void (async () => {
      try {
        const { invoke } = await import('@tauri-apps/api/core');
        const info = await invoke<{ version: string }>('get_platform_info');
        setVersion(info.version);
      } catch {
        setVersion(APP_VERSION);
      }
    })();
  }, []);

  useEffect(() => {
    if (version === '…') return;
    void refreshUpdate();
  }, [version, refreshUpdate]);

  const handleInstall = async () => {
    setInstalling(true);
    setDownloadPct(null);
    const unsub = subscribeAppUpdateProgress((downloaded, total) => {
      if (total && total > 0) {
        setDownloadPct(Math.min(100, Math.round((downloaded / total) * 100)));
      }
    });
    try {
      await installAppUpdate();
    } catch (e) {
      setUpdateStatus((prev) =>
        prev
          ? {
              ...prev,
              status: 'error',
              error: e instanceof Error ? e.message : String(e),
            }
          : prev,
      );
      setInstalling(false);
      unsub();
    }
  };

  const statusLine = (() => {
    if (!updateStatus) return null;
    if (checking) return t('about.updateChecking');
    switch (updateStatus.status) {
      case 'available':
        return t('about.updateAvailable', {
          version: updateStatus.availableVersion ?? '?',
        });
      case 'up_to_date':
        return t('about.updateUpToDate');
      case 'error':
        return updateStatus.error ?? t('about.updateError');
      case 'not_configured':
        return t('about.updateNotConfigured');
      default:
        return null;
    }
  })();

  return (
    <div className="flex h-full flex-col overflow-y-auto p-4">
      <div className="flex items-center gap-3 pb-4">
        <img
          src="/app-icon.png"
          alt=""
          className="size-12 shrink-0 rounded-xl object-cover shadow-sm"
          width={48}
          height={48}
        />
        <div>
          <h3 className="text-base font-semibold text-t-text">{t('app.title')}</h3>
          <p className="text-xs text-t-text-muted">
            {t('app.subtitle')} · v{version}
          </p>
        </div>
      </div>
      <p className="text-sm leading-relaxed text-t-text-secondary">{t('about.description')}</p>

      <section className="mt-4 rounded-lg border border-card-border bg-card/50 p-3">
        <h4 className="text-xs font-medium text-t-text">{t('about.updateTitle')}</h4>
        {statusLine && (
          <p className="mt-2 text-xs leading-relaxed text-t-text-muted">{statusLine}</p>
        )}
        {updateStatus?.notes && updateStatus.status === 'available' && (
          <p className="mt-1 text-xs leading-relaxed text-t-text-secondary">{updateStatus.notes}</p>
        )}
        {installing && downloadPct !== null && (
          <p className="mt-1 text-xs text-t-text-muted">
            {t('about.updateDownloading', { pct: String(downloadPct) })}
          </p>
        )}
        <div className="mt-3 flex flex-wrap gap-2">
          <button
            type="button"
            className="rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-white hover:opacity-90 disabled:opacity-50"
            disabled={checking || installing}
            onClick={() => void refreshUpdate()}
          >
            {checking ? t('about.updateChecking') : t('about.updateCheck')}
          </button>
          {updateStatus?.status === 'available' && (
            <button
              type="button"
              className="rounded-md border border-accent/40 px-3 py-1.5 text-xs font-medium text-accent hover:bg-accent/10 disabled:opacity-50"
              disabled={installing || checking}
              onClick={() => void handleInstall()}
            >
              {installing ? t('about.updateInstalling') : t('about.updateInstall')}
            </button>
          )}
          <button
            type="button"
            className="rounded-md border border-card-border px-3 py-1.5 text-xs text-t-text-secondary hover:text-accent"
            onClick={() =>
              void openExternalUrl(updateStatus?.downloadPageUrl ?? DOWNLOAD_PAGE_URL)
            }
          >
            {t('about.updateManualDownload')}
          </button>
        </div>
      </section>

      <dl className="mt-4 space-y-2 text-sm">
        <div className="flex flex-wrap gap-x-2 gap-y-0.5">
          <dt className="text-t-text-muted">{t('about.emailLabel')}</dt>
          <dd>
            <button
              type="button"
              className="text-accent hover:underline"
              onClick={() => void openExternalUrl(`mailto:${SUPPORT_EMAIL}`)}
            >
              {SUPPORT_EMAIL}
            </button>
          </dd>
        </div>
        <div className="flex flex-wrap gap-x-2 gap-y-0.5">
          <dt className="text-t-text-muted">{t('about.websiteLabel')}</dt>
          <dd>
            <button
              type="button"
              className="text-accent hover:underline"
              onClick={() => void openExternalUrl(WEBSITE_URL)}
            >
              {WEBSITE_URL}
            </button>
          </dd>
        </div>
      </dl>
      <div className="mt-6">
        <h4 className="text-xs font-medium text-t-text">{t('about.thirdPartyTitle')}</h4>
        <p className="mt-2 text-xs leading-relaxed text-t-text-muted">{t('about.thirdPartyLicenses')}</p>
      </div>
      <div className="mt-6">
        <h4 className="text-xs font-medium text-t-text">{t('about.techStackTitle')}</h4>
        <ul className="mt-2 space-y-1 text-xs leading-relaxed text-t-text-muted">
          <li>{t('about.techStackDeepseekTui')}</li>
          <li>{t('about.techStackTauri')}</li>
          <li>{t('about.techStackReact')}</li>
        </ul>
      </div>
    </div>
  );
}
