import { useCallback, useEffect, useState } from 'react';
import { fetchRoutingRules, setRoutingRules, type RuntimeConnectionState } from '../api/client';
import type { RoutingRule } from '../types/routing';

const PRESET_INTENTS = ['code', 'chat', 'research'];
const PRESET_MODELS = ['deepseek-v4-pro', 'deepseek-v4-flash'];

export default function RoutingPanel({ runtimeConn }: { runtimeConn: RuntimeConnectionState }) {
  const [rules, setRules] = useState<RoutingRule[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [newIntent, setNewIntent] = useState('');
  const [newModel, setNewModel] = useState('deepseek-v4-pro');

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetchRoutingRules();
      setRules(res.rules);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (runtimeConn === 'connected') {
      reload();
    }
  }, [runtimeConn, reload]);

  if (runtimeConn !== 'connected') {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center space-y-2">
        <p>等待运行时连接…</p>
        <p className="text-[10px]">路由规则将在运行时就绪后自动加载。</p>
      </div>
    );
  }

  const save = async (updated: RoutingRule[]) => {
    try {
      await setRoutingRules(updated);
      setRules(updated);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const addRule = () => {
    const intent = newIntent.trim();
    if (!intent || !newModel) return;
    if (rules.some((r) => r.intent.toLowerCase() === intent.toLowerCase())) {
      setError('该意图已存在');
      return;
    }
    save([...rules, { intent, model: newModel }]);
    setNewIntent('');
  };

  const removeRule = (intent: string) => {
    save(rules.filter((r) => r.intent !== intent));
  };

  if (loading && rules.length === 0) {
    return <div className="p-4 text-xs text-t-text-muted text-center">正在加载…</div>;
  }

  return (
    <div className="overflow-y-auto px-3 py-3 space-y-3">
      {/* Existing rules */}
      {rules.length === 0 && (
        <p className="text-xs text-t-text-muted text-center py-4">暂无路由规则。</p>
      )}
      {rules.map((r) => (
        <div key={r.intent} className="flex items-center gap-2 rounded-lg border border-card-border bg-canvas-alt px-3 py-2">
          <span className="px-2 py-0.5 rounded text-[10px] font-semibold bg-accent-soft text-accent">
            {r.intent}
          </span>
          <span className="text-[10px] text-t-text-muted">→</span>
          <span className="font-mono text-[11px] text-t-text-secondary flex-1">{r.model}</span>
          <button
            type="button"
            onClick={() => removeRule(r.intent)}
            className="text-[10px] text-t-text-muted hover:text-t-error px-1"
            title="删除规则"
          >
            ✕
          </button>
        </div>
      ))}

      {/* Add new rule */}
      <div className="border-t border-divider pt-3">
        <div className="text-[11px] font-medium text-t-text-secondary mb-2">添加规则</div>
        <div className="flex items-center gap-2 mb-2">
          <input
            type="text"
            list="intent-list"
            value={newIntent}
            onChange={(e) => setNewIntent(e.target.value)}
            placeholder="意图 (code/chat/research/…)"
            className="flex-1 px-2 py-1.5 text-xs rounded bg-input-bg border border-input-border text-t-text outline-none focus:border-accent"
            onKeyDown={(e) => e.key === 'Enter' && addRule()}
          />
          <datalist id="intent-list">
            {PRESET_INTENTS.map((i) => <option key={i} value={i} />)}
          </datalist>
          <span className="text-[10px] text-t-text-muted">→</span>
          <select
            value={newModel}
            onChange={(e) => setNewModel(e.target.value)}
            className="px-2 py-1.5 text-xs rounded bg-input-bg border border-input-border text-t-text"
          >
            {PRESET_MODELS.map((m) => (
              <option key={m} value={m}>{m}</option>
            ))}
          </select>
          <button
            type="button"
            onClick={addRule}
            className="px-3 py-1.5 text-xs font-medium rounded bg-accent text-accent-text hover:opacity-90"
          >
            添加
          </button>
        </div>
      </div>

      {error && (
        <p className="text-[10px] text-t-error">{error}</p>
      )}
    </div>
  );
}
