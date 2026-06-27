import type { RightPanelView, WorkspaceTabId } from '../components/RightPanel';

export type ContextCategoryNavTarget =
  | { view: 'workspace'; workspaceTab: WorkspaceTabId }
  | { view: Exclude<RightPanelView, 'workspace'> };

/** Explorer category → inspector destination (P2-3). */
export function contextCategoryNavTarget(categoryId: string): ContextCategoryNavTarget | null {
  switch (categoryId) {
    case 'rules':
      return { view: 'workspace', workspaceTab: 'rules' };
    case 'mcp':
      return { view: 'mcp' };
    case 'skills':
      return { view: 'skills' };
    case 'subagents':
      return { view: 'agents' };
    case 'tools':
      return { view: 'models' };
    case 'system':
      return { view: 'system' };
    default:
      return null;
  }
}

export function isContextCategoryNavigable(categoryId: string): boolean {
  return contextCategoryNavTarget(categoryId) != null;
}

export type ContextCategoryNavLabelKey =
  | 'contextExplorer.linkRules'
  | 'contextExplorer.linkMcp'
  | 'contextExplorer.linkSkills'
  | 'contextExplorer.linkSubagents'
  | 'contextExplorer.linkTools'
  | 'contextExplorer.linkSystem';

export function contextCategoryNavLabelKey(categoryId: string): ContextCategoryNavLabelKey | null {
  switch (categoryId) {
    case 'rules':
      return 'contextExplorer.linkRules';
    case 'mcp':
      return 'contextExplorer.linkMcp';
    case 'skills':
      return 'contextExplorer.linkSkills';
    case 'subagents':
      return 'contextExplorer.linkSubagents';
    case 'tools':
      return 'contextExplorer.linkTools';
    case 'system':
      return 'contextExplorer.linkSystem';
    default:
      return null;
  }
}
