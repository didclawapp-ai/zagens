import { useEffect, useMemo, useRef, useState } from 'react';
import { useT } from '../../i18n';
import type { RightPanelView } from '../RightPanel';
import {
  buildSettingsNavItems,
  isSettingsNavActive,
  type SettingsNavTab,
} from '../../lib/settingsNavItems';
import IconRailButton, { IconRailSvg } from './IconRailButton';

export type SettingsRailMenuProps = {
  activeInspector: RightPanelView;
  onInspectorChange: (view: RightPanelView) => void;
  desktopHost: boolean;
  onExpandRightPanel?: () => void;
};

export default function SettingsRailMenu({
  activeInspector,
  onInspectorChange,
  desktopHost,
  onExpandRightPanel,
}: SettingsRailMenuProps) {
  const { t } = useT();
  const wrapRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const items = useMemo(
    () => buildSettingsNavItems({ t, desktopHost }).filter((item) => item.show),
    [t, desktopHost],
  );
  const settingsActive = isSettingsNavActive(activeInspector, items);

  useEffect(() => {
    if (!open) {
      return;
    }
    const onDocClick = (event: MouseEvent) => {
      if (!wrapRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('click', onDocClick);
    return () => document.removeEventListener('click', onDocClick);
  }, [open]);

  const handleSelect = (tab: SettingsNavTab) => {
    onExpandRightPanel?.();
    onInspectorChange(tab);
    setOpen(false);
  };

  return (
    <div className="icon-rail-menu-wrap" ref={wrapRef}>
      <IconRailButton
        label={t('sidebar.settings')}
        active={settingsActive || open}
        expanded={open}
        hasPopup="menu"
        controls="settings-rail-menu"
        onClick={() => setOpen((value) => !value)}
      >
        <IconRailSvg>
          <path d="M12.22 2h-.44a2 2 0 00-2 2v.18a2 2 0 01-1 1.73l-.43.25a2 2 0 01-2 0l-.15-.08a2 2 0 00-2.73.73l-.22.38a2 2 0 00.73 2.73l.15.1a2 2 0 011 1.72v.51a2 2 0 01-1 1.74l-.15.09a2 2 0 00-.73 2.73l.22.38a2 2 0 002.73.73l.15-.08a2 2 0 012 0l.43.25a2 2 0 011 1.73V20a2 2 0 002 2h.44a2 2 0 002-2v-.18a2 2 0 011-1.73l.43-.25a2 2 0 012 0l.15.08a2 2 0 002.73-.73l.22-.39a2 2 0 00-.73-2.73l-.15-.08a2 2 0 01-1-1.74v-.5a2 2 0 011-1.74l.15-.09a2 2 0 00.73-2.73l-.22-.38a2 2 0 00-2.73-.73l-.15.08a2 2 0 01-2 0l-.43-.25a2 2 0 01-1-1.73V4a2 2 0 00-2-2z" />
          <circle cx="12" cy="12" r="3" />
        </IconRailSvg>
      </IconRailButton>
      <div
        id="settings-rail-menu"
        className={`icon-rail-menu-pop ${open ? 'icon-rail-menu-pop--open' : ''}`}
        role="menu"
        aria-label={t('iconRail.settingsMenu')}
        hidden={!open}
      >
        <div className="icon-rail-menu-pop__head">{t('sidebar.settings')}</div>
        {items.map((item) => (
          <button
            key={item.tab}
            type="button"
            role="menuitem"
            className={`icon-rail-menu-item ${
              activeInspector === item.tab ? 'icon-rail-menu-item--active' : ''
            }`}
            onClick={() => handleSelect(item.tab)}
          >
            {item.label}
          </button>
        ))}
      </div>
    </div>
  );
}
