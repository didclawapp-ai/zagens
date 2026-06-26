type DiffLineStatsProps = {
  added: number;
  removed: number;
  className?: string;
};

/** Git-style +N / -M line counts beside a changed file name. */
export default function DiffLineStats({ added, removed, className = '' }: DiffLineStatsProps) {
  return (
    <span
      className={`shrink-0 font-mono text-[10px] tabular-nums tracking-tight ${className}`.trim()}
      aria-label={`+${added} -${removed}`}
    >
      <span className="text-success">+{added}</span>
      <span className="text-t-text-muted">{'  '}</span>
      <span className="text-t-error-text">-{removed}</span>
    </span>
  );
}
