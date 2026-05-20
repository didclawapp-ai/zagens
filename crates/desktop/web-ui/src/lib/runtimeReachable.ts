import type { RuntimeConnectionState } from '../api/client';

/** Inputs for whether panel/workspace HTTP APIs should stay enabled. */
export type RuntimeReachabilityInput = {
  streaming?: boolean;
  /**
   * Boot connected or user has an active resumed thread — probe blips must not
   * disable checklist, workspace browse, audit bar, etc.
   */
  sessionEstablished?: boolean;
};

function reachability(
  streamingOrOpts: boolean | RuntimeReachabilityInput = false,
): Required<RuntimeReachabilityInput> {
  if (typeof streamingOrOpts === 'boolean') {
    return { streaming: streamingOrOpts, sessionEstablished: false };
  }
  return {
    streaming: Boolean(streamingOrOpts.streaming),
    sessionEstablished: Boolean(streamingOrOpts.sessionEstablished),
  };
}

/**
 * Runtime HTTP/SSE is usable for panel APIs and workspace browse.
 * Probe `offline` during an active session does not block the UI — only
 * `auth_mismatch` and a cold disconnected boot do.
 */
export function isRuntimeApiAvailable(
  conn: RuntimeConnectionState,
  streamingOrOpts: boolean | RuntimeReachabilityInput = false,
): boolean {
  const { streaming, sessionEstablished } = reachability(streamingOrOpts);
  if (conn === 'auth_mismatch') {
    return false;
  }
  if (sessionEstablished) {
    return true;
  }
  if (conn === 'connected') {
    return true;
  }
  if (streaming && conn === 'offline') {
    return true;
  }
  return false;
}

/** Sidebar / status: sidecar busy or probe lag, but session APIs should still work. */
export function runtimeConnIsDegraded(
  conn: RuntimeConnectionState,
  streamingOrOpts: boolean | RuntimeReachabilityInput = false,
): boolean {
  if (conn === 'connected' || conn === 'auth_mismatch') {
    return false;
  }
  const { streaming, sessionEstablished } = reachability(streamingOrOpts);
  return sessionEstablished || (streaming && conn === 'offline');
}

/** Sidebar dot: green when healthy; amber when degraded-busy; red when truly offline. */
export function runtimeConnIndicatorClass(
  conn: RuntimeConnectionState,
  streamingOrOpts: boolean | RuntimeReachabilityInput = false,
): string {
  if (conn === 'connected') {
    return 'bg-emerald-500';
  }
  if (runtimeConnIsDegraded(conn, streamingOrOpts)) {
    return 'bg-amber-400';
  }
  if (isRuntimeApiAvailable(conn, streamingOrOpts)) {
    return 'bg-amber-400';
  }
  return 'bg-red-500';
}

export function runtimeConnStatusLabel(
  conn: RuntimeConnectionState,
  streamingOrOpts: boolean | RuntimeReachabilityInput = false,
  labels: {
    connected: string;
    disconnected: string;
    busy: string;
    authMismatch: string;
    checking: string;
  },
): string {
  if (conn === 'checking') {
    return labels.checking;
  }
  if (conn === 'auth_mismatch') {
    return labels.authMismatch;
  }
  if (conn === 'connected') {
    return labels.connected;
  }
  if (runtimeConnIsDegraded(conn, streamingOrOpts)) {
    return labels.busy;
  }
  if (isRuntimeApiAvailable(conn, streamingOrOpts)) {
    return labels.connected;
  }
  return labels.disconnected;
}
