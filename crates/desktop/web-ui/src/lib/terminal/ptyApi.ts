import { invoke } from '@tauri-apps/api/core';

export type TerminalShellKind =
  | 'default'
  | 'pwsh'
  | 'powershell'
  | 'cmd'
  | 'bash'
  | 'zsh'
  | 'sh';

export interface SpawnTerminalOptions {
  shell?: TerminalShellKind;
  loadProfile?: boolean;
}

export async function spawnTerminal(
  workspace: string,
  cols: number,
  rows: number,
  options: SpawnTerminalOptions = {},
): Promise<string> {
  return invoke<string>('spawn_terminal', {
    workspace,
    cols,
    rows,
    shell: options.shell ?? 'default',
    loadProfile: options.loadProfile ?? false,
  });
}

export async function writeTerminal(id: string, data: string): Promise<void> {
  await invoke('write_terminal', { id, data });
}

export async function resizeTerminal(id: string, cols: number, rows: number): Promise<void> {
  await invoke('resize_terminal', { id, cols, rows });
}

export async function killTerminal(id: string): Promise<void> {
  await invoke('kill_terminal', { id });
}

export interface TerminalDataEvent {
  id: string;
  data: string;
}

export interface TerminalExitEvent {
  id: string;
  code: number | null;
}
