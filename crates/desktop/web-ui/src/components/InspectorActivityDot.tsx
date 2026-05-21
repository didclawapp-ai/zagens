import type { InspectorNavActivity } from '../lib/inspectorUnread';

/** Small sidebar activity indicator (dot, not a numeric badge). */
export default function InspectorActivityDot({ activity }: { activity: InspectorNavActivity }) {
  if (!activity.active) {
    return null;
  }
  return (
    <span
      className={`ml-auto shrink-0 size-[6px] rounded-full bg-accent ${
        activity.pulse ? 'animate-pulse' : ''
      }`}
      aria-hidden
    />
  );
}
