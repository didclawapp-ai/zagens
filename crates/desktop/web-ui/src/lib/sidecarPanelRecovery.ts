/**
 * Panel B-channel recovery after sidecar restart (LHT Phase 3 stable).
 * `sidecar://ready` is emitted once per boot; late windows get a re-emit from Rust.
 */

export const SIDECAR_READY_PANEL_EVENT = 'deepseek-sidecar-ready';

export function dispatchSidecarReadyForPanels(): void {
  window.dispatchEvent(new CustomEvent(SIDECAR_READY_PANEL_EVENT));
}
