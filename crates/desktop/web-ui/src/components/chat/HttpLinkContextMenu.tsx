import { useEffect } from 'react';
import { useT } from '../../i18n';

export interface HttpLinkMenuState {
  url: string;
  x: number;
  y: number;
}

interface Props {
  menu: HttpLinkMenuState;
  desktopHost: boolean;
  onClose: () => void;
  onOpenInApp: (url: string) => void;
  onOpenSystem: (url: string) => void;
}

export default function HttpLinkContextMenu({
  menu,
  desktopHost,
  onClose,
  onOpenInApp,
  onOpenSystem,
}: Props) {
  const { t } = useT();

  useEffect(() => {
    const dismiss = () => onClose();
    window.addEventListener('click', dismiss, { once: true });
    return () => window.removeEventListener('click', dismiss);
  }, [onClose]);

  const left = Math.min(menu.x, window.innerWidth - 220);
  const top = Math.min(menu.y, window.innerHeight - 120);
  const itemCls =
    'w-full text-left px-3 py-1.5 text-xs text-t-text hover:bg-hover transition-colors';

  return (
    <div
      className="fixed z-[10150] min-w-[180px] max-w-[min(280px,calc(100vw-1rem))] rounded-lg border border-divider bg-canvas py-1 shadow-lg"
      style={{ left, top }}
      role="menu"
      onClick={(e) => e.stopPropagation()}
      onContextMenu={(e) => e.preventDefault()}
    >
      <div
        className="px-3 py-1.5 text-[11px] font-medium text-t-text-muted truncate border-b border-divider"
        title={menu.url}
      >
        {menu.url}
      </div>
      {desktopHost ? (
        <button
          type="button"
          role="menuitem"
          className={itemCls}
          onClick={() => {
            onOpenInApp(menu.url);
            onClose();
          }}
        >
          {t('chatMarkdown.openInAppBrowser')}
        </button>
      ) : null}
      <button
        type="button"
        role="menuitem"
        className={itemCls}
        onClick={() => {
          onOpenSystem(menu.url);
          onClose();
        }}
      >
        {t('chatMarkdown.openSystemBrowser')}
      </button>
    </div>
  );
}
