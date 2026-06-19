import { useCallback, useMemo, useState } from 'react';
import type { HarnessGridDataSnapshot } from '../lib/useHarnessGridData';
import type { HarnessCardId } from '../components/chrome/HarnessCard';

export function useHarnessFloatStack(args: {
  harnessData: HarnessGridDataSnapshot;
  userDismissed: boolean;
  focusMode: boolean;
}) {
  const { harnessData, userDismissed, focusMode } = args;
  const [flashCardId, setFlashCardId] = useState<HarnessCardId | null>(null);

  const visible = useMemo(
    () => harnessData.hasAnyData && !userDismissed && !focusMode,
    [harnessData.hasAnyData, userDismissed, focusMode],
  );

  const openAndScrollTo = useCallback((cardId: HarnessCardId) => {
    setFlashCardId(cardId);
    window.setTimeout(() => setFlashCardId((current) => (current === cardId ? null : current)), 320);
    const element = document.getElementById(`harness-card-${cardId}`);
    element?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }, []);

  return {
    visible,
    flashCardId,
    openAndScrollTo,
  };
}
