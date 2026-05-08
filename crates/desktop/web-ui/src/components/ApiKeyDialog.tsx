import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface Props {
  open: boolean;
  onClose: () => void;
  onSaved: () => void;
}

export default function ApiKeyDialog({ open, onClose, onSaved }: Props) {
  const [key, setKey] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setKey('');
      setError(null);
    }
  }, [open]);

  if (!open) {
    return null;
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await invoke('save_deepseek_api_key', { key: key.trim() });
      setKey('');
      onSaved();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 px-4">
      <div
        className="w-full max-w-md rounded-xl border border-indigo-700/40 bg-gray-900 shadow-2xl p-6"
        role="dialog"
        aria-modal="true"
        aria-labelledby="apikey-title"
      >
        <h2 id="apikey-title" className="text-lg font-semibold text-indigo-200">
          DeepSeek API Key
        </h2>
        <p className="mt-1 text-xs text-gray-500">
          将写入用户目录下的 <code className="text-gray-400">.deepseek/config.toml</code>
          （与 CLI/TUI 共用）。保存后新的对话回合会生效。
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
          <div className="flex justify-end gap-3 pt-1">
            <button
              type="button"
              disabled={busy}
              onClick={onClose}
              className="px-4 py-2 rounded-lg border border-gray-600 text-gray-200 hover:bg-gray-800 disabled:opacity-50 text-sm"
            >
              取消
            </button>
            <button
              type="submit"
              disabled={busy || !key.trim()}
              className="px-4 py-2 rounded-lg bg-indigo-600 text-white hover:bg-indigo-500 disabled:opacity-50 text-sm"
            >
              {busy ? '保存中…' : '保存'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
