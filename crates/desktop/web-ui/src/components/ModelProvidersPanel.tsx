import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n';
import CustomProviderAddForm from './CustomProviderAddForm';
import ModelProviderCard from './ModelProviderCard';
import VisionBridgeSection from './VisionBridgeSection';
import type { ModelProviderStatus } from '../types/modelProviders';

interface Props {
  onSaved: () => void;
  className?: string;
}

export default function ModelProvidersPanel({ onSaved, className = '' }: Props) {
  const { t } = useT();
  const [providers, setProviders] = useState<ModelProviderStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedId, setExpandedId] = useState<string | null>('deepseek');
  const [advancedOpen, setAdvancedOpen] = useState(false);

  const loadProviders = useCallback(async () => {
    try {
      const list = await invoke<ModelProviderStatus[]>('get_model_providers_status');
      setProviders(list);
    } catch {
      setProviders([]);
    } finally {
      setLoading(false);
    }
  }, []);

  /** User changed credentials/model — refresh panel and sync composer + key status. */
  const refreshAndNotify = useCallback(() => {
    void loadProviders().then(() => onSaved());
  }, [loadProviders, onSaved]);

  useEffect(() => {
    void loadProviders();
  }, [loadProviders]);

  const primary = useMemo(
    () => providers.filter((p) => p.section === 'primary'),
    [providers],
  );
  const free = useMemo(() => providers.filter((p) => p.section === 'free'), [providers]);
  const custom = useMemo(
    () => providers.filter((p) => p.section === 'custom'),
    [providers],
  );

  const toggleExpanded = (id: string) => {
    setExpandedId((cur) => (cur === id ? null : id));
  };

  if (loading) {
    return <p className={`text-sm text-t-text-muted ${className}`}>{t('models.loading')}</p>;
  }

  return (
    <div className={className}>
      <p className="text-xs text-t-text-muted leading-relaxed">{t('models.intro')}</p>

      <section className="mt-4 space-y-2">
        <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
          {t('models.primarySection')}
        </p>
        {primary.map((status) => (
          <ModelProviderCard
            key={status.id}
            status={status}
            expanded={expandedId === status.id}
            onToggle={() => toggleExpanded(status.id)}
            onRefresh={refreshAndNotify}
          />
        ))}
      </section>

      <section className="mt-5 space-y-2">
        <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
          {t('models.freeSection')}
        </p>
        {free.map((status) => (
          <ModelProviderCard
            key={status.id}
            status={status}
            expanded={expandedId === status.id}
            onToggle={() => toggleExpanded(status.id)}
            onRefresh={refreshAndNotify}
          />
        ))}
      </section>

      <section className="mt-5 space-y-2">
        <p className="text-[11px] font-semibold uppercase tracking-wider text-t-text-muted">
          {t('models.customSection')}
        </p>
        <p className="text-[10px] text-t-text-muted leading-relaxed">{t('models.customSectionHint')}</p>
        {custom.map((status) => (
          <ModelProviderCard
            key={status.id}
            status={status}
            expanded={expandedId === status.id}
            onToggle={() => toggleExpanded(status.id)}
            onRefresh={refreshAndNotify}
          />
        ))}
        <CustomProviderAddForm onAdded={refreshAndNotify} />
      </section>

      <section className="mt-6 border-t border-divider pt-4">
        <button
          type="button"
          className="flex w-full items-center justify-between text-[11px] font-semibold uppercase tracking-wider text-t-text-muted hover:text-t-text-secondary"
          onClick={() => setAdvancedOpen((o) => !o)}
          aria-expanded={advancedOpen}
        >
          {t('models.advancedSection')}
          <svg
            viewBox="0 0 24 24"
            className={`h-3.5 w-3.5 transition-transform ${advancedOpen ? 'rotate-180' : ''}`}
            aria-hidden
          >
            <path d="M6 9l6 6 6-6" fill="none" stroke="currentColor" strokeWidth="2" />
          </svg>
        </button>
        {advancedOpen && <VisionBridgeSection className="mt-3" onSaved={onSaved} />}
      </section>
    </div>
  );
}
