import type { DesktopRunModeId } from '../types/desktop';

/** Flags sent to `/v1/stream` and thread turns — must match Rust `runtime_api` expectations. */
export function streamFlagsForRunMode(
  runMode: DesktopRunModeId,
  agentAutoApprove: boolean,
): {
  mode: string;
  allow_shell: boolean;
  trust_mode: boolean;
  auto_approve: boolean;
} {
  switch (runMode) {
    case 'plan':
      return {
        mode: 'plan',
        allow_shell: false,
        trust_mode: false,
        auto_approve: false,
      };
    case 'yolo':
      return {
        mode: 'yolo',
        allow_shell: true,
        trust_mode: true,
        auto_approve: true,
      };
    default:
      return {
        mode: 'agent',
        allow_shell: true,
        trust_mode: false,
        auto_approve: agentAutoApprove,
      };
  }
}
