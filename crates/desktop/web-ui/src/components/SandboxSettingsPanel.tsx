import { useCallback, useEffect, useMemo, useState } from 'react';
import { useT } from '../i18n';
import {
  fetchSandboxPlatformsOverview,
  fetchSandboxSettings,
  saveSandboxSettings,
  type SandboxPlatformsOverview,
  type SandboxSettings,
} from '../api/client';
import { confirmDialog } from '../lib/confirmDialog';

type PlatformTab = 'windows' | 'linux' | 'macos';

interface Props {
  desktopHost: boolean;
  platform: string;
  streaming?: boolean;
}

function hostDefaultTab(platform: string): PlatformTab {
  if (platform === 'windows') return 'windows';
  if (platform === 'linux') return 'linux';
  if (platform === 'darwin') return 'macos';
  return 'windows';
}

function statusBadge(enforced: boolean, available: boolean) {
  if (enforced) {
    return 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400 border-emerald-500/25';
  }
  if (available) {
    return 'bg-amber-500/15 text-amber-700 dark:text-amber-400 border-amber-500/25';
  }
  return 'bg-t-text-muted/10 text-t-text-muted border-divider';
}

export default function SandboxSettingsPanel({ desktopHost, platform, streaming = false }: Props) {
  const { t } = useT();
  const [settings, setSettings] = useState<SandboxSettings | null>(null);
  const [overview, setOverview] = useState<SandboxPlatformsOverview | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [tab, setTab] = useState<PlatformTab>(() => hostDefaultTab(platform));

  useEffect(() => {
    setTab(hostDefaultTab(platform));
  }, [platform]);

  useEffect(() => {
    if (!desktopHost) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    Promise.all([fetchSandboxSettings(), fetchSandboxPlatformsOverview()])
      .then(([s, o]) => {
        if (!cancelled) {
          setSettings(s);
          setOverview(o);
        }
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [desktopHost]);

  const update = useCallback(<K extends keyof SandboxSettings>(key: K, value: SandboxSettings[K]) => {
    setSettings((prev) => (prev ? { ...prev, [key]: value } : prev));
  }, []);

  const handleSave = useCallback(async () => {
    if (!settings || !desktopHost) return;
    if (streaming && !(await confirmDialog(t('settings.saveRestartsSidecar')))) {
      return;
    }
    setSaving(true);
    try {
      await saveSandboxSettings(settings);
      const o = await fetchSandboxPlatformsOverview();
      setOverview(o);
    } finally {
      setSaving(false);
    }
  }, [settings, desktopHost, streaming, t]);

  const selectCls =
    'w-full rounded-lg border border-divider bg-canvas px-3 py-2 text-xs text-t-text focus:outline-none focus:ring-1 focus:ring-accent';
  const labelCls = 'text-[11px] font-medium text-t-text-secondary';
  const descCls = 'text-[10px] text-t-text-muted';

  const tabItems = useMemo(
    (): { id: PlatformTab; label: string; host: boolean }[] => [
      { id: 'windows', label: t('sandboxSettings.tabWindows'), host: platform === 'windows' },
      { id: 'linux', label: t('sandboxSettings.tabLinux'), host: platform === 'linux' },
      { id: 'macos', label: t('sandboxSettings.tabMacos'), host: platform === 'darwin' },
    ],
    [platform, t],
  );

  const renderStatusLine = (p: PlatformTab) => {
    if (!overview) return null;
    const card = overview[p === 'macos' ? 'macos' : p];
    const enforced = card.enforced;
    const badgeCls = statusBadge(enforced, card.backend_available);
    const badgeLabel = enforced
      ? t('sandboxSettings.statusEnforced')
      : card.backend_available
        ? t('sandboxSettings.statusAvailable')
        : t('sandboxSettings.statusUnavailable');

    let detailKey: string | null = null;
    if (p === 'windows') {
      if (card.backend === 'elevated') detailKey = 'settings.sandboxElevatedEnforced';
      else if (card.backend === 'unelevated') detailKey = 'settings.sandboxUnelevatedEnforced';
      else if (platform === 'windows') detailKey = 'settings.sandboxSetupRequired';
    } else if (p === 'linux') {
      detailKey = card.enforced ? 'sandboxSettings.linuxEnforced' : 'sandboxSettings.linuxDegraded';
    } else {
      detailKey = card.enforced ? 'sandboxSettings.macosEnforced' : 'sandboxSettings.macosDegraded';
    }

    return (
      <div className="rounded-lg border border-divider bg-canvas/60 p-3 space-y-2">
        <div className="flex items-center justify-between gap-2">
          <span className="text-[11px] font-semibold text-t-text-secondary">{t('sandboxSettings.platformStatus')}</span>
          <span className={`text-[10px] px-2 py-0.5 rounded-full border ${badgeCls}`}>{badgeLabel}</span>
        </div>
        {detailKey && <p className={descCls}>{t(detailKey as any)}</p>}
        {p === 'windows' && card.setup_complete === false && platform === 'windows' && (
          <p className="text-[10px] text-amber-600">{t('settings.sandboxSetupRequired')}</p>
        )}
      </div>
    );
  };

  if (!desktopHost) {
    return (
      <div className="p-4">
        <p className="text-xs text-t-text-muted leading-relaxed">{t('sandboxSettings.notAvailable')}</p>
      </div>
    );
  }

  return (
    <div className="p-4 space-y-5 overflow-y-auto h-full">
      <div>
        <p className="text-xs text-t-text-muted leading-relaxed">{t('sandboxSettings.intro')}</p>
      </div>

      {loading && <p className="text-xs text-t-text-muted">{t('sandboxSettings.loading')}</p>}

      {settings && (
        <>
          <section className="space-y-3">
            <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
              {t('sandboxSettings.globalPolicy')}
            </p>
            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.sandboxMode')}</span>
              <p className={descCls}>{t('sandboxSettings.globalPolicyDesc')}</p>
              <select
                className={selectCls}
                value={settings.sandbox_mode}
                onChange={(e) => update('sandbox_mode', e.target.value)}
              >
                <option value="workspace-write">{t('settings.sandboxWorkspace')}</option>
                <option value="read-only">{t('settings.sandboxReadOnly')}</option>
                <option value="full-access">{t('settings.sandboxFullAccess')}</option>
              </select>
            </label>
          </section>

          <section className="space-y-3">
            <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
              {t('sandboxSettings.platformSection')}
            </p>

            <div className="flex border-b border-divider -mb-px" role="tablist" aria-label={t('sandboxSettings.platformSection')}>
              {tabItems.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  role="tab"
                  aria-selected={tab === item.id}
                  className={`flex-1 px-2 py-2 text-[11px] font-medium transition-colors border-b-2 -mb-px ${
                    tab === item.id
                      ? 'border-accent text-accent'
                      : 'border-transparent text-t-text-muted hover:text-t-text'
                  }`}
                  onClick={() => setTab(item.id)}
                >
                  {item.label}
                  {item.host && (
                    <span className="ml-1 text-[9px] text-accent/80">({t('sandboxSettings.thisHost')})</span>
                  )}
                </button>
              ))}
            </div>

            {tab === 'windows' && (
              <div className="space-y-3 pt-1">
                {renderStatusLine('windows')}
                <label className="block space-y-1">
                  <span className={labelCls}>{t('sandboxSettings.windowsMode')}</span>
                  <p className={descCls}>{t('sandboxSettings.windowsModeDesc')}</p>
                  <select
                    className={selectCls}
                    value={settings.windows_sandbox}
                    onChange={(e) => update('windows_sandbox', e.target.value)}
                  >
                    <option value="auto">{t('sandboxSettings.windowsModeAuto')}</option>
                    <option value="elevated">{t('sandboxSettings.windowsModeElevated')}</option>
                    <option value="unelevated">{t('sandboxSettings.windowsModeUnelevated')}</option>
                  </select>
                </label>
                <label className="flex items-center justify-between gap-2 py-1">
                  <div className="flex-1 min-w-0">
                    <span className={labelCls}>{t('sandboxSettings.privateDesktop')}</span>
                    <p className={descCls}>{t('sandboxSettings.privateDesktopDesc')}</p>
                  </div>
                  <input
                    type="checkbox"
                    className="shrink-0 w-4 h-4 accent-accent rounded"
                    checked={settings.windows_private_desktop}
                    onChange={(e) => update('windows_private_desktop', e.target.checked)}
                  />
                </label>
              </div>
            )}

            {tab === 'linux' && (
              <div className="space-y-3 pt-1">
                {renderStatusLine('linux')}
                <p className="text-xs text-t-text-muted leading-relaxed">{t('sandboxSettings.linuxComingSoon')}</p>
              </div>
            )}

            {tab === 'macos' && (
              <div className="space-y-3 pt-1">
                {renderStatusLine('macos')}
                <p className="text-xs text-t-text-muted leading-relaxed">{t('sandboxSettings.macosNote')}</p>
              </div>
            )}
          </section>

          <div className="pt-2">
            <button
              type="button"
              disabled={saving}
              onClick={() => void handleSave()}
              className="w-full rounded-lg bg-accent px-4 py-2 text-xs font-medium text-white hover:opacity-90 disabled:opacity-50"
            >
              {saving ? t('settings.saving') : t('settings.save')}
            </button>
            <p className="text-[10px] text-t-text-muted mt-1.5 text-center">{t('settings.saveHint')}</p>
          </div>
        </>
      )}
    </div>
  );
}
