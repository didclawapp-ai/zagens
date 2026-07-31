import { useCallback, useEffect, useState } from 'react';
import { useT, LOCALE_LABELS } from '../i18n';
import type { Locale } from '../i18n';
import type { RuntimeConnectionState } from '../api/client';
import {
  deleteThreadConfigField,
  fetchSystemSettings,
  fetchThreadConfig,
  putThreadConfig,
  saveSystemSettings,
  type SystemSettings,
} from '../api/client';
import type { Theme } from '../lib/appPreferences';
import { confirmDialog } from '../lib/confirmDialog';
import {
  applyEffectiveOverlayToSystemSettings,
  overlayHasSystemOverrides,
  SYSTEM_OVERLAY_SECTIONS,
  systemSettingsToOverlay,
  type ThreadConfigResponse,
} from '../lib/threadConfigOverlay';
import { hidesEffortOff, mapReasoningEffort } from '../lib/modelParams';

type WriteScope = 'session' | 'global';

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
  /** When set, session-scoped settings write to the thread overlay (zero restart). */
  threadId?: string | null;
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
  threadId = null,
}: Props) {
  const { t, locale, setLocale } = useT();

  const [settings, setSettings] = useState<SystemSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [writeScope, setWriteScope] = useState<WriteScope>(threadId?.trim() ? 'session' : 'global');
  const [threadConfig, setThreadConfig] = useState<ThreadConfigResponse | null>(null);

  const hasThread = Boolean(threadId?.trim());
  const sessionScoped = hasThread && writeScope === 'session';
  const hasSessionOverlay = overlayHasSystemOverrides(threadConfig?.overlay);

  const loadSettings = useCallback(async () => {
    const globalSettings = await fetchSystemSettings();
    if (sessionScoped && threadId?.trim()) {
      const cfg = await fetchThreadConfig(threadId.trim());
      setThreadConfig(cfg);
      setSettings(applyEffectiveOverlayToSystemSettings(cfg.effective, globalSettings));
    } else {
      setThreadConfig(null);
      setSettings(globalSettings);
    }
  }, [sessionScoped, threadId]);

  useEffect(() => {
    if (!desktopHost) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    loadSettings()
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [desktopHost, loadSettings]);

  useEffect(() => {
    setWriteScope(threadId?.trim() ? 'session' : 'global');
  }, [threadId]);

  const handleSave = useCallback(async () => {
    if (!settings || !desktopHost) return;
    if (sessionScoped && threadId?.trim()) {
      setSaving(true);
      try {
        const cfg = await putThreadConfig(threadId.trim(), systemSettingsToOverlay(settings));
        setThreadConfig(cfg);
      } finally {
        setSaving(false);
      }
      return;
    }
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
  }, [settings, desktopHost, sessionScoped, threadId, streaming, t, onSettingsSaved]);

  const handleScopeChange = useCallback(
    async (next: WriteScope) => {
      if (next === writeScope) return;
      setWriteScope(next);
      setLoading(true);
      try {
        const globalSettings = await fetchSystemSettings();
        if (next === 'session' && threadId?.trim()) {
          const cfg = await fetchThreadConfig(threadId.trim());
          setThreadConfig(cfg);
          setSettings(applyEffectiveOverlayToSystemSettings(cfg.effective, globalSettings));
        } else {
          setThreadConfig(null);
          setSettings(globalSettings);
        }
      } finally {
        setLoading(false);
      }
    },
    [writeScope, threadId],
  );

  const handleClearSessionOverride = useCallback(async () => {
    const id = threadId?.trim();
    if (!id || !hasSessionOverlay) return;
    if (!(await confirmDialog(t('settings.clearSessionOverrideConfirm')))) return;
    setSaving(true);
    try {
      for (const section of SYSTEM_OVERLAY_SECTIONS) {
        await deleteThreadConfigField(id, section).catch(() => {});
      }
      const globalSettings = await fetchSystemSettings();
      const cfg = await fetchThreadConfig(id);
      setThreadConfig(cfg);
      setSettings(applyEffectiveOverlayToSystemSettings(cfg.effective, globalSettings));
    } finally {
      setSaving(false);
    }
  }, [threadId, hasSessionOverlay, t]);

  const update = useCallback(<K extends keyof SystemSettings>(key: K, value: SystemSettings[K]) => {
    setSettings((prev) => (prev ? { ...prev, [key]: value } : prev));
  }, []);

  const selectCls = 'w-full rounded-lg border border-divider bg-canvas px-3 py-2 text-xs text-t-text focus:outline-none focus:ring-1 focus:ring-accent disabled:opacity-50';
  const labelCls = 'text-[11px] font-medium text-t-text-secondary';
  const descCls = 'text-[10px] text-t-text-muted';
  const sectionCls = 'text-[11px] font-semibold uppercase tracking-wider text-t-text-muted';
  // Process-global fields stay on `config.toml` even in session scope; disable them there.
  const globalDisabled = sessionScoped;

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

      {hasThread && desktopHost && (
        <section className="space-y-2 pb-3 border-b border-divider">
          <p className={sectionCls}>{t('settings.scopeSection')}</p>
          <div className="flex flex-wrap gap-1.5">
            {(['session', 'global'] as const).map((scope) => (
              <button
                key={scope}
                type="button"
                disabled={saving}
                onClick={() => void handleScopeChange(scope)}
                className={`rounded-full px-3 py-1 text-[11px] font-medium transition-colors disabled:opacity-50 ${
                  writeScope === scope
                    ? 'bg-accent text-white'
                    : 'border border-divider bg-canvas text-t-text-secondary hover:border-accent/40'
                }`}
              >
                {t(scope === 'session' ? 'settings.scopeSession' : 'settings.scopeGlobal')}
              </button>
            ))}
          </div>
          <p className={descCls}>
            {sessionScoped ? t('settings.scopeSessionHint') : t('settings.scopeGlobalHint')}
          </p>
          {hasSessionOverlay && (
            <div className="space-y-1.5">
              <p className="text-[10px] text-accent leading-relaxed">
                {sessionScoped
                  ? t('settings.sessionOverrideBadge')
                  : t('settings.sessionOverrideActiveWhileGlobal')}
              </p>
              <button
                type="button"
                disabled={saving}
                onClick={() => void handleClearSessionOverride()}
                className="rounded-full border border-divider bg-canvas px-3 py-1 text-[11px] font-medium text-t-text-secondary transition-colors hover:border-accent/40 disabled:opacity-50"
              >
                {t('settings.clearSessionOverride')}
              </button>
            </div>
          )}
        </section>
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
              <input
                type="text"
                list="settings-model-suggestions"
                className={selectCls}
                disabled={globalDisabled}
                value={settings.default_model}
                onChange={(e) => update('default_model', e.target.value)}
                placeholder={t('settings.defaultModelPlaceholder')}
              />
              <datalist id="settings-model-suggestions">
                {(settings.available_models?.length
                  ? settings.available_models
                  : ['deepseek-v4-pro', 'deepseek-v4-flash']
                ).map((m) => (
                  <option key={m} value={m} />
                ))}
              </datalist>
              <p className={descCls}>{t('settings.defaultModelHint')}</p>
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.reasoningEffort')}</span>
              <select
                className={selectCls}
                disabled={globalDisabled}
                value={
                  hidesEffortOff(settings.default_model) && settings.reasoning_effort === 'off'
                    ? (mapReasoningEffort(settings.default_model, 'off') ?? 'max')
                    : settings.reasoning_effort
                }
                onChange={(e) => update('reasoning_effort', e.target.value)}
              >
                <option value="max">{t('settings.reasoningMax')}</option>
                <option value="high">{t('settings.reasoningHigh')}</option>
                <option value="low">{t('settings.reasoningLow')}</option>
                <option value="auto">{t('settings.reasoningAuto')}</option>
                {!hidesEffortOff(settings.default_model) && (
                  <option value="off">{t('settings.reasoningOff')}</option>
                )}
              </select>
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.costCurrency')}</span>
              <select
                className={selectCls}
                disabled={globalDisabled}
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
                  className="shrink-0 w-4 h-4 accent-accent rounded disabled:opacity-50"
                  disabled={key === 'allow_shell' ? globalDisabled : false}
                  checked={settings[key] as boolean}
                  onChange={(e) => update(key as keyof SystemSettings, e.target.checked)}
                />
              </label>
            ))}

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
                <option value="auto">{t('settings.approvalAuto')}</option>
              </select>
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.maxSubagents')}</span>
              <div className="flex items-center gap-2">
                <input
                  type="range"
                  min={1}
                  max={20}
                  disabled={globalDisabled}
                  value={settings.max_subagents}
                  onChange={(e) => update('max_subagents', Number(e.target.value))}
                  className="flex-1 accent-accent disabled:opacity-50"
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
                  min={120}
                  max={1800}
                  step={60}
                  disabled={globalDisabled}
                  value={settings.subagent_step_timeout_secs}
                  onChange={(e) =>
                    update('subagent_step_timeout_secs', Number(e.target.value))
                  }
                  className="flex-1 accent-accent disabled:opacity-50"
                />
                <span className="text-xs text-t-text w-10 text-right tabular-nums">
                  {settings.subagent_step_timeout_secs}s
                </span>
              </div>
            </label>

            <div className="space-y-2 pt-1">
              <p className={labelCls}>{t('settings.craftModelOverrides')}</p>
              <p className={descCls}>{t('settings.craftModelOverridesDesc')}</p>
              {(
                [
                  ['subagent_review_model', 'craftModelReview'] as const,
                  ['subagent_implementer_model', 'craftModelImplementer'] as const,
                  ['subagent_verifier_model', 'craftModelVerifier'] as const,
                  ['subagent_auditor_model', 'craftModelAuditor'] as const,
                ] as const
              ).map(([key, i18nKey]) => (
                <label key={key} className="block space-y-1">
                  <span className="text-xs text-t-text-muted">{t(`settings.${i18nKey}` as any)}</span>
                  <select
                    className={selectCls}
                    disabled={globalDisabled}
                    value={settings[key]}
                    onChange={(e) => update(key, e.target.value)}
                  >
                    <option value="">{t('settings.craftModelInherit')}</option>
                    {(settings.available_models ?? []).map((model) => (
                      <option key={model} value={model}>
                        {model}
                      </option>
                    ))}
                  </select>
                </label>
              ))}
            </div>
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
              ['topic_memory_enabled', 'topicMemory', 'topicMemoryDesc'] as const,
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

            {settings.topic_memory_enabled && (
              <label className="block space-y-1">
                <span className={labelCls}>{t('settings.topicMemoryInterval')}</span>
                <p className={descCls}>{t('settings.topicMemoryIntervalDesc')}</p>
                <input
                  type="number"
                  min={1}
                  max={50}
                  className={selectCls}
                  value={settings.topic_memory_inject_interval}
                  onChange={(e) => {
                    const v = Number(e.target.value);
                    if (v >= 1) update('topic_memory_inject_interval', v);
                  }}
                />
              </label>
            )}

            <label className="block space-y-1">
              <span className={labelCls}>{t('settings.notifyMethod')}</span>
              <select
                className={selectCls}
                disabled={globalDisabled}
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
                disabled={globalDisabled}
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
                {theme === 'light'
                  ? t('settings.themeLight')
                  : theme === 'dark'
                    ? t('settings.themeDark')
                    : t('settings.themeDusk')}
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
            <p className="text-[10px] text-t-text-muted mt-1.5 text-center">
              {sessionScoped ? t('settings.saveHintSession') : t('settings.saveHint')}
            </p>
          </div>
        </>
      )}
    </div>
  );
}
