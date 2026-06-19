import type { RightPanelView } from '../components/RightPanel';

export type SettingsNavTab = Extract<
  RightPanelView,
  | 'settings'
  | 'api-key'
  | 'mcp'
  | 'skills'
  | 'routing'
  | 'topic-memory'
  | 'index'
  | 'system'
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
  officeSession: boolean;
}): SettingsNavItem[] {
  const { t, desktopHost, officeSession } = args;
  return [
    { tab: 'settings', label: t('panels.settings'), show: true },
    { tab: 'api-key', label: t('sidebar.apiKey'), show: desktopHost },
    { tab: 'mcp', label: t('panels.mcp'), show: true },
    { tab: 'skills', label: t('sidebar.skills'), show: true },
    { tab: 'routing', label: t('panels.routing'), show: !officeSession },
    { tab: 'topic-memory', label: t('sidebar.topicMemory'), show: !officeSession },
    { tab: 'index', label: t('panels.index'), show: !officeSession },
    { tab: 'system', label: t('panels.system'), show: true },
    { tab: 'sandbox', label: t('panels.sandbox'), show: true },
    { tab: 'lht-settings', label: t('panels.lhtSettings'), show: !officeSession },
    { tab: 'hooks', label: t('sidebar.hooks'), show: desktopHost },
    { tab: 'schedule', label: t('sidebar.schedule'), show: !officeSession },
    { tab: 'about', label: t('sidebar.about'), show: true },
  ];
}

export function isSettingsNavActive(activeInspector: RightPanelView, items: SettingsNavItem[]): boolean {
  return activeInspector === 'settings' || items.some(({ tab }) => tab === activeInspector);
}
