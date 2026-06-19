import type { ReactNode } from 'react';

export type HarnessCardId = 'checklist' | 'audit' | 'lht' | 'agents';

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

  const HeadTag = onHeadClick ? 'button' : 'div';

  return (
    <article
      id={`harness-card-${cardId}`}
      className={`harness-card ${className}`.trim()}
      data-has-data="true"
    >
      <HeadTag
        type={onHeadClick ? 'button' : undefined}
        className="harness-card__head"
        onClick={onHeadClick}
      >
        <span className="harness-card__icon" aria-hidden>
          {icon}
        </span>
        <span className="harness-card__label">{label}</span>
        <span className="harness-card__stat">{stat}</span>
        {headExtra}
      </HeadTag>
      {children ? <div className="harness-card__body">{children}</div> : null}
    </article>
  );
}
