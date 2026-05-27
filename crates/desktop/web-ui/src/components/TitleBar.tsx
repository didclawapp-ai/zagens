import { useT } from '../i18n';

type Props = {
  desktopHost: boolean;
  onNewWindow: () => void;
};

export default function TitleBar({ desktopHost, onNewWindow }: Props) {
  const { t } = useT();
  const handleMinimize = () => {
    void import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().minimize());
  };
  const handleToggleMaximize = () => {
    void import('@tauri-apps/api/window').then(async ({ getCurrentWindow }) => {
      const w = getCurrentWindow();
      const max = await w.isMaximized();
      if (max) await w.unmaximize();
      else await w.maximize();
    });
  };
  const handleClose = () => {
    if (desktopHost) {
      void import('../lib/windowBridge').then(({ closeCurrentWindow }) => closeCurrentWindow());
      return;
    }
    void import('@tauri-apps/api/window').then(({ getCurrentWindow }) => getCurrentWindow().hide());
  };

  return (
    <div
      data-tauri-drag-region
      className="flex items-center h-9 shrink-0 bg-canvas select-none"
    >
      <div className="flex items-center gap-0.5 shrink-0 pl-2" data-tauri-drag-region="false">
        {desktopHost && (
          <button
            type="button"
            onClick={onNewWindow}
            className="px-2 py-1 text-xs text-t-text-muted hover:text-t-text hover:bg-hover rounded transition-colors"
            title={t('titlebar.newWindow')}
          >
            {t('titlebar.newWindow')}
          </button>
        )}
      </div>
      <div className="flex-1 min-w-8" data-tauri-drag-region />
      <button
        type="button"
        data-tauri-drag-region="false"
        onClick={handleMinimize}
        className="px-3 py-2 text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
        aria-label={t('titlebar.minimize')}
      >
        <svg viewBox="0 0 24 24" className="w-3.5 h-3.5 stroke-current" style={{ fill: 'none', strokeWidth: 1.6 }}>
          <path d="M5 12h14" />
        </svg>
      </button>
      <button
        type="button"
        data-tauri-drag-region="false"
        onClick={handleToggleMaximize}
        className="px-3 py-2 text-t-text-muted hover:text-t-text hover:bg-hover transition-colors"
        aria-label={t('titlebar.maximize')}
      >
        <svg viewBox="0 0 24 24" className="w-3.5 h-3.5 stroke-current" style={{ fill: 'none', strokeWidth: 1.6 }}>
          <path d="M4 4h16v16H4z" />
        </svg>
      </button>
      <button
        type="button"
        data-tauri-drag-region="false"
        onClick={handleClose}
        className="px-3 py-2 text-t-text-muted hover:text-white hover:bg-t-error transition-colors"
        aria-label={t('titlebar.close')}
      >
        <svg viewBox="0 0 24 24" className="w-3.5 h-3.5 stroke-current" style={{ fill: 'none', strokeWidth: 1.6 }}>
          <path d="M6 6l12 12M18 6L6 18" />
        </svg>
      </button>
    </div>
  );
}
