import { useMemo, useState } from 'react';

import {
  contextExplorerCategoryColor,
  type ContextCategory,
  type ContextUsageBreakdown,
} from '../lib/contextUsage';
import {
  contextCategoryNavLabelKey,
  isContextCategoryNavigable,
} from '../lib/contextExplorerNav';

function formatTokenCount(n: number): string {
  if (n >= 1_000_000) {
    return `${(n / 1_000_000).toFixed(2)}M`;
  }
  if (n >= 1_000) {
    return `${(n / 1_000).toFixed(1)}K`;
  }
  return String(n);
}

function normalizeBreakdown(raw: unknown): ContextUsageBreakdown | null {
  if (!raw || typeof raw !== 'object') {
    return null;
  }
  const o = raw as Record<string, unknown>;
  if (typeof o.estimated_input_tokens !== 'number') {
    return null;
  }
  const categoriesRaw = o.categories;
  const categories: ContextCategory[] = Array.isArray(categoriesRaw)
    ? categoriesRaw
        .filter((row): row is Record<string, unknown> => !!row && typeof row === 'object')
        .map((row) => ({
          id: String(row.id ?? ''),
          label: String(row.label ?? row.id ?? ''),
          tokens: Number(row.tokens ?? 0),
          item_count: typeof row.item_count === 'number' ? row.item_count : undefined,
          user_action_hint:
            typeof row.user_action_hint === 'string' ? row.user_action_hint : undefined,
        }))
    : [];
  return {
    model: String(o.model ?? ''),
    context_window_tokens: Number(o.context_window_tokens ?? 0),
    estimated_input_tokens: Number(o.estimated_input_tokens ?? 0),
    usage_percent: Number(o.usage_percent ?? 0),
    profile: String(o.profile ?? 'unknown'),
    next_action: String(o.next_action ?? 'none') as ContextUsageBreakdown['next_action'],
    categories,
  };
}

function CategoryRow({
  category,
  windowTokens,
  t,
  onNavigateCategory,
}: {
  category: ContextCategory;
  windowTokens: number;
  t: (k: string, vars?: Record<string, string>) => string;
  onNavigateCategory?: (categoryId: string) => void;
}) {
  const pct =
    windowTokens > 0 ? Math.min(100, (category.tokens / windowTokens) * 100) : 0;
  const color = contextExplorerCategoryColor(category.id);
  const navLabelKey = contextCategoryNavLabelKey(category.id);
  return (
    <li className="space-y-0.5">
      <div className="flex items-center justify-between gap-2">
        <span className="font-medium text-t-text">{category.label}</span>
        <span className="font-mono tabular-nums text-t-text-muted">
          {formatTokenCount(category.tokens)}
          {category.item_count != null
            ? ` · ${t('contextExplorer.itemCount', { n: String(category.item_count) })}`
            : ''}
        </span>
      </div>
      <div className="h-1.5 overflow-hidden rounded bg-t-border/30">
        <div
          className="h-full rounded"
          style={{ width: `${pct}%`, backgroundColor: color }}
        />
      </div>
      {navLabelKey && onNavigateCategory ? (
        <button
          type="button"
          className="text-[10px] font-medium text-sky-700 hover:underline dark:text-sky-300"
          onClick={() => onNavigateCategory(category.id)}
        >
          {t(navLabelKey)}
        </button>
      ) : category.user_action_hint ? (
        <p className="text-[10px] leading-snug text-t-text-muted">{category.user_action_hint}</p>
      ) : null}
    </li>
  );
}

export function ContextExplorerView({
  breakdown: rawBreakdown,
  t,
  compact = false,
  onNavigateCategory,
  onArchiveContext,
  archivePending = false,
  canArchiveContext = false,
}: {
  breakdown: unknown;
  t: (k: string, vars?: Record<string, string>) => string;
  /** Cycle tab / composer tooltip — header + bar only. */
  compact?: boolean;
  onNavigateCategory?: (categoryId: string) => void;
  onArchiveContext?: () => void;
  archivePending?: boolean;
  canArchiveContext?: boolean;
}) {
  const breakdown = useMemo(() => normalizeBreakdown(rawBreakdown), [rawBreakdown]);
  const [expanded, setExpanded] = useState(!compact);

  if (!breakdown) {
    return <p className="text-xs text-t-text-muted">{t('contextExplorer.empty')}</p>;
  }

  const pct = Math.round(breakdown.usage_percent);
  const windowTokens = breakdown.context_window_tokens;
  const usedTokens = breakdown.estimated_input_tokens;
  const segments = breakdown.categories.filter((c) => c.tokens > 0);

  return (
    <div className="space-y-3 text-xs text-t-text">
      <div className="space-y-1">
        <div className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 font-mono tabular-nums">
          <span className="truncate text-t-text" title={breakdown.model}>
            {breakdown.model || t('contextExplorer.unknownModel')}
          </span>
          <span className="text-t-text-muted">·</span>
          <span>
            {t('contextExplorer.windowLine', {
              n: formatTokenCount(windowTokens),
            })}
          </span>
          <span className="text-t-text-muted">·</span>
          <span>
            {t('contextExplorer.usedLine', {
              n: formatTokenCount(usedTokens),
            })}
          </span>
          <span className="text-t-text-muted">·</span>
          <span className="font-semibold">{pct}%</span>
        </div>
        <div
          className="flex h-2 overflow-hidden rounded bg-t-border/30"
          title={t('contextExplorer.segmentBarTitle')}
        >
          {segments.map((cat) => {
            const width =
              windowTokens > 0
                ? Math.max(0.5, (cat.tokens / windowTokens) * 100)
                : 0;
            return (
              <div
                key={cat.id}
                style={{
                  width: `${width}%`,
                  backgroundColor: contextExplorerCategoryColor(cat.id),
                }}
              />
            );
          })}
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <span className="rounded bg-canvas-alt px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-t-text-muted">
          {t('contextExplorer.profile', { profile: breakdown.profile })}
        </span>
        {breakdown.next_action !== 'none' ? (
          <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-[10px] text-amber-800 dark:text-amber-200">
            {t(`contextExplorer.nextAction.${breakdown.next_action}`)}
          </span>
        ) : null}
      </div>

      {canArchiveContext && onArchiveContext ? (
        <div className="space-y-1 border-t border-t-border/30 pt-2">
          <button
            type="button"
            className="rounded border border-t-border/60 px-2 py-1 text-[11px] font-medium text-t-text hover:bg-canvas-alt disabled:opacity-50"
            disabled={archivePending}
            onClick={() => void onArchiveContext()}
          >
            {archivePending
              ? t('contextExplorer.archivePending')
              : t('contextExplorer.archiveContext')}
          </button>
          <p className="text-[10px] leading-snug text-t-text-muted">
            {breakdown.profile === 'large'
              ? t('contextExplorer.archiveHintLarge')
              : t('contextExplorer.archiveHintMedium')}
          </p>
        </div>
      ) : null}

      {compact ? null : (
      <div>
        <button
          type="button"
          className="mb-1 text-[11px] font-medium text-t-text-muted hover:text-t-text"
          onClick={() => setExpanded((v) => !v)}
        >
          {expanded ? t('contextExplorer.hideCategories') : t('contextExplorer.showCategories')}
        </button>
        {expanded ? (
          <ul className="space-y-2">
            {segments.map((cat) => (
              <CategoryRow
                key={cat.id}
                category={cat}
                windowTokens={windowTokens}
                t={t}
                onNavigateCategory={
                  isContextCategoryNavigable(cat.id) ? onNavigateCategory : undefined
                }
              />
            ))}
          </ul>
        ) : null}
      </div>
      )}
    </div>
  );
}

export { normalizeBreakdown as normalizeContextUsageBreakdown };
