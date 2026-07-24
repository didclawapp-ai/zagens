import type { HarnessCardId } from '../components/chrome/HarnessCard';
import type { RightPanelView } from '../components/RightPanel';

/** Right-panel view opened when a harness float card (or icon-rail harness btn) is activated. */
export const HARNESS_CARD_VIEWS: Record<
  Exclude<HarnessCardId, 'changes'>,
  RightPanelView
> = {
  checklist: 'checklist',
  audit: 'audit',
  lht: 'long-horizon',
  agents: 'agents',
};
