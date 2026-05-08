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
      <p className="text-xs text-gray-500">
        将写入用户目录下的 <code className="text-gray-400">.deepseek/config.toml</code>
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
          className="w-full rounded-lg bg-gray-950 border border-gray-600 px-3 py-2 text-sm text-gray-100 placeholder-gray-600 focus:border-indigo-500 focus:outline-none disabled:opacity-50"
        />
        {error && <p className="text-xs text-red-400">{error}</p>}
        <button
          type="submit"
          disabled={busy || !key.trim()}
          className="w-full px-4 py-2 rounded-lg bg-indigo-600 text-white hover:bg-indigo-500 disabled:opacity-50 text-sm"
        >
          {busy ? '保存中…' : '保存'}
        </button>
      </form>
    </div>
  );
}
