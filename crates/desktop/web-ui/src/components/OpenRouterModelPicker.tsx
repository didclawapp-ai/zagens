import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n';
import type { OpenRouterModelEntry, OpenRouterModelList } from '../types/modelProviders';

interface Props {
  currentModel: string | null;
  disabled?: boolean;
  onModelChanged: () => void;
}

function ModelListSection({
  title,
  models,
  currentModel,
  busyId,
  onSelect,
  filter,
}: {
  title: string;
  models: OpenRouterModelEntry[];
  currentModel: string | null;
  busyId: string | null;
  onSelect: (id: string) => void;
  filter: string;
}) {
  const { t } = useT();
  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return models;
    return models.filter(
      (m) => m.id.toLowerCase().includes(q) || m.name.toLowerCase().includes(q),
    );
  }, [filter, models]);

  if (models.length === 0) {
    return null;
  }

  return (
    <div className="space-y-1.5">
      <p className="text-[11px] font-medium text-t-text-secondary">{title}</p>
      <div
        className="max-h-40 overflow-y-auto rounded-md border border-input-border bg-input-bg/50 p-1"
        role="listbox"
        aria-label={title}
      >
        {filtered.length === 0 ? (
          <p className="px-2 py-1.5 text-xs text-t-text-muted">{t('models.openrouterNoMatch')}</p>
        ) : (
          filtered.map((m) => {
            const selected = currentModel === m.id;
            return (
              <button
                key={m.id}
                type="button"
                role="option"
                aria-selected={selected}
                disabled={busyId !== null}
                onClick={() => onSelect(m.id)}
                className={`flex w-full items-center justify-between gap-2 rounded px-2 py-1.5 text-left text-xs transition-colors ${
                  selected
                    ? 'bg-accent-soft text-accent font-medium'
                    : 'text-t-text hover:bg-hover'
                } disabled:opacity-50`}
              >
                <span className="min-w-0 truncate" title={m.id}>
                  {m.name}
                </span>
                {selected && (
                  <span className="shrink-0 text-[10px]">{t('models.openrouterCurrent')}</span>
                )}
                {busyId === m.id && (
                  <span className="shrink-0 text-[10px] text-t-text-muted">…</span>
                )}
              </button>
            );
          })
        )}
      </div>
    </div>
  );
}

export default function OpenRouterModelPicker({
  currentModel,
  disabled = false,
  onModelChanged,
}: Props) {
  const { t } = useT();
  const [list, setList] = useState<OpenRouterModelList | null>(null);
  const [loadBusy, setLoadBusy] = useState(false);
  const [selectBusyId, setSelectBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState('');

  const load = useCallback(async () => {
    setError(null);
    setLoadBusy(true);
    try {
      const data = await invoke<OpenRouterModelList>('list_openrouter_models');
      setList(data);
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
      await invoke('set_openrouter_model', { modelId });
      onModelChanged();
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSelectBusyId(null);
    }
  };

  const activeModel = list?.current_model ?? currentModel;

  return (
    <div className="space-y-2 border-t border-divider pt-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-[11px] font-medium text-t-text-secondary">{t('models.openrouterModels')}</p>
        <button
          type="button"
          disabled={disabled || loadBusy}
          onClick={() => void load()}
          className="text-[11px] text-accent hover:underline disabled:opacity-50"
        >
          {loadBusy ? t('models.openrouterRefreshing') : t('models.openrouterRefresh')}
        </button>
      </div>

      <input
        type="search"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        placeholder={t('models.openrouterSearch')}
        disabled={disabled || loadBusy}
        className="w-full rounded-md border border-input-border bg-input-bg px-2 py-1.5 text-xs text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50"
      />

      {error && <p className="text-xs text-error-text">{error}</p>}

      {loadBusy && !list && (
        <p className="text-xs text-t-text-muted">{t('models.openrouterLoading')}</p>
      )}

      {list && (
        <div className="space-y-3">
          <ModelListSection
            title={t('models.openrouterFree', { count: String(list.free.length) })}
            models={list.free}
            currentModel={activeModel}
            busyId={selectBusyId}
            onSelect={(id) => void handleSelect(id)}
            filter={filter}
          />
          <ModelListSection
            title={t('models.openrouterPaid', { count: String(list.paid.length) })}
            models={list.paid}
            currentModel={activeModel}
            busyId={selectBusyId}
            onSelect={(id) => void handleSelect(id)}
            filter={filter}
          />
        </div>
      )}
    </div>
  );
}
