import { useEffect, useMemo, useRef, useState } from 'react';
import DiffCard from '../DiffCard';
import { useT } from '../../i18n';
import {
  entryLabel,
  extractDiffEntries,
  type DiffEntry,
} from '../../lib/diff/diffEntries';
import { normalizeWorkspaceRelPath } from '../../lib/openWorkspaceFile';
import { IconFolder } from '../icons/FlatIcons';
import type { ToolCardModel } from '../ToolCard';

interface Message {
  id: string;
  tools?: ToolCardModel[];
}

interface Props {
  messages: Message[];
  /** First diff in the turn — parent switches to workspace / Diff tab */
  onDetected?: () => void;
  /** Reveal a workspace-relative path in the Files tab (no preview). */
  onRevealInFiles?: (relPath: string) => void;
  active: boolean;
}

type OutputFormat = 'side-by-side' | 'line-by-line';

export default function DiffPanel({ messages, onDetected, onRevealInFiles, active }: Props) {
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
            const rel = normalizeWorkspaceRelPath(e.fileName);
            return (
              <li key={e.id} className="flex items-stretch gap-0.5">
                <button
                  type="button"
                  role="option"
                  aria-selected={isSel}
                  className={`min-w-0 flex-1 rounded-md px-2.5 py-1.5 text-left text-[11px] font-mono transition-colors ${
                    isSel
                      ? 'bg-accent-soft text-accent'
                      : 'text-t-text-secondary hover:bg-hover'
                  }`}
                  onClick={() => setSelectedId(e.id)}
                >
                  <span className="block truncate">{entryLabel(e)}</span>
                  <span className="block truncate text-[10px] opacity-70">{e.toolName}</span>
                </button>
                {onRevealInFiles && rel ? (
                  <button
                    type="button"
                    className="shrink-0 rounded-md px-1.5 text-t-text-muted hover:text-accent hover:bg-hover transition-colors"
                    title={t('diff.showInFiles')}
                    onClick={(ev) => {
                      ev.stopPropagation();
                      onRevealInFiles(rel);
                    }}
                  >
                    <IconFolder className="size-3.5" />
                  </button>
                ) : null}
              </li>
            );
          })}
        </ul>
      </div>

      <div className="min-h-0 flex-1 overflow-hidden p-2 flex flex-col gap-2">
        {selected && onRevealInFiles ? (
          <div className="shrink-0 flex justify-end">
            <button
              type="button"
              className="inline-flex items-center gap-1 rounded-md border border-divider px-2 py-1 text-[10px] text-t-text-secondary hover:text-accent hover:bg-hover transition-colors"
              onClick={() => {
                const rel = normalizeWorkspaceRelPath(selected.fileName);
                if (rel) onRevealInFiles(rel);
              }}
            >
              <IconFolder className="size-3" />
              {t('diff.showInFiles')}
            </button>
          </div>
        ) : null}
        <div className="min-h-0 flex-1 overflow-hidden">
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
    </div>
  );
}
