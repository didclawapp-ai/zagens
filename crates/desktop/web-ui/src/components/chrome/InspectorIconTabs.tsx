import type { KeyboardEvent } from 'react';
import { useT } from '../../i18n';
import type { TranslationKey } from '../../i18n/keys';
import { handleTabListKeyDown } from '../../lib/a11y/rovingTabList';
import type { WorkspaceGitBadgeInfo } from '../../hooks/useWorkspaceGitStatus';
import type { WorkspaceTabId } from '../RightPanel';

const TAB_LABEL_KEYS: Record<WorkspaceTabId, TranslationKey> = {
  restore: 'workbench.tabRestore',
  files: 'workspaceFiles.tab',
  rules: 'workspaceRules.tab',
  terminal: 'terminal.tab',
  diff: 'diff.tab',
};

function WorkspaceTabIcon({ tab }: { tab: WorkspaceTabId }) {
  switch (tab) {
    case 'restore':
      return (
        <>
          <path d="M3 12a9 9 0 109-9 7.92 7.92 0 00-2.4-.36" />
          <path d="M3 3v5h5" />
        </>
      );
    case 'files':
      return (
        <>
          <path d="M4 6h16v12H4z" />
          <path d="M8 6V4h8v2" />
        </>
      );
    case 'rules':
      return (
        <>
          <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z" />
          <path d="M14 2v6h6M16 13H8M16 17H8M10 9H8" />
        </>
      );
    case 'terminal':
      return (
        <>
          <path d="M4 17l6-6-6-6" />
          <path d="M12 19h8" />
        </>
      );
    case 'diff':
      return (
        <>
          <path d="M8 6h13" />
          <path d="M8 12h13" />
          <path d="M8 18h13" />
          <path d="M3 6h.01" />
          <path d="M3 12h.01" />
          <path d="M3 18h.01" />
        </>
      );
    default:
      return null;
  }
}

export type InspectorIconTabsProps = {
  tabs: WorkspaceTabId[];
  activeTab: WorkspaceTabId;
  onTabChange: (tab: WorkspaceTabId) => void;
  tabIdFor: (tab: WorkspaceTabId) => string;
  tabPanelId: string;
  ariaLabel: string;
  /** Git status for Diff tab tip + dirty count badge (not shown in Composer). */
  diffGit?: WorkspaceGitBadgeInfo | null;
};

/** 44px vertical icon tabs for the workspace Inspector region. */
export default function InspectorIconTabs({
  tabs,
  activeTab,
  onTabChange,
  tabIdFor,
  tabPanelId,
  ariaLabel,
  diffGit = null,
}: InspectorIconTabsProps) {
  const { t } = useT();

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    handleTabListKeyDown(event, tabs, activeTab, onTabChange, tabIdFor);
  };

  return (
    <div
      className="inspector-icon-tabs"
      role="tablist"
      aria-orientation="vertical"
      aria-label={ariaLabel}
      onKeyDown={onKeyDown}
    >
      {tabs.map((tabId) => {
        const selected = activeTab === tabId;
        let label = t(TAB_LABEL_KEYS[tabId]);
        let badge: string | null = null;
        if (tabId === 'diff' && diffGit) {
          label =
            diffGit.dirty > 0
              ? t('diff.tabGitDirty', { branch: diffGit.branch, n: String(diffGit.dirty) })
              : t('diff.tabGitClean', { branch: diffGit.branch });
          if (diffGit.dirty > 0) {
            badge = diffGit.dirty > 99 ? '99+' : String(diffGit.dirty);
          }
        }
        return (
          <button
            key={tabId}
            id={tabIdFor(tabId)}
            type="button"
            role="tab"
            aria-selected={selected}
            aria-controls={tabPanelId}
            tabIndex={selected ? 0 : -1}
            className={`inspector-icon-tab${selected ? ' inspector-icon-tab--active' : ''}`}
            title={label}
            aria-label={label}
            data-tip={label}
            onClick={() => onTabChange(tabId)}
          >
            <svg viewBox="0 0 24 24" className="inspector-icon-tab__svg" aria-hidden>
              <WorkspaceTabIcon tab={tabId} />
            </svg>
            {badge ? (
              <span className="inspector-icon-tab__badge" aria-hidden>
                {badge}
              </span>
            ) : null}
          </button>
        );
      })}
    </div>
  );
}
