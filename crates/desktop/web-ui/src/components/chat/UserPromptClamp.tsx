import { useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { useT } from '../../i18n';

/** ~3–4 lines of text-sm / leading-relaxed. */
const CLAMP_MAX_CLASS = 'max-h-[4.5rem]';

/**
 * Clamp long user prompts to a short preview with a bottom fade; expand on demand.
 */
export function UserPromptClamp({ children }: { children: ReactNode }) {
  const { t } = useT();
  const bodyRef = useRef<HTMLDivElement>(null);
  const [expanded, setExpanded] = useState(false);
  const [overflows, setOverflows] = useState(false);

  useLayoutEffect(() => {
    if (expanded) return;
    const el = bodyRef.current;
    if (!el) return;
    const measure = () => {
      setOverflows(el.scrollHeight > el.clientHeight + 2);
    };
    measure();
    const ro = typeof ResizeObserver !== 'undefined' ? new ResizeObserver(measure) : null;
    ro?.observe(el);
    return () => ro?.disconnect();
  }, [expanded, children]);

  return (
    <div className="user-prompt-clamp relative">
      <div
        ref={bodyRef}
        className={`break-words text-sm leading-relaxed text-t-text ${
          expanded ? '' : `${CLAMP_MAX_CLASS} overflow-hidden`
        }`}
      >
        {children}
      </div>
      {!expanded && overflows ? (
        <>
          <div
            className="user-prompt-clamp-fade pointer-events-none absolute inset-x-0 bottom-0 h-10"
            aria-hidden
          />
          <button
            type="button"
            className="relative z-[1] mt-1 text-[11px] font-medium text-accent hover:underline"
            onClick={() => setExpanded(true)}
          >
            {t('message.expandPrompt')}
          </button>
        </>
      ) : null}
      {expanded && overflows ? (
        <button
          type="button"
          className="mt-1 text-[11px] font-medium text-accent hover:underline"
          onClick={() => setExpanded(false)}
        >
          {t('message.collapsePrompt')}
        </button>
      ) : null}
    </div>
  );
}
