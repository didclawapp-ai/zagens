/** Site-wide constants — keep in sync with deploy / CDN paths. */
export const SUPPORT_EMAIL = 'didclawapp@gmail.com';

/** Relative path; production serves installers from zagens.com/download/ */
export const DOWNLOAD_BASE = '/download/';

export function downloadUrl(filename: string): string {
  return `${DOWNLOAD_BASE}${encodeURIComponent(filename)}`;
}
