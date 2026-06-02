import { useCallback, useEffect, useState } from 'react';
import { useT } from '../i18n';
import {
  fetchLhtSettings,
  saveLhtSettings,
  type LhtGateMode,
  type LhtSettings,
} from '../api/client';
import { confirmDialog } from '../lib/confirmDialog';

interface Props {
  desktopHost: boolean;
  streaming?: boolean;
}

const GATE_OPTIONS: LhtGateMode[] = ['off', 'observe', 'enforce'];

export default function LhtSettingsPanel({ desktopHost, streaming = false }: Props) {
  const { t } = useT();
  const [settings, setSettings] = useState<LhtSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!desktopHost) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    fetchLhtSettings()
      .then((s) => {
        if (!cancelled) setSettings(s);
      })
      .catch(() => {})
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [desktopHost]);

  const update = useCallback(<K extends keyof LhtSettings>(key: K, value: LhtSettings[K]) => {
    setSettings((prev) => (prev ? { ...prev, [key]: value } : prev));
  }, []);

  const handleSave = useCallback(async () => {
    if (!settings || !desktopHost) return;
    const enforcing =
      settings.auto_verify_replay === 'enforce' ||
      settings.toolchain_gate === 'enforce' ||
      settings.stub_gate === 'enforce';
    if (enforcing && !(await confirmDialog(t('lhtSettings.enforceConfirm')))) {
      return;
    }
    if (streaming && !(await confirmDialog(t('settings.saveRestartsSidecar')))) {
      return;
    }
    setSaving(true);
    try {
      await saveLhtSettings(settings);
    } finally {
      setSaving(false);
    }
  }, [settings, desktopHost, streaming, t]);

  const selectCls =
    'w-full rounded-lg border border-divider bg-canvas px-3 py-2 text-xs text-t-text focus:outline-none focus:ring-1 focus:ring-accent';
  const labelCls = 'text-[11px] font-medium text-t-text-secondary';
  const descCls = 'text-[10px] text-t-text-muted';
  const sectionCls = 'text-[11px] font-semibold uppercase tracking-wider text-t-text-muted';

  const gateLabel = (mode: LhtGateMode) => t(`lhtSettings.gate_${mode}` as 'lhtSettings.gate_off');

  return (
    <div className="p-4 space-y-5 overflow-y-auto h-full">
      <p className="text-xs text-t-text-muted leading-relaxed border-b border-divider pb-3">
        {t('lhtSettings.intro')}
      </p>

      {!desktopHost && (
        <p className="text-xs text-t-text-muted leading-relaxed">{t('settings.notAvailable')}</p>
      )}

      {loading && <p className="text-xs text-t-text-muted">{t('settings.loadingSettings')}</p>}

      {settings && (
        <>
          <section className="space-y-3">
            <p className={sectionCls}>{t('lhtSettings.sectionHarness')}</p>

            <label className="flex items-center justify-between gap-2 py-1">
              <div className="flex-1 min-w-0">
                <span className={labelCls}>{t('lhtSettings.enabled')}</span>
                <p className={descCls}>{t('lhtSettings.enabledDesc')}</p>
              </div>
              <input
                type="checkbox"
                className="shrink-0 w-4 h-4 accent-accent rounded"
                checked={settings.enabled}
                onChange={(e) => update('enabled', e.target.checked)}
              />
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('lhtSettings.mode')}</span>
              <p className={descCls}>{t('lhtSettings.modeDesc')}</p>
              <select
                className={selectCls}
                value={settings.mode}
                onChange={(e) => update('mode', e.target.value as LhtSettings['mode'])}
              >
                <option value="auto">{t('lhtSettings.modeAuto')}</option>
                <option value="strict">{t('lhtSettings.modeStrict')}</option>
              </select>
            </label>

            <label className="flex items-center justify-between gap-2 py-1">
              <div className="flex-1 min-w-0">
                <span className={labelCls}>{t('lhtSettings.progressViaGit')}</span>
                <p className={descCls}>{t('lhtSettings.progressViaGitDesc')}</p>
              </div>
              <input
                type="checkbox"
                className="shrink-0 w-4 h-4 accent-accent rounded"
                checked={settings.progress_via_git}
                onChange={(e) => update('progress_via_git', e.target.checked)}
              />
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('lhtSettings.maxNudges')}</span>
              <input
                type="number"
                min={1}
                max={20}
                className={selectCls}
                value={settings.max_nudges_per_item}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  if (v >= 1 && v <= 20) update('max_nudges_per_item', v);
                }}
              />
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('lhtSettings.blockedNudges')}</span>
              <input
                type="number"
                min={1}
                max={10}
                className={selectCls}
                value={settings.blocked_nudges_without_progress}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  if (v >= 1 && v <= 10) update('blocked_nudges_without_progress', v);
                }}
              />
            </label>

            <label className="flex items-center justify-between gap-2 py-1">
              <div className="flex-1 min-w-0">
                <span className={labelCls}>{t('lhtSettings.autoContinue')}</span>
                <p className={descCls}>{t('lhtSettings.autoContinueDesc')}</p>
              </div>
              <input
                type="checkbox"
                className="shrink-0 w-4 h-4 accent-accent rounded"
                checked={settings.auto_continue}
                onChange={(e) => update('auto_continue', e.target.checked)}
              />
            </label>

            {settings.auto_continue && (
              <label className="block space-y-1">
                <span className={labelCls}>{t('lhtSettings.maxAutoContinue')}</span>
                <input
                  type="number"
                  min={1}
                  max={64}
                  className={selectCls}
                  value={settings.max_auto_continue_rounds}
                  onChange={(e) => {
                    const v = Number(e.target.value);
                    if (v >= 1 && v <= 64) update('max_auto_continue_rounds', v);
                  }}
                />
              </label>
            )}
          </section>

          <section className="space-y-3">
            <p className={sectionCls}>{t('lhtSettings.sectionCompletionGate')}</p>
            <p className={descCls}>{t('lhtSettings.completionGateIntro')}</p>

            {(
              [
                ['auto_verify_replay', 'autoVerifyReplay', 'autoVerifyReplayDesc'],
                ['toolchain_gate', 'toolchainGate', 'toolchainGateDesc'],
                ['stub_gate', 'stubGate', 'stubGateDesc'],
              ] as const
            ).map(([key, titleKey, descKey]) => (
              <label key={key} className="block space-y-1">
                <span className={labelCls}>{t(`lhtSettings.${titleKey}`)}</span>
                <p className={descCls}>{t(`lhtSettings.${descKey}`)}</p>
                <select
                  className={selectCls}
                  value={settings[key]}
                  onChange={(e) => update(key, e.target.value as LhtGateMode)}
                >
                  {GATE_OPTIONS.map((opt) => (
                    <option key={opt} value={opt}>
                      {gateLabel(opt)}
                    </option>
                  ))}
                </select>
              </label>
            ))}

            <label className="block space-y-1">
              <span className={labelCls}>{t('lhtSettings.maxManifestRounds')}</span>
              <input
                type="number"
                min={1}
                max={32}
                className={selectCls}
                value={settings.max_manifest_rounds}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  if (v >= 1 && v <= 32) update('max_manifest_rounds', v);
                }}
              />
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('lhtSettings.maxAuditRounds')}</span>
              <input
                type="number"
                min={1}
                max={32}
                className={selectCls}
                value={settings.max_audit_rounds}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  if (v >= 1 && v <= 32) update('max_audit_rounds', v);
                }}
              />
            </label>

            <label className="block space-y-1">
              <span className={labelCls}>{t('lhtSettings.maxInfraStrikes')}</span>
              <input
                type="number"
                min={1}
                max={16}
                className={selectCls}
                value={settings.max_infra_strikes}
                onChange={(e) => {
                  const v = Number(e.target.value);
                  if (v >= 1 && v <= 16) update('max_infra_strikes', v);
                }}
              />
            </label>

            {(settings.custom_verify_count > 0 || settings.custom_deliverable_count > 0) && (
              <p className="text-[10px] text-amber-600 dark:text-amber-400 leading-relaxed">
                {t('lhtSettings.customManifestHint', {
                  verify: String(settings.custom_verify_count),
                  deliverable: String(settings.custom_deliverable_count),
                })}
              </p>
            )}
          </section>

          <div className="pt-3 border-t border-divider">
            <button
              type="button"
              onClick={() => void handleSave()}
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
