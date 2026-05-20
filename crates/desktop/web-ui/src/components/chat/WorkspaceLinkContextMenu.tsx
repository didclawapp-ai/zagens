import { useEffect } from 'react';
import { useT } from '../../i18n';
import { isSystemOpenableFileName } from '../../lib/workspaceLinkMenu';

export interface WorkspaceLinkMenuState {
  relPath: string;
  absPath: string;
  fileName: string;
  x: number;
  y: number;
}

interface Props {
  menu: WorkspaceLinkMenuState;
  desktopHost: boolean;
  onClose: () => void;
  onOpenSystem: (absPath: string) => void;
}

export default function WorkspaceLinkContextMenu({
  menu,
  desktopHost,
  onClose,
  onOpenSystem,
}: Props) {
  const { t } = useT();
  const canSystemOpen = desktopHost && isSystemOpenableFileName(menu.fileName);

  useEffect(() => {
    const dismiss = () => onClose();
    window.addEventListener('click', dismiss, { once: true });
    return () => window.removeEventListener('click', dismiss);
  }, [onClose]);

  const left = Math.min(menu.x, window.innerWidth - 200);
  const top = Math.min(menu.y, window.innerHeight - 120);

  const itemCls =
    'w-full text-left px-3 py-1.5 text-xs text-t-text hover:bg-hover transition-colors';

  const copy = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      /* ignore */
    }
    onClose();
  };

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
        title={menu.relPath}
      >
        {menu.fileName}
      </div>
      {canSystemOpen ? (
        <button
          type="button"
          role="menuitem"
          className={itemCls}
          onClick={() => {
            onOpenSystem(menu.absPath);
            onClose();
          }}
        >
          {t('chatMarkdown.openWithSystemApp')}
        </button>
      ) : null}
      <button type="button" role="menuitem" className={itemCls} onClick={() => void copy(menu.absPath)}>
        {t('chatMarkdown.copyAbsPath')}
      </button>
      <button type="button" role="menuitem" className={itemCls} onClick={() => void copy(menu.relPath)}>
        {t('chatMarkdown.copyRelPath')}
      </button>
    </div>
  );
}
