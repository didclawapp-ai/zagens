import type { RightPanelView } from '../components/RightPanel';

export type SettingsNavTab = Extract<
  RightPanelView,
  | 'settings'
  | 'models'
  | 'mcp'
  | 'skills'
  | 'routing'
  | 'topic-memory'
  | 'index'
  | 'sandbox'
  | 'lht-settings'
  | 'hooks'
  | 'schedule'
  | 'about'
>;

export interface SettingsNavItem {
  tab: SettingsNavTab;
  label: string;
  show: boolean;
}

/** Shared settings sub-nav — used by icon-rail popover menu. */
export function buildSettingsNavItems(args: {
  t: (key: string) => string;
  desktopHost: boolean;
}): SettingsNavItem[] {
  const { t, desktopHost } = args;
  return [
    { tab: 'settings', label: t('panels.settings'), show: true },
    { tab: 'models', label: t('sidebar.models'), show: desktopHost },
    { tab: 'mcp', label: t('panels.mcp'), show: true },
    { tab: 'skills', label: t('sidebar.skills'), show: true },
    { tab: 'routing', label: t('panels.routing'), show: true },
    { tab: 'topic-memory', label: t('sidebar.topicMemory'), show: true },
    { tab: 'index', label: t('panels.index'), show: true },
    { tab: 'sandbox', label: t('panels.sandbox'), show: true },
    { tab: 'lht-settings', label: t('panels.lhtSettings'), show: true },
    { tab: 'hooks', label: t('sidebar.hooks'), show: desktopHost },
    { tab: 'schedule', label: t('sidebar.schedule'), show: true },
    { tab: 'about', label: t('sidebar.about'), show: true },
  ];
}

export function isSettingsNavActive(activeInspector: RightPanelView, items: SettingsNavItem[]): boolean {
  return activeInspector === 'settings' || items.some(({ tab }) => tab === activeInspector);
}
