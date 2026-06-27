import { useEffect, useRef, useState } from 'react';
import { useT } from '../../i18n';
import type { Theme } from '../../lib/appPreferences';
import IconRailButton, { IconRailSvg } from './IconRailButton';

export type ThemeRailMenuProps = {
  theme: Theme;
  onThemeChange: (theme: Theme) => void;
};

export default function ThemeRailMenu({ theme, onThemeChange }: ThemeRailMenuProps) {
  const { t } = useT();
  const wrapRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);

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

  const selectTheme = (next: Theme) => {
    onThemeChange(next);
    setOpen(false);
  };

  return (
    <div className="icon-rail-menu-wrap" ref={wrapRef}>
      <IconRailButton
        label={t('iconRail.themeMenu')}
        active={open}
        expanded={open}
        hasPopup="menu"
        controls="theme-rail-menu"
        onClick={() => setOpen((value) => !value)}
      >
        {theme === 'light' ? (
          <IconRailSvg>
            <circle cx="12" cy="12" r="4" />
            <path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" />
          </IconRailSvg>
        ) : theme === 'dusk' ? (
          <IconRailSvg>
            <path d="M3 16h18M5 16a7 7 0 0114 0" />
            <path d="M12 3v3M4.2 7.2l1.5 1.5M19.8 7.2l-1.5 1.5M2 20h20" />
          </IconRailSvg>
        ) : (
          <IconRailSvg>
            <path d="M21 14.5A8.5 8.5 0 1112.5 3a6.5 6.5 0 009 11.5z" />
          </IconRailSvg>
        )}
      </IconRailButton>
      <div
        id="theme-rail-menu"
        className={`icon-rail-menu-pop ${open ? 'icon-rail-menu-pop--open' : ''}`}
        role="menu"
        aria-label={t('iconRail.themeMenu')}
        hidden={!open}
      >
        <div className="icon-rail-menu-pop__head">{t('settings.theme')}</div>
        {(['light', 'dark', 'dusk'] as const).map((id) => (
          <button
            key={id}
            type="button"
            role="menuitemradio"
            aria-checked={theme === id}
            className={`icon-rail-menu-item ${theme === id ? 'icon-rail-menu-item--active' : ''}`}
            onClick={() => selectTheme(id)}
          >
            {id === 'light'
              ? t('sidebar.themeLight')
              : id === 'dark'
                ? t('sidebar.themeDark')
                : t('sidebar.themeDusk')}
          </button>
        ))}
      </div>
    </div>
  );
}
