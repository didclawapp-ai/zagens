import type { ReactNode } from 'react';

export type IconRailButtonProps = {
  label: string;
  active?: boolean;
  highlight?: boolean;
  expanded?: boolean;
  controls?: string;
  hasPopup?: boolean | 'menu';
  disabled?: boolean;
  onClick?: () => void;
  children: ReactNode;
  className?: string;
};

/** 52px icon rail control — stroke icon + CSS tooltip + focus ring. */
export default function IconRailButton({
  label,
  active = false,
  highlight = false,
  expanded,
  controls,
  hasPopup,
  disabled = false,
  onClick,
  children,
  className = '',
}: IconRailButtonProps) {
  return (
    <button
      type="button"
      className={`icon-rail-btn ${active ? 'icon-rail-btn--active' : ''} ${
        highlight ? 'icon-rail-btn--highlight' : ''
      } ${className}`.trim()}
      aria-label={label}
      title={label}
      data-tip={label}
      aria-expanded={expanded}
      aria-controls={controls}
      aria-haspopup={hasPopup}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

export function IconRailSvg({ children }: { children: ReactNode }) {
  return (
    <svg viewBox="0 0 24 24" className="icon-rail-btn__svg" aria-hidden>
      {children}
    </svg>
  );
}
