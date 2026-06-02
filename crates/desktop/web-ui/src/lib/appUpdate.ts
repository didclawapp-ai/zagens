import { invoke } from '@tauri-apps/api/core';
import { relaunch } from '@tauri-apps/plugin-process';
import { subscribeCurrentWebviewEvent } from './tauriListen';
import { UPDATE_DOWNLOAD_BASE, UPDATE_MANIFEST_URL } from './updateConfig';

export type AppUpdateStatus = {
  ready: boolean;
  currentVersion: string;
  status: 'not_configured' | 'up_to_date' | 'available' | 'error';
  availableVersion?: string;
  notes?: string;
  downloadPageUrl: string;
  error?: string;
};

type RawUpdateStatus = {
  ready: boolean;
  currentVersion: string;
  status: string;
  availableVersion?: string;
  notes?: string;
  downloadPageUrl?: string;
  download_page_url?: string;
  error?: string;
};

function normalizeDownloadPageUrl(raw: RawUpdateStatus): string {
  const url = (raw.downloadPageUrl ?? raw.download_page_url ?? '').trim();
  if (url.startsWith('http://') || url.startsWith('https://')) {
    return url;
  }
  const base = UPDATE_DOWNLOAD_BASE.endsWith('/')
    ? UPDATE_DOWNLOAD_BASE
    : `${UPDATE_DOWNLOAD_BASE}/`;
  return url.startsWith('/') ? `https://zagens.com${url}` : base;
}

function normalizeStatus(raw: RawUpdateStatus): AppUpdateStatus {
  const status = raw.status as AppUpdateStatus['status'];
  return {
    ready: raw.ready,
    currentVersion: raw.currentVersion,
    status:
      status === 'up_to_date' ||
      status === 'available' ||
      status === 'error' ||
      status === 'not_configured'
        ? status
        : 'error',
    availableVersion: raw.availableVersion,
    notes: raw.notes,
    downloadPageUrl: normalizeDownloadPageUrl(raw),
    error: raw.error,
  };
}

export async function fetchAppUpdateStatus(): Promise<AppUpdateStatus> {
  const raw = await invoke<RawUpdateStatus>('get_update_status');
  return normalizeStatus(raw);
}

export function subscribeAppUpdateProgress(
  onProgress: (downloaded: number, total: number | null) => void,
): () => void {
  return subscribeCurrentWebviewEvent<{ downloaded: number; total: number | null }>(
    'zagens://app-update-progress',
    (payload) => {
      onProgress(payload.downloaded, payload.total ?? null);
    },
  );
}

export async function installAppUpdate(): Promise<void> {
  await invoke('install_app_update');
  // Windows NSIS installer quits the process before this runs; macOS/Linux relaunch.
  try {
    await relaunch();
  } catch {
    // Process may already be exiting on Windows.
  }
}

export { UPDATE_MANIFEST_URL };
