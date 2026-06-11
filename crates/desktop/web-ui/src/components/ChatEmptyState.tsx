import { useT } from '../i18n';

/** Minimal new-session placeholder — single line above the composer (Codex-style). */
export function ChatEmptyState() {
  const { t } = useT();

  return (
    <div className="flex min-h-[min(50vh,24rem)] items-center justify-center px-6">
      <p className="text-center text-2xl font-medium tracking-tight text-t-text-secondary">
        {t('chatEmpty.title')}
      </p>
    </div>
  );
}
