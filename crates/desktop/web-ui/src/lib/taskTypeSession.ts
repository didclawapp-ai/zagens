import type {
  DesktopRunModeId,
  DesktopTaskTypePreference,
  DesktopTaskTypeResolved,
} from '../types/desktop';

/** Whether the active UI session should use Office tooling and chrome. */
export function isOfficeSession(
  preference: DesktopTaskTypePreference,
  locked: DesktopTaskTypeResolved | null,
  hasResumedThread: boolean,
): boolean {
  if (locked === 'office') return true;
  if (locked === 'code') return false;
  return !hasResumedThread && preference === 'office';
}

/** Run modes that make sense for the current task type (§12.4 task-type-prompt-architecture). */
export function runModesForSession(officeSession: boolean): DesktopRunModeId[] {
  return officeSession ? ['agent'] : ['plan', 'agent', 'yolo'];
}

export function coerceRunModeForSession(
  runMode: DesktopRunModeId,
  officeSession: boolean,
): DesktopRunModeId {
  if (officeSession && runMode !== 'agent') {
    return 'agent';
  }
  return runMode;
}
