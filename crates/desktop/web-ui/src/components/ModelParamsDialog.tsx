import { useEffect, useId, useRef, useState, type KeyboardEvent } from 'react';
import { useT } from '../i18n';

interface Props {
  open: boolean;
  onClose: () => void;
  onApply: (params: ModelParams) => void;
  initial: ModelParams;
}

export interface ModelParams {
  temperature: number;
  topP: number;
  maxTokens: number;
}

export default function ModelParamsDialog({ open, onClose, onApply, initial }: Props) {
  const { t } = useT();
  const titleId = useId();
  const panelRef = useRef<HTMLDivElement>(null);
  const [temperature, setTemperature] = useState(initial.temperature);
  const [topP, setTopP] = useState(initial.topP);
  const [maxTokens, setMaxTokens] = useState(initial.maxTokens);

  useEffect(() => {
    if (!open) {
      return;
    }
    setTemperature(initial.temperature);
    setTopP(initial.topP);
    setMaxTokens(initial.maxTokens);
  }, [open, initial.temperature, initial.topP, initial.maxTokens]);

  useEffect(() => {
    if (!open) {
      return;
    }
    const first = panelRef.current?.querySelector<HTMLElement>(
      'input:not([disabled]), button:not([disabled])',
    );
    first?.focus();
  }, [open]);

  if (!open) {
    return null;
  }

  const onKeyDown = (e: KeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      onClose();
    }
  };

  return (
    <div
      className="fixed inset-0 bg-overlay flex items-center justify-center z-50"
      onClick={(e) => {
        if (e.target === e.currentTarget) {
          onClose();
        }
      }}
      onKeyDown={onKeyDown}
    >
      <div
        ref={panelRef}
        className="bg-card border border-card-border rounded-2xl p-6 min-w-[340px] shadow-lg"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onKeyDown={onKeyDown}
      >
        <h3 id={titleId} className="text-base font-semibold mb-5">
          {t('modelParams.title')}
        </h3>

        <div className="space-y-4">
          <div>
            <label htmlFor="model-params-temperature" className="block text-xs text-t-text-secondary mb-1">
              {t('modelParams.temperature')}
            </label>
            <div className="flex items-center gap-3">
              <input
                id="model-params-temperature"
                type="range"
                min="0"
                max="2"
                step="0.1"
                value={temperature}
                onChange={(e) => setTemperature(Number(e.target.value))}
                className="flex-1 accent-current"
                style={{ accentColor: 'var(--accent)' }}
                aria-valuemin={0}
                aria-valuemax={2}
                aria-valuenow={temperature}
              />
              <span className="text-sm font-semibold text-accent w-9 text-right" aria-hidden="true">
                {temperature.toFixed(1)}
              </span>
            </div>
          </div>

          <div>
            <label htmlFor="model-params-top-p" className="block text-xs text-t-text-secondary mb-1">
              {t('modelParams.topP')}
            </label>
            <div className="flex items-center gap-3">
              <input
                id="model-params-top-p"
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={topP}
                onChange={(e) => setTopP(Number(e.target.value))}
                className="flex-1"
                style={{ accentColor: 'var(--accent)' }}
                aria-valuemin={0}
                aria-valuemax={1}
                aria-valuenow={topP}
              />
              <span className="text-sm font-semibold text-accent w-9 text-right" aria-hidden="true">
                {topP.toFixed(2)}
              </span>
            </div>
          </div>

          <div>
            <label htmlFor="model-params-max-tokens" className="block text-xs text-t-text-secondary mb-1">
              {t('modelParams.maxTokens')}
            </label>
            <input
              id="model-params-max-tokens"
              type="number"
              min={256}
              max={65536}
              value={maxTokens}
              onChange={(e) => setMaxTokens(Number(e.target.value))}
              className="w-full px-3 py-2 rounded-lg bg-input-bg border border-input-border text-sm text-t-text outline-none focus:border-accent"
            />
          </div>
        </div>

        <div className="flex justify-end gap-2 mt-6">
          <button
            type="button"
            onClick={onClose}
            className="px-4 py-2 rounded-lg text-sm text-t-text-secondary hover:bg-hover"
          >
            {t('modelParams.cancel')}
          </button>
          <button
            type="button"
            onClick={() => onApply({ temperature, topP, maxTokens })}
            className="px-4 py-2 rounded-lg text-sm font-medium bg-accent text-accent-text hover:opacity-90"
          >
            {t('modelParams.apply')}
          </button>
        </div>
      </div>
    </div>
  );
}
