export type StoragePressureLevel = 'ok' | 'warn' | 'critical';

export type VolumePressure = {
  path: string;
  free_bytes: number;
  level: StoragePressureLevel;
};

export type StoragePressureSnapshot = {
  pause_turns: boolean;
  user_data: VolumePressure;
  workspace: VolumePressure | null;
};

export function formatStorageBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '—';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export async function fetchStoragePressure(
  workspaceRoot: string,
): Promise<StoragePressureSnapshot | null> {
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) {
    return null;
  }
  const { invoke } = await import('@tauri-apps/api/core');
  const ws = workspaceRoot.trim();
  return invoke<StoragePressureSnapshot>('get_storage_pressure', {
    workspace_root: ws.length > 0 ? ws : null,
  });
}

export function worstStorageLevel(
  snapshot: StoragePressureSnapshot | null,
): StoragePressureLevel {
  if (!snapshot) return 'ok';
  const levels: StoragePressureLevel[] = [snapshot.user_data.level];
  if (snapshot.workspace) levels.push(snapshot.workspace.level);
  if (levels.includes('critical')) return 'critical';
  if (levels.includes('warn')) return 'warn';
  return 'ok';
}
