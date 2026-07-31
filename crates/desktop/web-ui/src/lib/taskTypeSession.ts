import type { DesktopRunModeId } from '../types/desktop';

export function runModesForSession(): DesktopRunModeId[] {
  return ['plan', 'agent', 'yolo'];
}
