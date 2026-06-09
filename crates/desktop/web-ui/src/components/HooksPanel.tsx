import { useCallback, useEffect, useState } from 'react';
import { useT } from '../i18n';
import {
  fetchHooksSettings,
  saveHooksSettings,
  type HookConditionSettings,
  type HookEntrySettings,
  type HooksSettings,
} from '../api/client';
import { confirmDialog } from '../lib/confirmDialog';

const HOOK_EVENTS = [
  'session_start',
  'session_end',
  'message_submit',
  'tool_call_before',
  'tool_call_after',
  'mode_change',
  'on_error',
  'shell_env',
  'pre_compact',
  'post_compact',
  'subagent_start',
  'subagent_end',
] as const;

const HOOK_CONDITION_TYPES = [
  'always',
  'tool_name',
  'tool_name_regex',
  'tool_category',
  'mode',
  'exit_code',
  'all',
  'any',
] as const;

type HookConditionType = (typeof HOOK_CONDITION_TYPES)[number];

function emptyCondition(): HookConditionSettings {
  return { type: 'always', value: null, conditions: null };
}

function normalizeCondition(
  condition?: HookConditionSettings | null,
): HookConditionSettings {
  const type = (condition?.type?.trim() || 'always') as HookConditionType;
  if (type === 'all' || type === 'any') {
    const subs = (condition?.conditions ?? []).map(normalizeCondition);
    return { type, value: null, conditions: subs.length > 0 ? subs : [emptyCondition()] };
  }
  if (!HOOK_CONDITION_TYPES.includes(type)) {
    return emptyCondition();
  }
  if (type === 'always') {
    return emptyCondition();
  }
  return {
    type,
    value: condition?.value?.trim() || '',
    conditions: null,
  };
}

interface ConditionEditorProps {
  condition: HookConditionSettings;
  onChange: (next: HookConditionSettings) => void;
  labelCls: string;
  inputCls: string;
  depth?: number;
}

function HookConditionEditor({
  condition,
  onChange,
  labelCls,
  inputCls,
  depth = 0,
}: ConditionEditorProps) {
  const { t } = useT();
  const normalized = normalizeCondition(condition);
  const isComposite = normalized.type === 'all' || normalized.type === 'any';

  const updateSub = (index: number, next: HookConditionSettings) => {
    const subs = [...(normalized.conditions ?? [])];
    subs[index] = next;
    onChange({ ...normalized, conditions: subs });
  };

  const addSub = () => {
    onChange({
      ...normalized,
      conditions: [...(normalized.conditions ?? []), emptyCondition()],
    });
  };

  const removeSub = (index: number) => {
    const subs = (normalized.conditions ?? []).filter((_, i) => i !== index);
    onChange({
      ...normalized,
      conditions: subs.length > 0 ? subs : [emptyCondition()],
    });
  };

  return (
    <div className={depth > 0 ? 'pl-2 border-l border-divider space-y-2' : 'space-y-2'}>
      <div className="grid grid-cols-2 gap-2">
        <label className={`${labelCls} flex flex-col gap-1`}>
          {t('hooks.conditionType')}
          <select
            value={normalized.type}
            onChange={(e) => {
              const type = e.target.value as HookConditionType;
              if (type === 'all' || type === 'any') {
                onChange({ type, value: null, conditions: [emptyCondition()] });
              } else if (type === 'always') {
                onChange(emptyCondition());
              } else {
                onChange({ type, value: '', conditions: null });
              }
            }}
            className={inputCls}
          >
            {HOOK_CONDITION_TYPES.map((ct) => (
              <option key={ct} value={ct}>
                {t(`hooks.conditionTypes.${ct}` as 'hooks.conditionTypes.always')}
              </option>
            ))}
          </select>
        </label>
        {!isComposite && normalized.type !== 'always' ? (
          <label className={`${labelCls} flex flex-col gap-1`}>
            {t('hooks.conditionValue')}
            <input
              type="text"
              value={normalized.value ?? ''}
              onChange={(e) =>
                onChange({ ...normalized, value: e.target.value, conditions: null })
              }
              placeholder={t('hooks.conditionValuePlaceholder')}
              className={inputCls}
            />
          </label>
        ) : !isComposite ? (
          <div className="text-[10px] text-t-text-muted self-end pb-2">
            {t('hooks.conditionAlwaysHint')}
          </div>
        ) : null}
      </div>
      {isComposite ? (
        <div className="space-y-2">
          <div className="flex items-center justify-between gap-2">
            <span className="text-[10px] text-t-text-muted">
              {t('hooks.compositeSubconditions')}
            </span>
            <button
              type="button"
              onClick={addSub}
              className="text-[10px] text-accent hover:underline"
            >
              {t('hooks.addSubcondition')}
            </button>
          </div>
          {(normalized.conditions ?? []).map((sub, subIndex) => (
            <div key={subIndex} className="rounded border border-divider p-2 space-y-1">
              <div className="flex justify-end">
                <button
                  type="button"
                  onClick={() => removeSub(subIndex)}
                  className="text-[10px] text-t-error hover:underline"
                >
                  {t('hooks.removeSubcondition')}
                </button>
              </div>
              <HookConditionEditor
                condition={sub}
                onChange={(next) => updateSub(subIndex, next)}
                labelCls={labelCls}
                inputCls={inputCls}
                depth={depth + 1}
              />
            </div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function emptyHook(): HookEntrySettings {
  return {
    event: 'session_start',
    command: '',
    name: null,
    timeout_secs: 30,
    background: false,
    continue_on_error: true,
    condition: emptyCondition(),
  };
}

interface Props {
  desktopHost: boolean;
  streaming?: boolean;
}

export default function HooksPanel({ desktopHost, streaming = false }: Props) {
  const { t } = useT();
  const [settings, setSettings] = useState<HooksSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saveSuccess, setSaveSuccess] = useState(false);

  useEffect(() => {
    if (!desktopHost) {
      setLoading(false);
      return;
    }
    let cancelled = false;
    fetchHooksSettings()
      .then((s) => {
        if (!cancelled) {
          setSettings({
            ...s,
            hooks: s.hooks.map((h) => ({
              ...h,
              condition: normalizeCondition(h.condition),
            })),
          });
          setLoadError(null);
        }
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setLoadError(err instanceof Error ? err.message : String(err));
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [desktopHost]);

  const update = useCallback(<K extends keyof HooksSettings>(key: K, value: HooksSettings[K]) => {
    setSettings((prev) => (prev ? { ...prev, [key]: value } : prev));
  }, []);

  const updateHook = useCallback((index: number, patch: Partial<HookEntrySettings>) => {
    setSettings((prev) => {
      if (!prev) return prev;
      const hooks = prev.hooks.map((h, i) => (i === index ? { ...h, ...patch } : h));
      return { ...prev, hooks };
    });
  }, []);

  const addHook = useCallback(() => {
    setSettings((prev) => (prev ? { ...prev, hooks: [...prev.hooks, emptyHook()] } : prev));
  }, []);

  const removeHook = useCallback((index: number) => {
    setSettings((prev) => {
      if (!prev) return prev;
      return { ...prev, hooks: prev.hooks.filter((_, i) => i !== index) };
    });
  }, []);

  const handleSave = useCallback(async () => {
    if (!settings || !desktopHost) return;
    if (streaming && !(await confirmDialog(t('settings.saveRestartsSidecar')))) {
      return;
    }
    setSaving(true);
    setSaveError(null);
    setSaveSuccess(false);
    try {
      await saveHooksSettings(settings);
      setSaveSuccess(true);
      setTimeout(() => setSaveSuccess(false), 3000);
    } catch (err: unknown) {
      setSaveError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }, [settings, desktopHost, streaming, t]);

  const labelCls = 'text-[11px] font-medium text-t-text-secondary';
  const inputCls =
    'w-full rounded-lg border border-divider bg-canvas px-3 py-2 text-xs text-t-text focus:outline-none focus:ring-1 focus:ring-accent font-mono';

  if (!desktopHost) {
    return (
      <div className="p-4 text-xs text-t-text-muted text-center">
        {t('hooks.notDesktop')}
      </div>
    );
  }

  if (loading) {
    return <div className="p-4 text-xs text-t-text-muted text-center">{t('hooks.loading')}</div>;
  }

  if (!settings) {
    return (
      <div className="p-4 text-xs text-error-text text-center">
        {loadError ?? t('hooks.loadFailedGeneric')}
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full overflow-hidden">
      <div className="overflow-y-auto px-4 py-3 flex-1 min-h-0 space-y-4">
        <p className="text-[11px] text-t-text-muted leading-relaxed">{t('hooks.intro')}</p>

        <label className="inline-flex items-center gap-2 text-xs text-t-text cursor-pointer">
          <input
            type="checkbox"
            checked={settings.enabled}
            onChange={(e) => update('enabled', e.target.checked)}
          />
          {t('hooks.enabled')}
        </label>

        <div className="grid grid-cols-2 gap-3">
          <label className={`${labelCls} flex flex-col gap-1`}>
            {t('hooks.defaultTimeout')}
            <input
              type="number"
              min={1}
              value={settings.default_timeout_secs ?? ''}
              onChange={(e) => {
                const raw = e.target.value.trim();
                update('default_timeout_secs', raw ? Number(raw) : null);
              }}
              placeholder="30"
              className={inputCls}
            />
          </label>
          <label className={`${labelCls} flex flex-col gap-1`}>
            {t('hooks.workingDir')}
            <input
              type="text"
              value={settings.working_dir ?? ''}
              onChange={(e) => update('working_dir', e.target.value.trim() || null)}
              placeholder={t('hooks.workingDirPlaceholder')}
              className={inputCls}
            />
          </label>
        </div>

        <div className="space-y-2">
          <div className="flex items-center justify-between gap-2">
            <span className="text-xs font-semibold text-t-text">{t('hooks.entriesTitle')}</span>
            <button
              type="button"
              onClick={addHook}
              className="px-2.5 py-1 text-[11px] font-medium rounded-md border border-card-border bg-canvas-alt hover:bg-hover text-t-text"
            >
              {t('hooks.addHook')}
            </button>
          </div>

          {settings.hooks.length === 0 ? (
            <p className="text-xs text-t-text-muted text-center py-4">{t('hooks.noHooks')}</p>
          ) : (
            settings.hooks.map((hook, index) => (
              <div
                key={index}
                className="rounded-lg border border-card-border bg-canvas-alt p-3 space-y-2"
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="text-[11px] font-medium text-t-text">
                    {hook.name?.trim() || t('hooks.unnamedHook', { index: String(index + 1) })}
                  </span>
                  <button
                    type="button"
                    onClick={() => void removeHook(index)}
                    className="text-[10px] text-t-error hover:underline"
                  >
                    {t('hooks.removeHook')}
                  </button>
                </div>
                <div className="grid grid-cols-2 gap-2">
                  <label className={`${labelCls} flex flex-col gap-1`}>
                    {t('hooks.event')}
                    <select
                      value={hook.event}
                      onChange={(e) => updateHook(index, { event: e.target.value })}
                      className={inputCls}
                    >
                      {HOOK_EVENTS.map((ev) => (
                        <option key={ev} value={ev}>
                          {t(`hooks.events.${ev}` as 'hooks.events.session_start')}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className={`${labelCls} flex flex-col gap-1`}>
                    {t('hooks.nameOptional')}
                    <input
                      type="text"
                      value={hook.name ?? ''}
                      onChange={(e) =>
                        updateHook(index, { name: e.target.value.trim() || null })
                      }
                      className={inputCls}
                    />
                  </label>
                </div>
                <label className={`${labelCls} flex flex-col gap-1`}>
                  {t('hooks.command')}
                  <textarea
                    value={hook.command}
                    onChange={(e) => updateHook(index, { command: e.target.value })}
                    rows={2}
                    className={`${inputCls} resize-y min-h-[48px]`}
                    placeholder={t('hooks.commandPlaceholder')}
                  />
                </label>
                <HookConditionEditor
                  condition={normalizeCondition(hook.condition)}
                  onChange={(condition) => updateHook(index, { condition })}
                  labelCls={labelCls}
                  inputCls={inputCls}
                />
                <div className="flex flex-wrap gap-x-4 gap-y-1 text-[10px] text-t-text">
                  <label className="inline-flex items-center gap-1.5 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={hook.background}
                      onChange={(e) => updateHook(index, { background: e.target.checked })}
                    />
                    {t('hooks.background')}
                  </label>
                  <label className="inline-flex items-center gap-1.5 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={hook.continue_on_error}
                      onChange={(e) =>
                        updateHook(index, { continue_on_error: e.target.checked })
                      }
                    />
                    {t('hooks.continueOnError')}
                  </label>
                  <label className="inline-flex items-center gap-1.5 text-t-text-muted">
                    {t('hooks.timeoutSecs')}
                    <input
                      type="number"
                      min={1}
                      value={hook.timeout_secs}
                      onChange={(e) =>
                        updateHook(index, {
                          timeout_secs: Math.max(1, Number(e.target.value) || 30),
                        })
                      }
                      className="w-16 rounded border border-card-border bg-canvas px-1.5 py-0.5 text-[10px]"
                    />
                  </label>
                </div>
              </div>
            ))
          )}
        </div>

        <p className="text-[10px] text-t-text-muted leading-relaxed">{t('hooks.configHint')}</p>
        <p className="text-[10px] text-t-text-muted leading-relaxed">{t('hooks.protocolHint')}</p>
      </div>

      <div className="shrink-0 border-t border-divider px-4 py-3 flex items-center justify-between gap-3">
        <div className="flex-1 min-w-0">
          {saveError && (
            <p className="text-xs text-error-text truncate" title={saveError}>
              {saveError}
            </p>
          )}
          {saveSuccess && !saveError && (
            <p className="text-xs text-emerald-500">{t('hooks.saveSuccess')}</p>
          )}
        </div>
        <button
          type="button"
          onClick={() => void handleSave()}
          disabled={saving}
          className="shrink-0 px-4 py-2 text-xs font-medium rounded-lg bg-accent text-white hover:opacity-90 disabled:opacity-40"
        >
          {saving ? t('hooks.saving') : t('hooks.save')}
        </button>
      </div>
    </div>
  );
}
