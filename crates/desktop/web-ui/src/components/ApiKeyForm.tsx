import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface Props {
  onSaved: () => void;
  className?: string;
}

export default function ApiKeyForm({ onSaved, className = '' }: Props) {
  const [key, setKey] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  return (
    <div className={className}>
      <p className="text-xs text-t-text-muted leading-relaxed">
        将写入用户目录下的 <code className="text-t-text-secondary bg-canvas-alt px-1 py-0.5 rounded text-[11px] font-mono">.deepseek/config.toml</code>
        （与 CLI/TUI 共用）。保存后运行时侧载会重启以生效。
      </p>
      <form onSubmit={(e) => void handleSubmit(e)} className="mt-4 space-y-3">
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
    </div>
  );
}
