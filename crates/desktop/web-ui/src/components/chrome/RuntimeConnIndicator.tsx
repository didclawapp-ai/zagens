import type { RuntimeConnectionState } from '../../api/client';
import {
  runtimeConnIndicatorClass,
  runtimeConnStatusLabel,
} from '../../lib/runtimeReachable';

export type RuntimeConnIndicatorProps = {
  runtimeConn: RuntimeConnectionState;
  streaming?: boolean;
  runtimeSessionEstablished?: boolean;
  pulseWhenBusy?: boolean;
  className?: string;
  labels: {
    connected: string;
    disconnected: string;
    busy: string;
    authMismatch: string;
    checking: string;
  };
};

/** 8px runtime status dot for the icon rail footer. */
export default function RuntimeConnIndicator({
  runtimeConn,
  streaming = false,
  runtimeSessionEstablished = false,
  pulseWhenBusy = true,
  className = '',
  labels,
}: RuntimeConnIndicatorProps) {
  const reachability = { streaming, sessionEstablished: runtimeSessionEstablished };
  const indicatorClass = runtimeConnIndicatorClass(runtimeConn, reachability);
  const label = runtimeConnStatusLabel(runtimeConn, reachability, labels);
  const isBusy = pulseWhenBusy && runtimeConn === 'checking';

  return (
    <div
      className={`icon-rail-conn ${className}`.trim()}
      title={label}
      aria-label={label}
    >
      <span
        className={`icon-rail-conn-dot ${indicatorClass} ${isBusy ? 'icon-rail-conn-dot--pulse' : ''}`}
        aria-hidden
      />
    </div>
  );
}
