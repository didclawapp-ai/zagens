import { useEffect, useMemo, useRef, useState } from 'react';
import DiffCard from '../DiffCard';
import { useT } from '../../i18n';
import {
  entryLabel,
  extractDiffEntries,
  type DiffEntry,
} from '../../lib/diff/diffEntries';
import type { ToolCardModel } from '../ToolCard';

interface Message {
  id: string;
  tools?: ToolCardModel[];
}

interface Props {
  messages: Message[];
  /** First diff in the turn — parent switches to workspace / Diff tab */
  onDetected?: () => void;
  active: boolean;
}

type OutputFormat = 'side-by-side' | 'line-by-line';

export default function DiffPanel({ messages, onDetected, active }: Props) {
  const { t } = useT();
  const firedRef = useRef(false);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [outputFormat, setOutputFormat] = useState<OutputFormat>('side-by-side');

  const entries = useMemo(() => extractDiffEntries(messages), [messages]);

  const selected: DiffEntry | null =
    entries.find((e) => e.id === selectedId) ?? entries[entries.length - 1] ?? null;

  useEffect(() => {
    if (entries.length === 0) {
      setSelectedId(null);
      return;
    }
    setSelectedId((prev) => {
      if (prev && entries.some((e) => e.id === prev)) return prev;
      return entries[entries.length - 1]?.id ?? null;
    });
  }, [entries]);

  useEffect(() => {
    if (!active || entries.length === 0 || firedRef.current) return;
    firedRef.current = true;
    onDetected?.();
  }, [active, entries.length, onDetected]);

  useEffect(() => {
    if (entries.length === 0) {
      firedRef.current = false;
    }
  }, [entries.length]);

  if (entries.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-xs text-t-text-muted">
        <p>{t('diff.empty')}</p>
        <p className="max-w-[16rem] text-[11px] leading-relaxed opacity-80">{t('diff.emptyHint')}</p>
      </div>
    );
  }

  return (
    <div className="diff-panel flex min-h-0 flex-1 flex-col">
      <div className="shrink-0 flex flex-wrap items-center gap-2 border-b border-divider bg-canvas-alt px-3 py-2">
        <span className="text-[10px] font-medium uppercase tracking-wide text-t-text-muted">
          {t('diff.count', { count: String(entries.length) })}
        </span>
        <div className="ml-auto flex rounded-md border border-divider overflow-hidden text-[10px]">
          <button
            type="button"
            className={`px-2 py-1 transition-colors ${
              outputFormat === 'side-by-side'
                ? 'bg-hover text-accent'
                : 'text-t-text-muted hover:bg-hover'
            }`}
            onClick={() => setOutputFormat('side-by-side')}
          >
            {t('diff.sideBySide')}
          </button>
          <button
            type="button"
            className={`px-2 py-1 border-l border-divider transition-colors ${
              outputFormat === 'line-by-line'
                ? 'bg-hover text-accent'
                : 'text-t-text-muted hover:bg-hover'
            }`}
            onClick={() => setOutputFormat('line-by-line')}
          >
            {t('diff.lineByLine')}
          </button>
        </div>
      </div>

      <div className="shrink-0 max-h-[28%] overflow-y-auto border-b border-divider bg-canvas">
        <ul className="p-1.5 space-y-0.5" role="listbox" aria-label={t('diff.listLabel')}>
          {entries.map((e) => {
            const isSel = e.id === selected?.id;
            return (
              <li key={e.id}>
                <button
                  type="button"
                  role="option"
                  aria-selected={isSel}
                  className={`w-full rounded-md px-2.5 py-1.5 text-left text-[11px] font-mono transition-colors ${
                    isSel
                      ? 'bg-accent-soft text-accent'
                      : 'text-t-text-secondary hover:bg-hover'
                  }`}
                  onClick={() => setSelectedId(e.id)}
                >
                  <span className="block truncate">{entryLabel(e)}</span>
                  <span className="block truncate text-[10px] opacity-70">{e.toolName}</span>
                </button>
              </li>
            );
          })}
        </ul>
      </div>

      <div className="min-h-0 flex-1 overflow-hidden p-2">
        {selected ? (
          <DiffCard
            key={selected.id}
            diffText={selected.diffText}
            fileName={selected.fileName}
            outputFormat={outputFormat}
            variant="panel"
          />
        ) : null}
      </div>
    </div>
  );
}
