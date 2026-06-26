import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n';
import type { DesktopTaskTypePreference } from '../types/desktop';
import { persistOnboardingComplete, persistTaskTypePreference } from '../lib/appPreferences';

const DEEPSEEK_API_KEYS_URL = 'https://platform.deepseek.com/api_keys';

type Phase = 'key' | 'mode';

interface Props {
  apiKeyConfigured: boolean | null;
  needsKeyStep: boolean;
  needsModeStep: boolean;
  refreshApiKeyStatus: () => void;
  taskTypePreference: DesktopTaskTypePreference;
  onTaskTypePreferenceChange: (value: DesktopTaskTypePreference) => void;
  onComplete: (taskType?: DesktopTaskTypePreference) => void;
}

const MODE_OPTIONS: DesktopTaskTypePreference[] = ['auto', 'code', 'office'];

function resolveInitialPhase(
  needsKeyStep: boolean,
  needsModeStep: boolean,
  apiKeyConfigured: boolean | null,
): Phase {
  if (needsKeyStep && apiKeyConfigured !== true) return 'key';
  return 'mode';
}

export default function OnboardingOverlay({
  apiKeyConfigured,
  needsKeyStep,
  needsModeStep,
  refreshApiKeyStatus,
  taskTypePreference,
  onTaskTypePreferenceChange,
  onComplete,
}: Props) {
  const { t } = useT();
  const [phase, setPhase] = useState<Phase>(() =>
    resolveInitialPhase(needsKeyStep, needsModeStep, apiKeyConfigured),
  );
  const [key, setKey] = useState('');
  const [saveBusy, setSaveBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const setupSteps = useMemo(() => {
    const steps: Phase[] = [];
    if (needsKeyStep) steps.push('key');
    if (needsModeStep) steps.push('mode');
    return steps;
  }, [needsKeyStep, needsModeStep]);

  useEffect(() => {
    if (!needsKeyStep && !needsModeStep) {
      onComplete();
    }
  }, [needsKeyStep, needsModeStep, onComplete]);

  const advanceFromKey = useCallback(() => {
    if (needsModeStep) {
      setPhase('mode');
      return;
    }
    onComplete();
  }, [needsModeStep, onComplete]);

  const handleSaveKey = useCallback(async () => {
    setError(null);
    setSaveBusy(true);
    try {
      await invoke('save_deepseek_api_key', { key: key.trim() });
      setKey('');
      refreshApiKeyStatus();
      advanceFromKey();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaveBusy(false);
    }
  }, [key, refreshApiKeyStatus, advanceFromKey]);

  const stepLabels: Record<Phase, string> = {
    key: t('onboarding.stepKey'),
    mode: t('onboarding.stepMode'),
  };

  const phaseIndex = setupSteps.indexOf(phase);
  const showStepRail = setupSteps.length > 1;

  if (!needsKeyStep && !needsModeStep) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-[210] flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      <div className="w-full max-w-md rounded-2xl bg-canvas-alt border border-border shadow-2xl overflow-hidden">
        <div className="px-6 pt-6 pb-4">
          <h1 className="text-lg font-semibold text-t-text">{t('onboarding.welcomeTitle')}</h1>
          <p className="text-xs text-t-text-muted mt-1">
            {setupSteps.length > 1 ? t('onboarding.welcomeSubtitle') : t('onboarding.connectHint')}
          </p>
        </div>

        {showStepRail && (
          <div className="flex items-center gap-2 px-6 pb-5">
            {setupSteps.map((s, idx) => {
              const active = s === phase;
              const done = idx < phaseIndex;
              const n = idx + 1;
              return (
                <div key={s} className="flex items-center gap-2 flex-1">
                  <div className="flex items-center gap-2">
                    <span
                      className={`flex h-6 w-6 items-center justify-center rounded-full text-[11px] font-medium transition-colors ${
                        active
                          ? 'bg-accent text-accent-text'
                          : done
                            ? 'bg-emerald-500 text-white'
                            : 'bg-input-bg text-t-text-muted border border-input-border'
                      }`}
                    >
                      {done ? '✓' : n}
                    </span>
                    <span
                      className={`text-xs ${active ? 'text-t-text font-medium' : 'text-t-text-muted'}`}
                    >
                      {stepLabels[s]}
                    </span>
                  </div>
                  {idx < setupSteps.length - 1 && <div className="flex-1 h-px bg-border" />}
                </div>
              );
            })}
          </div>
        )}

        <div className="px-6 pb-6 min-h-[200px]">
          {phase === 'key' && (
            <div className="space-y-3">
              <p className="text-sm font-medium text-t-text">{t('onboarding.keyTitle')}</p>
              <p className="text-xs text-t-text-muted leading-relaxed">{t('onboarding.keyDesc')}</p>
              {apiKeyConfigured ? (
                <p className="text-xs text-emerald-500">{t('onboarding.keyConfigured')}</p>
              ) : (
                <input
                  type="password"
                  autoComplete="off"
                  value={key}
                  onChange={(e) => setKey(e.target.value)}
                  placeholder={t('onboarding.keyPlaceholder')}
                  disabled={saveBusy}
                  className="w-full rounded-lg bg-input-bg border border-input-border px-3 py-2 text-sm text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50 transition-colors"
                />
              )}
              {error && <p className="text-xs text-error-text">{error}</p>}
              <button
                type="button"
                onClick={() => window.open(DEEPSEEK_API_KEYS_URL, '_blank', 'noopener,noreferrer')}
                className="text-xs text-accent hover:underline"
              >
                {t('onboarding.keyGetLink')}
              </button>
              <div className="flex items-center justify-end pt-2 gap-3">
                {!apiKeyConfigured && (
                  <button
                    type="button"
                    onClick={advanceFromKey}
                    className="text-xs text-t-text-muted hover:text-t-text transition-colors"
                  >
                    {t('onboarding.keySkip')}
                  </button>
                )}
                <button
                  type="button"
                  disabled={saveBusy || (!apiKeyConfigured && !key.trim())}
                  onClick={() => (apiKeyConfigured ? advanceFromKey() : void handleSaveKey())}
                  className="px-4 py-2 rounded-lg bg-accent text-accent-text hover:bg-accent-hover disabled:opacity-50 text-sm font-medium transition-colors"
                >
                  {saveBusy ? t('common.saving') : t('onboarding.next')}
                </button>
              </div>
            </div>
          )}

          {phase === 'mode' && (
            <div className="space-y-3">
              <p className="text-sm font-medium text-t-text">{t('onboarding.modeTitle')}</p>
              <p className="text-xs text-t-text-muted leading-relaxed">{t('onboarding.modeDesc')}</p>
              <div className="space-y-2">
                {MODE_OPTIONS.map((mode) => {
                  const selected = taskTypePreference === mode;
                  const titleKey =
                    mode === 'auto'
                      ? 'onboarding.modeAutoTitle'
                      : mode === 'code'
                        ? 'onboarding.modeCodeTitle'
                        : 'onboarding.modeOfficeTitle';
                  const descKey =
                    mode === 'auto'
                      ? 'onboarding.modeAutoDesc'
                      : mode === 'code'
                        ? 'onboarding.modeCodeDesc'
                        : 'onboarding.modeOfficeDesc';
                  return (
                    <button
                      key={mode}
                      type="button"
                      onClick={() => {
                        onTaskTypePreferenceChange(mode);
                        persistTaskTypePreference(mode);
                      }}
                      className={`w-full text-left rounded-lg border px-3 py-2.5 transition-colors ${
                        selected
                          ? 'border-accent bg-accent/10'
                          : 'border-input-border hover:bg-input-bg'
                      }`}
                    >
                      <span className="block text-sm font-medium text-t-text">{t(titleKey)}</span>
                      <span className="block text-xs text-t-text-muted mt-0.5 leading-relaxed">
                        {t(descKey)}
                      </span>
                    </button>
                  );
                })}
              </div>
              {error && <p className="text-xs text-error-text">{error}</p>}
              <div className="flex items-center justify-end pt-2">
                <button
                  type="button"
                  disabled={saveBusy}
                  onClick={() => {
                    void (async () => {
                      setError(null);
                      setSaveBusy(true);
                      try {
                        await persistOnboardingComplete(taskTypePreference);
                        onComplete(taskTypePreference);
                      } catch (err) {
                        setError(err instanceof Error ? err.message : String(err));
                      } finally {
                        setSaveBusy(false);
                      }
                    })();
                  }}
                  className="px-4 py-2 rounded-lg bg-accent text-accent-text hover:bg-accent-hover disabled:opacity-50 text-sm font-medium transition-colors"
                >
                  {saveBusy ? t('common.saving') : t('onboarding.finish')}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
