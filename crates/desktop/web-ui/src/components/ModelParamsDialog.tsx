import { useState } from 'react';

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
  const [temperature, setTemperature] = useState(initial.temperature);
  const [topP, setTopP] = useState(initial.topP);
  const [maxTokens, setMaxTokens] = useState(initial.maxTokens);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 bg-overlay flex items-center justify-center z-50"
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div className="bg-card border border-card-border rounded-2xl p-6 min-w-[340px] shadow-lg">
        <h3 className="text-base font-semibold mb-5">⚙️ 模型参数</h3>

        <div className="space-y-4">
          <div>
            <label className="block text-xs text-t-text-secondary mb-1">Temperature</label>
            <div className="flex items-center gap-3">
              <input
                type="range"
                min="0"
                max="2"
                step="0.1"
                value={temperature}
                onChange={(e) => setTemperature(Number(e.target.value))}
                className="flex-1 accent-current"
                style={{ accentColor: 'var(--accent)' }}
              />
              <span className="text-sm font-semibold text-accent w-9 text-right">{temperature.toFixed(1)}</span>
            </div>
          </div>

          <div>
            <label className="block text-xs text-t-text-secondary mb-1">Top P</label>
            <div className="flex items-center gap-3">
              <input
                type="range"
                min="0"
                max="1"
                step="0.05"
                value={topP}
                onChange={(e) => setTopP(Number(e.target.value))}
                className="flex-1"
                style={{ accentColor: 'var(--accent)' }}
              />
              <span className="text-sm font-semibold text-accent w-9 text-right">{topP.toFixed(2)}</span>
            </div>
          </div>

          <div>
            <label className="block text-xs text-t-text-secondary mb-1">Max Tokens</label>
            <input
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
            取消
          </button>
          <button
            type="button"
            onClick={() => onApply({ temperature, topP, maxTokens })}
            className="px-4 py-2 rounded-lg text-sm font-medium bg-accent text-accent-text hover:opacity-90"
          >
            应用
          </button>
        </div>
      </div>
    </div>
  );
}
