import { useT } from '../i18n';

type Props = {
  onRetry: () => void;
};

/** Shown when Tauri is present but shell IPC failed (often disk full on system or user-data volume). */
export default function ShellLoadFailure({ onRetry }: Props) {
  const { t } = useT();
  return (
    <div className="flex min-h-screen flex-col items-center justify-center bg-t-bg px-6 text-center text-t-text">
      <h1 className="text-lg font-semibold">{t('storage.shellInitFailed')}</h1>
      <p className="mt-3 max-w-md text-sm text-t-text-muted">{t('storage.shellInitHint')}</p>
      <button
        type="button"
        className="mt-6 rounded-md bg-accent px-4 py-2 text-sm font-medium text-white hover:brightness-105"
        onClick={onRetry}
      >
        {t('common.retry')}
      </button>
    </div>
  );
}
