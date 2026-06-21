import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n';
import type { SenseNovaModelEntry, SenseNovaModelList } from '../types/modelProviders';
import { setSenseNovaOutputLimits } from '../lib/modelParams';

interface Props {
  currentModel: string | null;
  disabled?: boolean;
  onModelChanged: () => void;
}

export default function SenseNovaModelPicker({
  currentModel,
  disabled = false,
  onModelChanged,
}: Props) {
  const { t } = useT();
  const [list, setList] = useState<SenseNovaModelList | null>(null);
  const [loadBusy, setLoadBusy] = useState(false);
  const [selectBusyId, setSelectBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState('');

  const load = useCallback(async () => {
    setError(null);
    setLoadBusy(true);
    try {
      const data = await invoke<SenseNovaModelList>('list_sensenova_models');
      setList(data);
      const limits: Record<string, number> = {};
      for (const m of data.models) {
        if (m.max_output_length != null && m.max_output_length > 0) {
          limits[m.id] = m.max_output_length;
        }
      }
      setSenseNovaOutputLimits(limits);
    } catch (err) {
      setList(null);
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoadBusy(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const handleSelect = async (modelId: string) => {
    if (disabled || selectBusyId) return;
    setError(null);
    setSelectBusyId(modelId);
    try {
      await invoke('set_sensenova_model', { modelId });
      onModelChanged();
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSelectBusyId(null);
    }
  };

  const activeModel = list?.current_model ?? currentModel;

  const filtered = useMemo(() => {
    const models = list?.models ?? [];
    const q = filter.trim().toLowerCase();
    if (!q) return models;
    return models.filter(
      (m) =>
        m.id.toLowerCase().includes(q) ||
        m.name.toLowerCase().includes(q) ||
        (m.description?.toLowerCase().includes(q) ?? false),
    );
  }, [filter, list?.models]);

  return (
    <div className="space-y-2 border-t border-divider pt-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-[11px] font-medium text-t-text-secondary">{t('models.sensenovaModels')}</p>
        <button
          type="button"
          disabled={disabled || loadBusy}
          onClick={() => void load()}
          className="text-[11px] text-accent hover:underline disabled:opacity-50"
        >
          {loadBusy ? t('models.sensenovaRefreshing') : t('models.sensenovaRefresh')}
        </button>
      </div>

      <input
        type="search"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        placeholder={t('models.sensenovaSearch')}
        disabled={disabled || loadBusy}
        className="w-full rounded-md border border-input-border bg-input-bg px-2 py-1.5 text-xs text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50"
      />

      {error && <p className="text-xs text-error-text">{error}</p>}

      {loadBusy && !list && (
        <p className="text-xs text-t-text-muted">{t('models.sensenovaLoading')}</p>
      )}

      {list && (
        <div
          className="max-h-48 overflow-y-auto rounded-md border border-input-border bg-input-bg/50 p-1"
          role="listbox"
          aria-label={t('models.sensenovaModels')}
        >
          {filtered.length === 0 ? (
            <p className="px-2 py-1.5 text-xs text-t-text-muted">{t('models.sensenovaNoMatch')}</p>
          ) : (
            filtered.map((m) => (
              <ModelRow
                key={m.id}
                model={m}
                selected={activeModel === m.id}
                busy={selectBusyId === m.id}
                disabled={disabled || selectBusyId !== null}
                onSelect={() => void handleSelect(m.id)}
              />
            ))
          )}
        </div>
      )}
    </div>
  );
}

function ModelRow({
  model,
  selected,
  busy,
  disabled,
  onSelect,
}: {
  model: SenseNovaModelEntry;
  selected: boolean;
  busy: boolean;
  disabled: boolean;
  onSelect: () => void;
}) {
  const { t } = useT();
  const contextLabel =
    model.context_length != null
      ? t('models.sensenovaContext', { tokens: String(model.context_length) })
      : null;

  return (
    <button
      type="button"
      role="option"
      aria-selected={selected}
      disabled={disabled}
      onClick={onSelect}
      className={`flex w-full flex-col gap-0.5 rounded px-2 py-1.5 text-left transition-colors ${
        selected ? 'bg-accent-soft text-accent' : 'text-t-text hover:bg-hover'
      } disabled:opacity-50`}
    >
      <span className="flex items-center justify-between gap-2">
        <span className="min-w-0 truncate text-xs font-medium" title={model.id}>
          {model.name}
        </span>
        <span className="flex shrink-0 items-center gap-1.5 text-[10px]">
          {selected && <span>{t('models.sensenovaCurrent')}</span>}
          {busy && <span className="text-t-text-muted">…</span>}
        </span>
      </span>
      {(model.description || contextLabel) && (
        <span className="line-clamp-2 text-[10px] text-t-text-muted leading-snug">
          {[contextLabel, model.description].filter(Boolean).join(' · ')}
        </span>
      )}
    </button>
  );
}
