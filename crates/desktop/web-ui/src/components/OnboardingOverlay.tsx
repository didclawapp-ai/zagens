import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n';
import type { RuntimeConnectionState } from '../api/client';
import type { DesktopTaskTypePreference } from '../types/desktop';

const DEEPSEEK_API_KEYS_URL = 'https://platform.deepseek.com/api_keys';

interface Props {
  runtimeConn: RuntimeConnectionState;
  apiKeyConfigured: boolean | null;
  refreshApiKeyStatus: () => void;
  taskTypePreference: DesktopTaskTypePreference;
  onTaskTypePreferenceChange: (value: DesktopTaskTypePreference) => void;
  onComplete: () => void;
}

type Step = 1 | 2 | 3;

const MODE_OPTIONS: DesktopTaskTypePreference[] = ['auto', 'code', 'office'];

export default function OnboardingOverlay({
  runtimeConn,
  apiKeyConfigured,
  refreshApiKeyStatus,
  taskTypePreference,
  onTaskTypePreferenceChange,
  onComplete,
}: Props) {
  const { t } = useT();
  const [step, setStep] = useState<Step>(1);
  const [key, setKey] = useState('');
  const [saveBusy, setSaveBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Step 1 auto-advances once the runtime reports connected.
  useEffect(() => {
    if (step !== 1 || runtimeConn !== 'connected') return;
    const id = window.setTimeout(() => setStep(2), 500);
    return () => window.clearTimeout(id);
  }, [step, runtimeConn]);

  const handleSaveKey = useCallback(async () => {
    setError(null);
    setSaveBusy(true);
    try {
      await invoke('save_deepseek_api_key', { key: key.trim() });
      setKey('');
      refreshApiKeyStatus();
      setStep(3);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaveBusy(false);
    }
  }, [key, refreshApiKeyStatus]);

  const connectStatus = (() => {
    switch (runtimeConn) {
      case 'connected':
        return { text: t('onboarding.connectConnected'), tone: 'ok' as const };
      case 'auth_mismatch':
        return { text: t('onboarding.connectAuthMismatch'), tone: 'warn' as const };
      case 'offline':
        return { text: t('onboarding.connectOffline'), tone: 'warn' as const };
      default:
        return { text: t('onboarding.connectChecking'), tone: 'busy' as const };
    }
  })();

  const steps: Array<{ n: Step; label: string }> = [
    { n: 1, label: t('onboarding.stepConnect') },
    { n: 2, label: t('onboarding.stepKey') },
    { n: 3, label: t('onboarding.stepMode') },
  ];

  return (
    <div className="fixed inset-0 z-[200] flex items-center justify-center bg-black/60 backdrop-blur-sm p-4">
      <div className="w-full max-w-md rounded-2xl bg-canvas-alt border border-border shadow-2xl overflow-hidden">
        <div className="px-6 pt-6 pb-4">
          <h1 className="text-lg font-semibold text-t-text">{t('onboarding.welcomeTitle')}</h1>
          <p className="text-xs text-t-text-muted mt-1">{t('onboarding.welcomeSubtitle')}</p>
        </div>

        <div className="flex items-center gap-2 px-6 pb-5">
          {steps.map((s, idx) => {
            const active = s.n === step;
            const done = s.n < step;
            return (
              <div key={s.n} className="flex items-center gap-2 flex-1">
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
                    {done ? '✓' : s.n}
                  </span>
                  <span
                    className={`text-xs ${active ? 'text-t-text font-medium' : 'text-t-text-muted'}`}
                  >
                    {s.label}
                  </span>
                </div>
                {idx < steps.length - 1 && (
                  <div className="flex-1 h-px bg-border" />
                )}
              </div>
            );
          })}
        </div>

        <div className="px-6 pb-6 min-h-[200px]">
          {step === 1 && (
            <div className="space-y-4">
              <p className="text-sm font-medium text-t-text">{t('onboarding.connectTitle')}</p>
              <div className="h-2 w-full overflow-hidden rounded-full bg-input-bg">
                <div
                  className={`h-full rounded-full transition-all duration-500 ${
                    connectStatus.tone === 'ok'
                      ? 'w-full bg-emerald-500'
                      : connectStatus.tone === 'warn'
                        ? 'w-1/3 bg-amber-500 animate-pulse'
                        : 'w-2/3 bg-accent animate-pulse'
                  }`}
                />
              </div>
              <p
                className={`text-xs ${
                  connectStatus.tone === 'ok'
                    ? 'text-emerald-500'
                    : connectStatus.tone === 'warn'
                      ? 'text-amber-500'
                      : 'text-t-text-muted'
                }`}
              >
                {connectStatus.text}
              </p>
              <p className="text-[11px] text-t-text-muted leading-relaxed">
                {t('onboarding.connectHint')}
              </p>
              <div className="flex justify-end pt-2">
                <button
                  type="button"
                  disabled={runtimeConn !== 'connected'}
                  onClick={() => setStep(2)}
                  className="px-4 py-2 rounded-lg bg-accent text-accent-text hover:bg-accent-hover disabled:opacity-50 text-sm font-medium transition-colors"
                >
                  {t('onboarding.next')}
                </button>
              </div>
            </div>
          )}

          {step === 2 && (
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
              <div className="flex items-center justify-between pt-2">
                <button
                  type="button"
                  onClick={() => setStep(1)}
                  className="px-3 py-2 rounded-lg border border-input-border text-sm text-t-text-secondary hover:bg-canvas-alt transition-colors"
                >
                  {t('onboarding.back')}
                </button>
                <div className="flex items-center gap-3">
                  {!apiKeyConfigured && (
                    <button
                      type="button"
                      onClick={() => setStep(3)}
                      className="text-xs text-t-text-muted hover:text-t-text transition-colors"
                    >
                      {t('onboarding.keySkip')}
                    </button>
                  )}
                  <button
                    type="button"
                    disabled={saveBusy || (!apiKeyConfigured && !key.trim())}
                    onClick={() => (apiKeyConfigured ? setStep(3) : void handleSaveKey())}
                    className="px-4 py-2 rounded-lg bg-accent text-accent-text hover:bg-accent-hover disabled:opacity-50 text-sm font-medium transition-colors"
                  >
                    {saveBusy ? t('common.saving') : t('onboarding.next')}
                  </button>
                </div>
              </div>
            </div>
          )}

          {step === 3 && (
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
                      onClick={() => onTaskTypePreferenceChange(mode)}
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
              <div className="flex items-center justify-between pt-2">
                <button
                  type="button"
                  onClick={() => setStep(2)}
                  className="px-3 py-2 rounded-lg border border-input-border text-sm text-t-text-secondary hover:bg-canvas-alt transition-colors"
                >
                  {t('onboarding.back')}
                </button>
                <button
                  type="button"
                  onClick={onComplete}
                  className="px-4 py-2 rounded-lg bg-accent text-accent-text hover:bg-accent-hover text-sm font-medium transition-colors"
                >
                  {t('onboarding.finish')}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
