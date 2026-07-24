import type { ReactNode } from 'react';

export type HarnessCardId = 'checklist' | 'audit' | 'lht' | 'agents' | 'changes';

export type HarnessCardProps = {
  cardId: HarnessCardId;
  label: string;
  stat: ReactNode;
  icon: ReactNode;
  hasData: boolean;
  onHeadClick?: () => void;
  headExtra?: ReactNode;
  children?: ReactNode;
  className?: string;
};

/** Single harness summary card shell — returns null when hasData is false. */
export default function HarnessCard({
  cardId,
  label,
  stat,
  icon,
  hasData,
  onHeadClick,
  headExtra,
  children,
  className = '',
}: HarnessCardProps) {
  if (!hasData) {
    return null;
  }

  const interactive = Boolean(onHeadClick);

  return (
    <article
      id={`harness-card-${cardId}`}
      className={`harness-card ${interactive ? 'harness-card--interactive' : ''} ${className}`.trim()}
      data-has-data="true"
      role={interactive ? 'button' : undefined}
      tabIndex={interactive ? 0 : undefined}
      onClick={onHeadClick}
      onKeyDown={
        interactive
          ? (e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onHeadClick?.();
              }
            }
          : undefined
      }
    >
      <div className="harness-card__head">
        <span className="harness-card__icon" aria-hidden>
          {icon}
        </span>
        <span className="harness-card__label">{label}</span>
        <span className="harness-card__stat">{stat}</span>
        {headExtra}
      </div>
      {children ? <div className="harness-card__body">{children}</div> : null}
    </article>
  );
}
