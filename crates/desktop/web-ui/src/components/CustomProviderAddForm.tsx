import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n';

interface Props {
  disabled?: boolean;
  onAdded: () => void;
}

export default function CustomProviderAddForm({ disabled = false, onAdded }: Props) {
  const { t } = useT();
  const [expanded, setExpanded] = useState(false);
  const [displayName, setDisplayName] = useState('');
  const [baseUrl, setBaseUrl] = useState('https://');
  const [model, setModel] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [maxOutputTokensRaw, setMaxOutputTokensRaw] = useState('');
  const [setActive, setSetActive] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setBusy(true);
    const parsedMax = parseInt(maxOutputTokensRaw.trim(), 10);
    const maxOutputTokens = maxOutputTokensRaw.trim() && parsedMax > 0 ? parsedMax : null;
    try {
      await invoke('add_custom_model_provider', {
        displayName: displayName.trim(),
        baseUrl: baseUrl.trim(),
        apiKey: apiKey.trim(),
        model: model.trim(),
        maxOutputTokens,
        setActive,
      });
      setDisplayName('');
      setBaseUrl('https://');
      setModel('');
      setApiKey('');
      setMaxOutputTokensRaw('');
      setExpanded(false);
      onAdded();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const formDisabled = disabled || busy;
  const canSubmit =
    displayName.trim() &&
    baseUrl.trim().length > 'https://'.length &&
    model.trim() &&
    apiKey.trim();

  return (
    <div className="rounded-lg border border-dashed border-card-border bg-card/20 overflow-hidden">
      <button
        type="button"
        className="flex w-full items-center gap-2 px-3 py-2.5 text-left hover:bg-hover/40 transition-colors disabled:opacity-50"
        onClick={() => setExpanded((open) => !open)}
        disabled={disabled}
        aria-expanded={expanded}
      >
        <span className="min-w-0 flex-1 text-[11px] font-medium text-t-text-secondary">
          {t('models.customAddTitle')}
        </span>
        <svg
          viewBox="0 0 24 24"
          className={`h-4 w-4 shrink-0 text-t-text-muted transition-transform ${expanded ? 'rotate-180' : ''}`}
          aria-hidden
        >
          <path d="M6 9l6 6 6-6" fill="none" stroke="currentColor" strokeWidth="2" />
        </svg>
      </button>

      {expanded && (
        <form
          onSubmit={(e) => void handleSubmit(e)}
          className="border-t border-card-border/60 px-3 pb-3 pt-2 space-y-2"
        >
          <p className="text-[10px] text-t-text-muted leading-relaxed">{t('models.customAddHint')}</p>

          <label className="block space-y-1">
            <span className="text-[11px] text-t-text-secondary">{t('models.customDisplayName')}</span>
            <input
              type="text"
              value={displayName}
              onChange={(e) => setDisplayName(e.target.value)}
              placeholder={t('models.customDisplayNamePlaceholder')}
              disabled={formDisabled}
              className="w-full rounded-lg bg-input-bg border border-input-border px-3 py-2 text-sm text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50"
            />
          </label>

          <label className="block space-y-1">
            <span className="text-[11px] text-t-text-secondary">{t('models.customBaseUrl')}</span>
            <input
              type="url"
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="https://api.example.com/v1"
              disabled={formDisabled}
              className="w-full rounded-lg bg-input-bg border border-input-border px-3 py-2 text-sm text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50"
            />
          </label>

          <label className="block space-y-1">
            <span className="text-[11px] text-t-text-secondary">{t('models.customModelId')}</span>
            <input
              type="text"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder="deepseek-ai/DeepSeek-V3"
              disabled={formDisabled}
              className="w-full rounded-lg bg-input-bg border border-input-border px-3 py-2 text-sm text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50"
            />
          </label>

          <label className="block space-y-1">
            <span className="text-[11px] text-t-text-secondary">{t('models.apiKeyLabel')}</span>
            <input
              type="password"
              autoComplete="off"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-…"
              disabled={formDisabled}
              className="w-full rounded-lg bg-input-bg border border-input-border px-3 py-2 text-sm text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50"
            />
          </label>

          <label className="block space-y-1">
            <span className="text-[11px] text-t-text-secondary">{t('models.customMaxOutputTokens')}</span>
            <input
              type="number"
              min={1}
              max={1000000}
              value={maxOutputTokensRaw}
              onChange={(e) => setMaxOutputTokensRaw(e.target.value)}
              placeholder={t('models.customMaxOutputTokensPlaceholder')}
              disabled={formDisabled}
              className="w-full rounded-lg bg-input-bg border border-input-border px-3 py-2 text-sm text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50"
            />
          </label>

          <label className="flex items-center gap-2 text-[11px] text-t-text-secondary">
            <input
              type="checkbox"
              checked={setActive}
              onChange={(e) => setSetActive(e.target.checked)}
              disabled={formDisabled}
              className="rounded border-input-border"
            />
            {t('models.customSetActive')}
          </label>

          {error && <p className="text-xs text-error-text">{error}</p>}

          <button
            type="submit"
            disabled={formDisabled || !canSubmit}
            className="px-3 py-1.5 rounded-lg bg-accent text-accent-text text-sm font-medium hover:opacity-90 disabled:opacity-50"
          >
            {busy ? t('models.saving') : t('models.customAddSubmit')}
          </button>
        </form>
      )}
    </div>
  );
}
