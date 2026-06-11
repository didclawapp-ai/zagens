/** Compact context-usage indicator — percent ring; details on hover via native title. */

type Level = '' | 'warn' | 'danger';

const RING_R = 9;
const CIRC = 2 * Math.PI * RING_R;

export function ComposerContextMeter({
  percent,
  level,
  tooltip,
  ariaLabel,
}: {
  percent: number;
  level: Level;
  tooltip: string;
  ariaLabel: string;
}) {
  const clamped = Math.min(100, Math.max(0, percent));
  const dash = (CIRC * clamped) / 100;
  const label = `${Math.round(clamped)}`;

  return (
    <button
      type="button"
      className={`composer-ctx-meter composer-ctx-meter--${level || 'ok'}`}
      title={tooltip}
      aria-label={ariaLabel}
    >
      <svg viewBox="0 0 24 24" className="composer-ctx-meter-svg" aria-hidden>
        <circle
          cx="12"
          cy="12"
          r={RING_R}
          fill="none"
          stroke="currentColor"
          strokeOpacity={0.18}
          strokeWidth="2"
        />
        <circle
          cx="12"
          cy="12"
          r={RING_R}
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeDasharray={`${dash} ${CIRC}`}
          transform="rotate(-90 12 12)"
        />
        <text
          x="12"
          y="12.5"
          textAnchor="middle"
          className="composer-ctx-meter-label"
          fontSize="6.5"
          fontWeight="600"
        >
          {label}
        </text>
      </svg>
    </button>
  );
}
