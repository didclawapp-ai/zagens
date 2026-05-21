import { useCallback, useState, type MouseEvent } from 'react';
import { copyPlainText } from '../lib/copyPlainText';
import { useT } from '../i18n';

const COPY_SVG = (
  <svg className="size-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden>
    <rect x="9" y="9" width="13" height="13" rx="2" />
    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
  </svg>
);

const CHECK_SVG = (
  <svg className="size-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" aria-hidden>
    <path d="M20 6L9 17l-5-5" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

export default function CopyTextButton({
  getText,
  title,
  disabled = false,
  className = '',
}: {
  getText: () => string;
  title: string;
  disabled?: boolean;
  className?: string;
}) {
  const { t } = useT();
  const [copied, setCopied] = useState(false);

  const onClick = useCallback(
    async (e: MouseEvent<HTMLButtonElement>) => {
      e.stopPropagation();
      e.preventDefault();
      if (disabled) {
        return;
      }
      const text = getText().trim();
      if (!text) {
        return;
      }
      const ok = await copyPlainText(text);
      if (ok) {
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1500);
      }
    },
    [disabled, getText],
  );

  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      title={copied ? t('chatMarkdown.copied') : title}
      aria-label={copied ? t('chatMarkdown.copied') : title}
      className={`shrink-0 inline-flex items-center justify-center rounded p-1 text-t-text-muted transition-colors hover:bg-hover hover:text-t-text disabled:opacity-40 disabled:pointer-events-none ${className}`}
    >
      {copied ? CHECK_SVG : COPY_SVG}
    </button>
  );
}
