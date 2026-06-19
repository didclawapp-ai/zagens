import type { ReactNode } from 'react';

type Props = {
  title: string;
  summary?: string;
  expanded: boolean;
  onToggle: () => void;
  panelId: string;
  children: ReactNode;
};

export default function ComposerOverflowSection({
  title,
  summary,
  expanded,
  onToggle,
  panelId,
  children,
}: Props) {
  return (
    <div className="composer-overflow-section">
      <button
        type="button"
        className="composer-overflow-section__head"
        aria-expanded={expanded}
        aria-controls={panelId}
        onClick={onToggle}
      >
        <span className="composer-overflow-section__title">{title}</span>
        {summary ? (
          <span className="composer-overflow-section__summary" title={summary}>
            {summary}
          </span>
        ) : null}
        <svg viewBox="0 0 24 24" className="composer-overflow-section__chevron" aria-hidden>
          <path d="M6 9l6 6 6-6" />
        </svg>
      </button>
      {expanded ? (
        <div id={panelId} className="composer-overflow-section__body">
          {children}
        </div>
      ) : null}
    </div>
  );
}
