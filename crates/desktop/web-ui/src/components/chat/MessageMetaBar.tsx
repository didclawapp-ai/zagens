import type { ReactNode } from 'react';
import CopyTextButton from '../CopyTextButton';
import { IconChevronRight } from '../icons/FlatIcons';

export function MessageMetaBar({
  icon,
  label,
  hint,
  expanded,
  onToggle,
  copyText,
  copyTitle,
  copyDisabled,
  children,
}: {
  icon: ReactNode;
  label: string;
  hint?: string;
  expanded: boolean;
  onToggle: () => void;
  copyText?: string;
  copyTitle?: string;
  copyDisabled?: boolean;
  children?: ReactNode;
}) {
  return (
    <div className="message-meta-section">
      <button
        type="button"
        onClick={onToggle}
        className="message-meta-bar group"
        aria-expanded={expanded}
      >
        <IconChevronRight
          className={`message-meta-chevron size-3.5 shrink-0 text-t-text-muted ${
            expanded ? 'message-meta-chevron--open' : ''
          }`}
        />
        <span className="message-meta-icon shrink-0 text-t-text-muted">{icon}</span>
        <span className="min-w-0 truncate font-medium text-t-text-secondary">{label}</span>
        {copyText != null && (
          <CopyTextButton
            getText={() => copyText}
            title={copyTitle ?? ''}
            disabled={copyDisabled ?? !copyText.trim()}
            className="ml-0.5 opacity-0 transition-opacity group-hover:opacity-100"
          />
        )}
        {hint && !expanded ? (
          <span className="message-meta-hint ml-auto truncate">{hint}</span>
        ) : null}
      </button>
      {children ? (
        <div
          className={`message-meta-panel ${expanded ? 'message-meta-panel--open' : ''}`}
          aria-hidden={!expanded}
        >
          <div className="message-meta-panel-inner">
            <div className="message-meta-body">{children}</div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
