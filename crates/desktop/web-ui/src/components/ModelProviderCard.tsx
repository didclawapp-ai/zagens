import { useCallback, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n';
import { confirmDialog } from '../lib/confirmDialog';
import type { ModelProviderStatus, ProviderProbeResult } from '../types/modelProviders';
import OpenRouterModelPicker from './OpenRouterModelPicker';
import SenseNovaModelPicker from './SenseNovaModelPicker';

interface Props {
  status: ModelProviderStatus;
  expanded: boolean;
  disabled?: boolean;
  onToggle: () => void;
  onRefresh: () => void;
}

export default function ModelProviderCard({
  status,
  expanded,
  disabled = false,
  onToggle,
  onRefresh,
}: Props) {
  const { t } = useT();
  const isCustom = status.section === 'custom';
  const [keyDraft, setKeyDraft] = useState('');
  const [baseUrlDraft, setBaseUrlDraft] = useState('');
  const [modelDraft, setModelDraft] = useState('');
  const [saveBusy, setSaveBusy] = useState(false);
  const [clearBusy, setClearBusy] = useState(false);
  const [activateBusy, setActivateBusy] = useState(false);
  const [probeBusy, setProbeBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [probeResult, setProbeResult] = useState<ProviderProbeResult | null>(null);

  const handleSave = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setSaveBusy(true);
    try {
      await invoke('save_model_provider_credentials', {
        providerId: status.id,
        apiKey: keyDraft.trim() || null,
        baseUrl: isCustom ? (baseUrlDraft.trim() || status.base_url) : null,
        model: isCustom ? (modelDraft.trim() || status.model) : null,
      });
      setKeyDraft('');
      onRefresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaveBusy(false);
    }
  };

  const handleClear = async () => {
    if (
      !(await confirmDialog(
        isCustom
          ? t('models.customRemoveConfirm', { name: status.display_name })
          : t('models.clearKeyConfirm', { name: status.display_name }),
      ))
    ) {
      return;
    }
    setError(null);
    setClearBusy(true);
    try {
      await invoke('clear_model_provider_credentials', { providerId: status.id });
      setKeyDraft('');
      onRefresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setClearBusy(false);
    }
  };

  const handleActivate = async () => {
    setError(null);
    setActivateBusy(true);
    try {
      await invoke('activate_model_provider', { providerId: status.id });
      onRefresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setActivateBusy(false);
    }
  };

  const handleProbe = useCallback(async () => {
    setError(null);
    setProbeBusy(true);
    try {
      const result = await invoke<ProviderProbeResult>('probe_model_provider', {
        providerId: status.id,
      });
      setProbeResult(result);
      onRefresh();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setProbeBusy(false);
    }
  }, [status.id, onRefresh]);

  const busy = saveBusy || clearBusy || activateBusy || probeBusy || disabled;

  return (
    <div className="rounded-lg border border-card-border bg-card/40 overflow-hidden">
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-2.5 text-left hover:bg-hover/60 transition-colors disabled:opacity-50"
        onClick={onToggle}
        disabled={disabled}
        aria-expanded={expanded}
      >
        <span className="min-w-0 flex-1 truncate text-sm font-semibold text-t-text">
          {status.display_name}
        </span>
        {status.active && (
          <span className="shrink-0 rounded-full bg-accent-soft px-2 py-0.5 text-[10px] font-medium text-accent">
            {t('models.activeBadge')}
          </span>
        )}
        {status.configured && !status.active && (
          <span className="shrink-0 text-[10px] text-emerald-500/90">{t('models.configuredBadge')}</span>
        )}
        <svg
          viewBox="0 0 24 24"
          className={`h-4 w-4 shrink-0 text-t-text-muted transition-transform ${expanded ? 'rotate-180' : ''}`}
          aria-hidden
        >
          <path d="M6 9l6 6 6-6" fill="none" stroke="currentColor" strokeWidth="2" />
        </svg>
      </button>

      {expanded && (
        <div className="border-t border-divider px-3 py-3 space-y-3">
          {status.model && !isCustom && (
            <p className="text-[11px] text-t-text-muted">
              {t('models.currentModel')}: <span className="text-t-text-secondary">{status.model}</span>
            </p>
          )}

          {isCustom && (
            <>
              <label className="block space-y-1">
                <span className="text-[11px] font-medium text-t-text-secondary">
                  {t('models.customBaseUrl')}
                </span>
                <input
                  type="url"
                  value={baseUrlDraft || status.base_url || ''}
                  onChange={(e) => setBaseUrlDraft(e.target.value)}
                  disabled={busy}
                  className="w-full rounded-lg bg-input-bg border border-input-border px-3 py-2 text-sm text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50"
                />
              </label>
              <label className="block space-y-1">
                <span className="text-[11px] font-medium text-t-text-secondary">
                  {t('models.customModelId')}
                </span>
                <input
                  type="text"
                  value={modelDraft || status.model || ''}
                  onChange={(e) => setModelDraft(e.target.value)}
                  disabled={busy}
                  className="w-full rounded-lg bg-input-bg border border-input-border px-3 py-2 text-sm text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50"
                />
              </label>
            </>
          )}

          <form onSubmit={(e) => void handleSave(e)} className="space-y-2">
            <label className="block space-y-1">
              <span className="text-[11px] font-medium text-t-text-secondary">
                {status.key_required ? t('models.apiKeyLabel') : t('models.apiKeyOptional')}
              </span>
              <input
                type="password"
                autoComplete="off"
                value={keyDraft}
                onChange={(e) => setKeyDraft(e.target.value)}
                placeholder={status.configured ? t('models.apiKeyPlaceholderKeep') : 'sk-…'}
                disabled={busy}
                className="w-full rounded-lg bg-input-bg border border-input-border px-3 py-2 text-sm text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50"
              />
            </label>
            {error && <p className="text-xs text-error-text">{error}</p>}
            <div className="flex flex-wrap gap-2">
              <button
                type="submit"
                disabled={busy || (status.key_required && !keyDraft.trim() && !status.configured && !isCustom)}
                className="px-3 py-1.5 rounded-lg bg-accent text-accent-text text-sm font-medium hover:opacity-90 disabled:opacity-50"
              >
                {saveBusy ? t('models.saving') : t('models.save')}
              </button>
              <button
                type="button"
                disabled={busy || !status.configured}
                onClick={() => void handleClear()}
                className="px-3 py-1.5 rounded-lg border border-input-border text-sm text-t-text-secondary hover:bg-canvas-alt disabled:opacity-40"
              >
                {clearBusy ? t('models.clearing') : isCustom ? t('models.customRemove') : t('models.clearKey')}
              </button>
              <button
                type="button"
                disabled={busy || !status.configured || status.active}
                onClick={() => void handleActivate()}
                className="px-3 py-1.5 rounded-lg border border-accent/40 text-sm text-accent hover:bg-accent-soft disabled:opacity-40"
              >
                {activateBusy ? t('models.activating') : t('models.activate')}
              </button>
              {(status.id === 'ollama' || status.configured) && (
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void handleProbe()}
                  className="px-3 py-1.5 rounded-lg border border-input-border text-sm text-t-text-secondary hover:bg-canvas-alt disabled:opacity-40"
                >
                  {probeBusy ? t('models.probing') : t('models.probeService')}
                </button>
              )}
            </div>
          </form>

          {probeResult && (
            <p
              className={`text-xs ${probeResult.ok ? 'text-emerald-500/90' : 'text-amber-500/90'}`}
              role="status"
            >
              {probeResult.message}
            </p>
          )}

          {status.id === 'openrouter' && status.configured && (
            <OpenRouterModelPicker
              currentModel={status.model}
              disabled={busy}
              onModelChanged={onRefresh}
            />
          )}

          {status.id === 'sensenova' && status.configured && (
            <SenseNovaModelPicker
              currentModel={status.model}
              disabled={busy}
              onModelChanged={onRefresh}
            />
          )}

          {status.id !== 'deepseek' && status.configured && !isCustom && (
            <p className="text-[10px] text-t-text-muted leading-relaxed">{t('models.freeProviderHint')}</p>
          )}
          {isCustom && status.configured && (
            <p className="text-[10px] text-t-text-muted leading-relaxed">{t('models.customProviderHint')}</p>
          )}
        </div>
      )}
    </div>
  );
}
