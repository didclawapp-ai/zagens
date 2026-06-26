import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n';
import { setCatalogOutputLimits } from '../lib/modelParams';
import type { CatalogModelEntry, CatalogModelList } from '../types/modelProviders';

interface Props {
  providerId: string;
  currentModel: string | null;
  disabled?: boolean;
  onModelChanged: () => void;
}

function filterModels(models: CatalogModelEntry[], filter: string): CatalogModelEntry[] {
  const q = filter.trim().toLowerCase();
  if (!q) return models;
  return models.filter(
    (m) =>
      m.id.toLowerCase().includes(q) ||
      m.name.toLowerCase().includes(q) ||
      (m.description?.toLowerCase().includes(q) ?? false),
  );
}

function ModelListSection({
  title,
  models,
  currentModel,
  busyId,
  onSelect,
  filter,
  showMeta,
}: {
  title: string;
  models: CatalogModelEntry[];
  currentModel: string | null;
  busyId: string | null;
  onSelect: (id: string) => void;
  filter: string;
  showMeta: boolean;
}) {
  const { t } = useT();
  const filtered = useMemo(() => filterModels(models, filter), [filter, models]);

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
          <p className="px-2 py-1.5 text-xs text-t-text-muted">{t('models.catalogNoMatch')}</p>
        ) : (
          filtered.map((m) => (
            <ModelRow
              key={m.id}
              model={m}
              selected={currentModel === m.id}
              busy={busyId === m.id}
              disabled={busyId !== null}
              showMeta={showMeta}
              onSelect={() => onSelect(m.id)}
            />
          ))
        )}
      </div>
    </div>
  );
}

function ModelRow({
  model,
  selected,
  busy,
  disabled,
  showMeta,
  onSelect,
}: {
  model: CatalogModelEntry;
  selected: boolean;
  busy: boolean;
  disabled: boolean;
  showMeta: boolean;
  onSelect: () => void;
}) {
  const { t } = useT();
  const meta: string[] = [];
  if (showMeta) {
    if (model.context_length != null) {
      meta.push(t('models.catalogContext', { tokens: model.context_length.toLocaleString() }));
    }
    if (model.max_output_length != null) {
      meta.push(t('models.catalogMaxOutput', { tokens: model.max_output_length.toLocaleString() }));
    }
    if (model.description) {
      meta.push(model.description);
    } else if (model.name !== model.id) {
      meta.push(model.id);
    }
  }

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
          {selected && <span>{t('models.catalogCurrent')}</span>}
          {busy && <span className="text-t-text-muted">…</span>}
        </span>
      </span>
      {showMeta && meta.length > 0 && (
        <span className="line-clamp-2 text-[10px] text-t-text-muted leading-snug">
          {meta.join(' · ')}
        </span>
      )}
    </button>
  );
}

export default function CatalogModelPicker({
  providerId,
  currentModel,
  disabled = false,
  onModelChanged,
}: Props) {
  const { t } = useT();
  const [list, setList] = useState<CatalogModelList | null>(null);
  const [loadBusy, setLoadBusy] = useState(false);
  const [selectBusyId, setSelectBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState('');

  const load = useCallback(async () => {
    setError(null);
    setLoadBusy(true);
    try {
      const data = await invoke<CatalogModelList>('list_catalog_models', { providerId });
      setList(data);
      setCatalogOutputLimits(data.output_limits ?? {});
    } catch (err) {
      setList(null);
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoadBusy(false);
    }
  }, [providerId]);

  useEffect(() => {
    void load();
  }, [load]);

  const handleSelect = async (modelId: string) => {
    if (disabled || selectBusyId) return;
    setError(null);
    setSelectBusyId(modelId);
    try {
      await invoke('set_catalog_model', { providerId, modelId });
      onModelChanged();
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSelectBusyId(null);
    }
  };

  const activeModel = list?.current_model ?? currentModel;
  const isFreePaid = list?.variant === 'free_paid';

  return (
    <div className="space-y-2 border-t border-divider pt-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-[11px] font-medium text-t-text-secondary">{t('models.catalogModels')}</p>
        <button
          type="button"
          disabled={disabled || loadBusy}
          onClick={() => void load()}
          className="text-[11px] text-accent hover:underline disabled:opacity-50"
        >
          {loadBusy ? t('models.catalogRefreshing') : t('models.catalogRefresh')}
        </button>
      </div>

      <input
        type="search"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        placeholder={t('models.catalogSearch')}
        disabled={disabled || loadBusy}
        className="w-full rounded-md border border-input-border bg-input-bg px-2 py-1.5 text-xs text-t-text placeholder-t-text-muted focus:border-accent focus:outline-none disabled:opacity-50"
      />

      {error && <p className="text-xs text-error-text">{error}</p>}

      {loadBusy && !list && (
        <p className="text-xs text-t-text-muted">{t('models.catalogLoading')}</p>
      )}

      {list && isFreePaid && (
        <div className="space-y-3">
          <ModelListSection
            title={t('models.catalogFree', { count: String(list.free?.length ?? 0) })}
            models={list.free ?? []}
            currentModel={activeModel}
            busyId={selectBusyId}
            onSelect={(id) => void handleSelect(id)}
            filter={filter}
            showMeta={false}
          />
          <ModelListSection
            title={t('models.catalogPaid', { count: String(list.paid?.length ?? 0) })}
            models={list.paid ?? []}
            currentModel={activeModel}
            busyId={selectBusyId}
            onSelect={(id) => void handleSelect(id)}
            filter={filter}
            showMeta={false}
          />
        </div>
      )}

      {list && !isFreePaid && (
        <div
          className="max-h-48 overflow-y-auto rounded-md border border-input-border bg-input-bg/50 p-1"
          role="listbox"
          aria-label={t('models.catalogModels')}
        >
          {filterModels(list.models, filter).length === 0 ? (
            <p className="px-2 py-1.5 text-xs text-t-text-muted">{t('models.catalogNoMatch')}</p>
          ) : (
            filterModels(list.models, filter).map((m) => (
              <ModelRow
                key={m.id}
                model={m}
                selected={activeModel === m.id}
                busy={selectBusyId === m.id}
                disabled={disabled || selectBusyId !== null}
                showMeta
                onSelect={() => void handleSelect(m.id)}
              />
            ))
          )}
        </div>
      )}
    </div>
  );
}
