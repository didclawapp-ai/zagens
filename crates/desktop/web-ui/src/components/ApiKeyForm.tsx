import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

/** Defaults match `describe_image` / `DEFAULT_VISION_MODEL` (`deepseek-config`). */
const PLACEHOLDER_VISION_BASE = 'https://api.siliconflow.cn/v1';
const PLACEHOLDER_VISION_MODEL = 'Qwen/Qwen3-VL-32B-Instruct';

interface Props {
  onSaved: () => void;
  className?: string;
}

export default function ApiKeyForm({ onSaved, className = '' }: Props) {
  const [key, setKey] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await invoke('save_deepseek_api_key', { key: key.trim() });
      setKey('');
      onSaved();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

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
      onSaved();
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
      onSaved();
    } catch (err) {
      setVisionError(err instanceof Error ? err.message : String(err));
    } finally {
      setVisionBusy(false);
    }
  };

  return (
    <div className={className}>
      <p className="text-xs text-t-text-muted leading-relaxed">
        将写入用户目录下的{' '}
        <code className="text-t-text-secondary bg-canvas-alt px-1 py-0.5 rounded text-[11px] font-mono">
          .deepseek/config.toml
        </code>
        （与 CLI/TUI 共用）。保存后运行时侧载会重启以生效。
      </p>
      <form onSubmit={(e) => void handleSubmit(e)} className="mt-4 space-y-3">
        <p className="text-[11px] font-medium text-t-text-secondary">DeepSeek 主模型</p>
        <input
          type="password"
          autoComplete="off"
          value={key}
          onChange={(e) => setKey(e.target.value)}
          placeholder="sk-…"
          disabled={busy}
          className="w-full rounded-lg bg-input-bg border border-input-border px-3 py-2 text-sm text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50 transition-colors"
        />
        {error && <p className="text-xs text-error-text">{error}</p>}
        <button
          type="submit"
          disabled={busy || !key.trim()}
          className="w-full px-4 py-2 rounded-lg bg-accent text-accent-text hover:bg-accent-hover disabled:opacity-50 text-sm font-medium transition-colors"
        >
          {busy ? '保存中…' : '保存'}
        </button>
      </form>

      <hr className="my-5 border-border/60" />

      <div className="space-y-2">
        <p className="text-[11px] font-medium text-t-text-secondary">视觉桥接（describe_image 工具）</p>
        <p className="text-xs text-t-text-muted leading-relaxed">
          对应配置表{' '}
          <code className="text-t-text-secondary bg-canvas-alt px-1 py-0.5 rounded text-[11px] font-mono">
            [vision]
          </code>
          。留空端点/模型时使用运行时默认（硅基流动：`Qwen/Qwen3-VL-32B-Instruct`；仍可选用 `deepseek-ai/DeepSeek-OCR` 等）。密钥保存后不会回显；若已保存过密钥，可只改端点/模型（不填密钥则保留原密钥）。
        </p>
        {visionConfigured && (
          <p className="text-xs text-emerald-400/90">已检测到已保存的视觉桥接 API Key。</p>
        )}
      </div>
      <form onSubmit={(e) => void handleVisionSubmit(e)} className="mt-3 space-y-3">
        <input
          type="password"
          autoComplete="off"
          value={visionKey}
          onChange={(e) => setVisionKey(e.target.value)}
          placeholder={
            visionConfigured ? '留空则保留已保存的密钥；输入以覆盖' : '视觉服务商 API Key（如 SiliconFlow）'
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
              visionConfigured
                ? '保存端点/模型；如需更换密钥请填写新密钥'
                : '首次请填写视觉 API Key（如 SiliconFlow）'
            }
          >
            {visionBusy ? '保存中…' : '保存视觉桥接'}
          </button>
          <button
            type="button"
            disabled={visionBusy || !visionConfigured}
            onClick={() => void handleClearVision()}
            className="px-3 py-2 rounded-lg border border-input-border text-sm text-t-text-secondary hover:bg-canvas-alt disabled:opacity-40 transition-colors"
          >
            清除
          </button>
        </div>
      </form>
    </div>
  );
}
