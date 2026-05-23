import { useCallback, useEffect, useState } from 'react';
import { useT, LOCALE_LABELS } from '../i18n';
import type { Locale } from '../i18n';
import type { RuntimeConnectionState } from '../api/client';
import { fetchSystemSettings, saveSystemSettings, type SystemSettings } from '../api/client';
import { confirmDialog } from '../lib/confirmDialog';

type Theme = 'light' | 'dark';

interface Props {
  runtimeConn: RuntimeConnectionState;
  desktopHost: boolean;
  apiKeyConfigured: boolean | null;
  platform: string;
  theme: Theme;
  onToggleTheme: () => void;
  /** When true, saving settings restarts the sidecar and interrupts the active stream. */
  streaming?: boolean;
  onSettingsSaved?: (settings: SystemSettings) => void;
}

export default function SettingsPanel({
  runtimeConn,
  desktopHost,
  apiKeyConfigured,
  platform,
  theme,
  onToggleTheme,
  streaming = false,
  onSettingsSaved,
}: Props) {
  const { t, locale, setLocale } = useT();

  const [settings, setSettings] = useState<SystemSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!desktopHost) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    fetchSystemSettings()
      .then((s) => {
        if (!cancelled) setSettings(s);
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [desktopHost]);

  const handleSave = useCallback(async () => {
    if (!settings || !desktopHost) return;
    if (streaming && !(await confirmDialog(t('settings.saveRestartsSidecar')))) {
      return;
    }
    setSaving(true);
    try {
      await saveSystemSettings(settings);
      onSettingsSaved?.(settings);
    } finally {
      setSaving(false);
    }
  }, [settings, desktopHost, streaming, t, onSettingsSaved]);

  const update = useCallback(<K extends keyof SystemSettings>(key: K, value: SystemSettings[K]) => {
    setSettings((prev) => (prev ? { ...prev, [key]: value } : prev));
  }, []);

  const selectCls = 'w-full rounded-lg border border-divider bg-canvas px-3 py-2 text-xs text-t-text focus:outline-none focus:ring-1 focus:ring-accent';
  const labelCls = 'text-[11px] font-medium text-t-text-secondary';
  const descCls = 'text-[10px] text-t-text-muted';

  return (
    <div className="p-4 space-y-5 overflow-y-auto h-full">
      <div className="space-y-1.5 pb-3 border-b border-divider">
        <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">{t('settings.diagInfo')}</p>
        <div className="flex justify-between gap-2 py-1 text-xs">
          <span className="text-t-text-muted">{t('settings.runtimeStatus')}</span>
          <span className="text-t-text">
            {runtimeConn === 'connected' && t('common.runtimeReady')}
            {runtimeConn === 'checking' && t('common.runtimeChecking')}
            {runtimeConn === 'offline' && t('common.runtimeOffline')}
            {runtimeConn === 'auth_mismatch' && t('common.runtimeAuthMismatch')}
          </span>
        </div>
        <div className="flex justify-between gap-2 py-1 text-xs">
          <span className="text-t-text-muted">{t('settings.runtimeConn')}</span>
          <span className="text-t-text">{desktopHost ? t('settings.desktopMode') : t('settings.browserMode')}</span>
        </div>
        <div className="flex justify-between gap-2 py-1 text-xs">
          <span className="text-t-text-muted">{t('settings.apiKey')}</span>
          <span className={apiKeyConfigured ? 'text-emerald-600' : 'text-amber-600'}>
            {apiKeyConfigured === null ? '…' : apiKeyConfigured ? t('settings.configured') : t('settings.notConfigured')}
          </span>
        </div>
      </div>

      {!desktopHost && (
        <p className="text-xs text-t-text-muted leading-relaxed">{t('settings.notAvailable')}</p>
      )}

      {loading && (
        <p className="text-xs text-t-text-muted">{t('settings.loadingSettings')}</p>
      )}

      {settings && (
        <>
          <section className="space-y-3">
            <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">{t('settings.core')}</p>

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.defaultModel')}</span>
              <select
                className={selectCls}
                value={settings.default_model}
                onChange={(e) => update('default_model', e.target.value)}
              >
                <option value="deepseek-v4-pro">DeepSeek V4 Pro</option>
                <option value="deepseek-v4-flash">DeepSeek V4 Flash</option>
                <option value="deepseek-chat">DeepSeek Chat</option>
                <option value="deepseek-reasoner">DeepSeek Reasoner</option>
              </select>
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.reasoningEffort')}</span>
              <select
                className={selectCls}
                value={settings.reasoning_effort}
                onChange={(e) => update('reasoning_effort', e.target.value)}
              >
                <option value="max">{t('settings.reasoningMax')}</option>
                <option value="high">{t('settings.reasoningHigh')}</option>
                <option value="auto">{t('settings.reasoningAuto')}</option>
                <option value="off">{t('settings.reasoningOff')}</option>
              </select>
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.costCurrency')}</span>
              <select
                className={selectCls}
                value={settings.cost_currency}
                onChange={(e) => update('cost_currency', e.target.value)}
              >
                <option value="usd">USD ($)</option>
                <option value="cny">CNY (¥)</option>
              </select>
            </label>
          </section>

          <section className="space-y-3">
            <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">{t('settings.security')}</p>

            {[
              ['allow_shell', 'shellTool'] as const,
              ['web_search', 'webSearch'] as const,
              ['exec_policy', 'execPolicy'] as const,
              ['subagents_enabled', 'subagents', 'subagentsDesc'] as const,
            ].map(([key, i18nKey, ...rest]) => (
              <label key={key} className="flex items-center justify-between gap-2 py-1">
                <div className="flex-1 min-w-0">
                  <span className={labelCls}>{t(`settings.${i18nKey}` as any)}</span>
                  {rest[0] && <p className={descCls}>{t(`settings.${rest[0]}` as any)}</p>}
                </div>
                <input
                  type="checkbox"
                  className="shrink-0 w-4 h-4 accent-accent rounded"
                  checked={settings[key] as boolean}
                  onChange={(e) => update(key as keyof SystemSettings, e.target.checked)}
                />
              </label>
            ))}

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.sandboxMode')}</span>
              <select
                className={selectCls}
                value={settings.sandbox_mode}
                onChange={(e) => update('sandbox_mode', e.target.value)}
              >
                <option value="workspace-write">{t('settings.sandboxWorkspace')}</option>
                <option value="read-only">{t('settings.sandboxReadOnly')}</option>
                <option value="full-access">{t('settings.sandboxFullAccess')}</option>
              </select>
              {platform !== 'darwin' && (
                <p className="text-[11px] text-t-text-muted mt-0.5">{t('settings.sandboxDegradedMode')}</p>
              )}
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.approvalPolicy')}</span>
              <select
                className={selectCls}
                value={settings.approval_policy}
                onChange={(e) => update('approval_policy', e.target.value)}
              >
                <option value="on-request">{t('settings.approvalOnRequest')}</option>
                <option value="untrusted">{t('settings.approvalUntrusted')}</option>
                <option value="never">{t('settings.approvalNever')}</option>
              </select>
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.maxSubagents')}</span>
              <div className="flex items-center gap-2">
                <input
                  type="range"
                  min={1}
                  max={20}
                  value={settings.max_subagents}
                  onChange={(e) => update('max_subagents', Number(e.target.value))}
                  className="flex-1 accent-accent"
                />
                <span className="text-xs text-t-text w-6 text-right">{settings.max_subagents}</span>
              </div>
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.subagentStepTimeout')}</span>
              <p className={descCls}>{t('settings.subagentStepTimeoutDesc')}</p>
              <div className="flex items-center gap-2">
                <input
                  type="range"
                  min={60}
                  max={600}
                  step={30}
                  value={settings.subagent_step_timeout_secs}
                  onChange={(e) =>
                    update('subagent_step_timeout_secs', Number(e.target.value))
                  }
                  className="flex-1 accent-accent"
                />
                <span className="text-xs text-t-text w-10 text-right tabular-nums">
                  {settings.subagent_step_timeout_secs}s
                </span>
              </div>
            </label>
          </section>

          <section className="space-y-3">
            <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">{t('settings.contextSection')}</p>

            <label className="flex items-center justify-between gap-2 py-1">
              <div className="flex-1 min-w-0">
                <span className={labelCls}>{t('settings.autoCompact')}</span>
                <p className={descCls}>{t('settings.autoCompactDesc')}</p>
              </div>
              <input
                type="checkbox"
                className="shrink-0 w-4 h-4 accent-accent rounded"
                checked={settings.auto_compact}
                onChange={(e) => update('auto_compact', e.target.checked)}
              />
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.compactionThreshold')}</span>
              <p className={descCls}>
                {t('settings.compactionThresholdDesc', {
                  default: settings.compaction_threshold_default.toLocaleString(),
                })}
              </p>
              <input
                type="number"
                min={50_000}
                step={10_000}
                className={selectCls}
                value={settings.compaction_threshold_tokens}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  if (v >= 50_000) update('compaction_threshold_tokens', v);
                }}
              />
            </label>
          </section>

          <section className="space-y-3">
            <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">{t('settings.advanced')}</p>

            {[
              ['lsp_enabled', 'lspDiag', 'lspDiagDesc'] as const,
              ['memory_enabled', 'userMemory', 'userMemoryDesc'] as const,
              ['snapshots_enabled', 'snapshots', 'snapshotsDesc'] as const,
            ].map(([key, i18nTitle, i18nDesc]) => (
              <label key={key} className="flex items-center justify-between gap-2 py-1">
                <div className="flex-1 min-w-0">
                  <span className={labelCls}>{t(`settings.${i18nTitle}` as any)}</span>
                  <p className={descCls}>{t(`settings.${i18nDesc}` as any)}</p>
                </div>
                <input
                  type="checkbox"
                  className="shrink-0 w-4 h-4 accent-accent rounded"
                  checked={settings[key] as boolean}
                  onChange={(e) => update(key as keyof SystemSettings, e.target.checked)}
                />
              </label>
            ))}

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.notifyMethod')}</span>
              <select
                className={selectCls}
                value={settings.notify_method}
                onChange={(e) => update('notify_method', e.target.value)}
              >
                <option value="auto">{t('settings.notifyAuto')}</option>
                <option value="osc9">{t('settings.notifyOsc9')}</option>
                <option value="bel">{t('settings.notifyBel')}</option>
                <option value="off">{t('settings.notifyOff')}</option>
              </select>
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.sessionFileLimit')}</span>
              <p className={descCls}>{t('settings.sessionFileLimitDesc')}</p>
              <input
                type="number"
                min={0}
                className={selectCls}
                value={settings.session_file_mb}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  if (v >= 0) update('session_file_mb', v);
                }}
              />
            </label>
          </section>

          <section className="space-y-3">
            <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">{t('settings.appearance')}</p>

            <label className="flex items-center justify-between gap-2 py-1">
              <span className={labelCls}>{t('settings.theme')}</span>
              <button
                type="button"
                onClick={onToggleTheme}
                className="text-xs text-accent hover:underline"
              >
                {theme === 'light' ? t('settings.themeLight') : t('settings.themeDark')}
              </button>
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.language')}</span>
              <select
                className={selectCls}
                value={locale}
                onChange={(e) => setLocale(e.target.value as Locale)}
              >
                {(Object.entries(LOCALE_LABELS) as [Locale, string][]).map(([id, label]) => (
                  <option key={id} value={id}>{label}</option>
                ))}
              </select>
            </label>
          </section>

          <div className="pt-3 border-t border-divider">
            <button
              type="button"
              onClick={handleSave}
              disabled={saving}
              className="w-full py-2 rounded-lg bg-accent text-white text-sm font-medium hover:opacity-90 disabled:opacity-50 transition-colors"
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
