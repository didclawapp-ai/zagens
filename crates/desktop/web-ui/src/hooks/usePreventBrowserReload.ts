import { useEffect } from 'react';
import { useT } from '../i18n';
import { toast } from '../lib/toast';

function isBrowserReloadShortcut(e: KeyboardEvent): boolean {
  if (e.key === 'F5') {
    return true;
  }
  return (e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'r';
}

/**
 * Block browser full-page reload (F5 / Ctrl+R). Zagens keeps chat state in memory
 * until persist-session completes; accidental reload loses the active session.
 */
export function usePreventBrowserReload(): void {
  const { t } = useT();

  useEffect(() => {
    let lastToastAt = 0;

    const handler = (e: KeyboardEvent) => {
      if (!isBrowserReloadShortcut(e)) {
        return;
      }
      e.preventDefault();
      e.stopPropagation();
      const now = Date.now();
      if (now - lastToastAt > 3000) {
        lastToastAt = now;
        toast.info(t('banner.pageReloadBlocked'));
      }
    };

    window.addEventListener('keydown', handler, { capture: true });
    return () => window.removeEventListener('keydown', handler, { capture: true });
  }, [t]);
}
