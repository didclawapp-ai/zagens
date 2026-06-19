import { useEffect, useId, useRef, type ReactNode } from 'react';

export type OverflowMenuProps = {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  disabled?: boolean;
  triggerTitle: string;
  triggerAriaLabel: string;
  menuAriaLabel?: string;
  align?: 'start' | 'end';
  panelClassName?: string;
  children: ReactNode;
};

export default function OverflowMenu({
  open,
  onOpenChange,
  disabled = false,
  triggerTitle,
  triggerAriaLabel,
  menuAriaLabel,
  align = 'end',
  panelClassName = 'w-64',
  children,
}: OverflowMenuProps) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const menuId = useId();

  useEffect(() => {
    if (!open) {
      return;
    }
    const handler = (event: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(event.target as Node)) {
        onOpenChange(false);
      }
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, [open, onOpenChange]);

  return (
    <div className="relative" ref={wrapRef}>
      <button
        type="button"
        className="composer-icon-btn"
        disabled={disabled}
        onClick={() => onOpenChange(!open)}
        aria-expanded={open}
        aria-haspopup="menu"
        aria-controls={menuId}
        title={triggerTitle}
        aria-label={triggerAriaLabel}
      >
        <svg viewBox="0 0 24 24" aria-hidden>
          <circle cx="12" cy="6" r="1.5" fill="currentColor" stroke="none" />
          <circle cx="12" cy="12" r="1.5" fill="currentColor" stroke="none" />
          <circle cx="12" cy="18" r="1.5" fill="currentColor" stroke="none" />
        </svg>
      </button>
      {open ? (
        <div
          id={menuId}
          className={`absolute bottom-full z-[10040] mb-1 max-h-[min(70vh,calc(100vh-6rem))] overflow-y-auto rounded-lg border border-card-border bg-card p-1 shadow-lg ring-1 ring-black/[0.06] dark:ring-white/[0.08] ${align === 'end' ? 'right-0' : 'left-0'} ${panelClassName}`}
          role="menu"
          aria-label={menuAriaLabel ?? triggerAriaLabel}
        >
          {children}
        </div>
      ) : null}
    </div>
  );
}
