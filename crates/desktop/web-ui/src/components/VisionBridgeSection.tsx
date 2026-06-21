/** Vision bridge (describe_image) — extracted from legacy ApiKeyForm. */

import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n';

const PLACEHOLDER_VISION_BASE = 'https://api.siliconflow.cn/v1';
const PLACEHOLDER_VISION_MODEL = 'Qwen/Qwen3-VL-32B-Instruct';

interface Props {
  onSaved?: () => void;
  className?: string;
}

export default function VisionBridgeSection({ onSaved, className = '' }: Props) {
  const { t } = useT();
  const [visionKey, setVisionKey] = useState('');
  const [visionBaseUrl, setVisionBaseUrl] = useState('');
  const [visionModel, setVisionModel] = useState('');
  const [visionConfigured, setVisionConfigured] = useState(false);
  const [visionBusy, setVisionBusy] = useState(false);
  const [visionError, setVisionError] = useState<string | null>(null);

  const refreshVision = useCallback(() => {
    void (async () => {
      try {
        const s = await invoke<{
          configured: boolean;
          base_url: string | null;
          model: string | null;
        }>('get_vision_bridge_status');
        setVisionConfigured(s.configured);
        setVisionBaseUrl(s.base_url ?? '');
        setVisionModel(s.model ?? '');
      } catch {
        setVisionConfigured(false);
        setVisionBaseUrl('');
        setVisionModel('');
      }
    })();
  }, []);

  useEffect(() => {
    refreshVision();
  }, [refreshVision]);

  const handleVisionSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setVisionError(null);
    setVisionBusy(true);
    try {
      await invoke('save_vision_bridge', {
        apiKey: visionKey.trim(),
        baseUrl: visionBaseUrl.trim(),
        model: visionModel.trim(),
      });
      setVisionKey('');
      refreshVision();
      onSaved?.();
    } catch (err) {
      setVisionError(err instanceof Error ? err.message : String(err));
    } finally {
      setVisionBusy(false);
    }
  };

  const handleClearVision = async () => {
    setVisionError(null);
    setVisionBusy(true);
    try {
      await invoke('clear_vision_bridge');
      setVisionKey('');
      refreshVision();
      onSaved?.();
    } catch (err) {
      setVisionError(err instanceof Error ? err.message : String(err));
    } finally {
      setVisionBusy(false);
    }
  };

  return (
    <div className={className}>
      <p className="text-[11px] font-medium text-t-text-secondary">{t('apiKey.visionBridge')}</p>
      <p className="mt-1 text-xs text-t-text-muted leading-relaxed">{t('apiKey.visionConfig')}</p>
      {visionConfigured && (
        <p className="mt-2 text-xs text-emerald-400/90">{t('apiKey.visionConfigured')}</p>
      )}
      <form onSubmit={(e) => void handleVisionSubmit(e)} className="mt-3 space-y-3">
        <input
          type="password"
          autoComplete="off"
          value={visionKey}
          onChange={(e) => setVisionKey(e.target.value)}
          placeholder={
            visionConfigured ? t('apiKey.visionKeyPlaceholderKeep') : t('apiKey.visionKeyPlaceholder')
          }
          disabled={visionBusy}
          className="w-full rounded-lg bg-input-bg border border-input-border px-3 py-2 text-sm text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50 transition-colors"
        />
        <input
          type="text"
          autoComplete="off"
          value={visionBaseUrl}
          onChange={(e) => setVisionBaseUrl(e.target.value)}
          placeholder={PLACEHOLDER_VISION_BASE}
          disabled={visionBusy}
          className="w-full rounded-lg bg-input-bg border border-input-border px-3 py-2 text-sm text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50 transition-colors"
        />
        <input
          type="text"
          autoComplete="off"
          value={visionModel}
          onChange={(e) => setVisionModel(e.target.value)}
          placeholder={PLACEHOLDER_VISION_MODEL}
          disabled={visionBusy}
          className="w-full rounded-lg bg-input-bg border border-input-border px-3 py-2 text-sm text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50 transition-colors"
        />
        {visionError && <p className="text-xs text-error-text">{visionError}</p>}
        <div className="flex gap-2">
          <button
            type="submit"
            disabled={visionBusy || (!visionConfigured && !visionKey.trim())}
            className="flex-1 px-4 py-2 rounded-lg bg-accent text-accent-text hover:bg-accent-hover disabled:opacity-50 text-sm font-medium transition-colors"
            title={
              visionConfigured ? t('apiKey.saveEndpointModel') : t('apiKey.fillVisionKey')
            }
          >
            {visionBusy ? t('apiKey.saving') : t('apiKey.saveVision')}
          </button>
          <button
            type="button"
            disabled={visionBusy || !visionConfigured}
            onClick={() => void handleClearVision()}
            className="px-3 py-2 rounded-lg border border-input-border text-sm text-t-text-secondary hover:bg-canvas-alt disabled:opacity-40 transition-colors"
          >
            {t('apiKey.clear')}
          </button>
        </div>
      </form>
    </div>
  );
}
