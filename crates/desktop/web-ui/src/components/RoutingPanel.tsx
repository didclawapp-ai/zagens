import { useCallback, useEffect, useState } from 'react';
import { fetchRoutingRules, setRoutingRules, type RuntimeConnectionState } from '../api/client';
import { useT } from '../i18n';
import { isRuntimeApiAvailable } from '../lib/runtimeReachable';
import type { RoutingRule } from '../types/routing';
import type { DesktopRouteIntentOption } from '../types/desktop';
import {
  ROUTE_INTENT_OPTIONS,
} from '../types/desktop';
import type { TranslationKey } from '../i18n/keys';

const ROUTE_INTENT_LABEL_KEYS: Record<DesktopRouteIntentOption, TranslationKey> = {
  off: 'routing.intentOff',
  follow_runmode: 'routing.intentFollowRunmode',
  code: 'routing.intentCode',
  chat: 'routing.intentChat',
  research: 'routing.intentResearch',
};

const ROUTE_INTENT_HINT_KEYS: Record<DesktopRouteIntentOption, TranslationKey> = {
  off: 'routing.intentOffHint',
  follow_runmode: 'routing.intentFollowRunmodeHint',
  code: 'routing.intentCodeHint',
  chat: 'routing.intentChatHint',
  research: 'routing.intentResearchHint',
};

const PRESET_INTENTS = ['plan', 'agent', 'yolo', 'code', 'chat', 'research'];
const PRESET_MODELS = ['deepseek-v4-pro', 'deepseek-v4-flash'];

interface Props {
  runtimeConn: RuntimeConnectionState;
  streaming?: boolean;
  runtimeSessionEstablished?: boolean;
  routeIntent: DesktopRouteIntentOption;
  onRouteIntentChange: (v: DesktopRouteIntentOption) => void;
}

export default function RoutingPanel({
  runtimeConn,
  streaming = false,
  runtimeSessionEstablished = false,
  routeIntent,
  onRouteIntentChange,
}: Props) {
  const runtimeReady = isRuntimeApiAvailable(runtimeConn, {
    streaming,
    sessionEstablished: runtimeSessionEstablished,
  });
  const { t } = useT();
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
    if (runtimeReady) {
      reload();
    }
  }, [runtimeReady, reload]);

  if (!runtimeReady) {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center space-y-2">
        <p>{t('routing.waitingRuntime')}</p>
        <p className="text-[10px]">{t('routing.waitingDetail')}</p>
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
      setError(t('routing.intentExists'));
      return;
    }
    save([...rules, { intent, model: newModel }]);
    setNewIntent('');
  };

  const removeRule = (intent: string) => {
    save(rules.filter((r) => r.intent !== intent));
  };

  return (
    <div className="overflow-y-auto px-3 py-3 space-y-4">
      <section className="space-y-2">
        <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
          {t('routing.strategyTitle')}
        </p>
        <p className="text-[11px] leading-snug text-t-text-secondary">{t('routing.strategyDesc')}</p>
        <div className="space-y-1" role="radiogroup" aria-label={t('routing.strategyTitle')}>
          {ROUTE_INTENT_OPTIONS.map((id) => (
            <label
              key={id}
              className={`flex cursor-pointer gap-2 rounded-lg border px-3 py-2 transition-colors ${
                routeIntent === id
                  ? 'border-accent/40 bg-accent-soft'
                  : 'border-card-border bg-canvas-alt hover:bg-hover'
              }`}
            >
              <input
                type="radio"
                name="route-strategy"
                className="mt-0.5 accent-accent"
                checked={routeIntent === id}
                onChange={() => onRouteIntentChange(id)}
              />
              <div className="min-w-0 flex-1">
                <span
                  className={`block text-xs font-medium ${
                    routeIntent === id ? 'text-accent' : 'text-t-text'
                  }`}
                >
                  {t(ROUTE_INTENT_LABEL_KEYS[id])}
                </span>
                <span className="mt-0.5 block text-[10px] leading-snug text-t-text-muted">
                  {t(ROUTE_INTENT_HINT_KEYS[id])}
                </span>
              </div>
            </label>
          ))}
        </div>
      </section>

      <section className="space-y-2 border-t border-divider pt-3">
        <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
          {t('routing.rulesTitle')}
        </p>
        {loading && rules.length === 0 ? (
          <p className="text-xs text-t-text-muted text-center py-3">{t('routing.loading')}</p>
        ) : null}
        {!loading && rules.length === 0 ? (
          <p className="text-xs text-t-text-muted text-center py-3">{t('routing.noRules')}</p>
        ) : null}
        {rules.map((r) => (
          <div
            key={r.intent}
            className="flex items-center gap-2 rounded-lg border border-card-border bg-canvas-alt px-3 py-2"
          >
            <span className="px-2 py-0.5 rounded text-[10px] font-semibold bg-accent-soft text-accent">
              {r.intent}
            </span>
            <span className="text-[10px] text-t-text-muted">→</span>
            <span className="font-mono text-[11px] text-t-text-secondary flex-1">{r.model}</span>
            <button
              type="button"
              onClick={() => removeRule(r.intent)}
              className="text-[10px] text-t-text-muted hover:text-t-error px-1"
              title={t('routing.deleteRule')}
            >
              ✕
            </button>
          </div>
        ))}

        <div className="border-t border-divider pt-3">
          <div className="text-[11px] font-medium text-t-text-secondary mb-2">{t('routing.addRule')}</div>
          <div className="flex items-center gap-2 mb-2">
            <input
              type="text"
              list="intent-list"
              value={newIntent}
              onChange={(e) => setNewIntent(e.target.value)}
              placeholder={t('routing.intentPlaceholder')}
              className="flex-1 px-2 py-1.5 text-xs rounded bg-input-bg border border-input-border text-t-text outline-none focus:border-accent"
              onKeyDown={(e) => e.key === 'Enter' && addRule()}
            />
            <datalist id="intent-list">
              {PRESET_INTENTS.map((i) => (
                <option key={i} value={i} />
              ))}
            </datalist>
            <span className="text-[10px] text-t-text-muted">→</span>
            <select
              value={newModel}
              onChange={(e) => setNewModel(e.target.value)}
              className="px-2 py-1.5 text-xs rounded bg-input-bg border border-input-border text-t-text"
            >
              {PRESET_MODELS.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
            <button
              type="button"
              onClick={addRule}
              className="px-3 py-1.5 text-xs font-medium rounded bg-accent text-accent-text hover:opacity-90"
            >
              {t('routing.add')}
            </button>
          </div>
        </div>
      </section>

      {error ? <p className="text-[10px] text-t-error">{error}</p> : null}
    </div>
  );
}
