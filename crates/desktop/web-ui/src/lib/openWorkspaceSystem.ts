import { invoke } from '@tauri-apps/api/core';
import { normalizeWorkspaceRelPath } from './openWorkspaceFile';
import { workspaceAbsolutePath } from './workspaceLinkMenu';

/** Office formats opened with the OS default app (not in-app preview). */
const OFFICE_EXTERNAL_EXTS = new Set([
  'docx',
  'doc',
  'xlsx',
  'xls',
  'pptx',
  'ppt',
]);

export function fileExtension(fileName: string): string {
  const dot = fileName.lastIndexOf('.');
  if (dot < 0) return '';
  return fileName.slice(dot + 1).toLowerCase();
}

export function isOfficePreviewExternal(fileName: string): boolean {
  return OFFICE_EXTERNAL_EXTS.has(fileExtension(fileName));
}

export async function openWorkspaceFileWithSystemApp(
  workspaceRoot: string,
  relPath: string,
): Promise<void> {
  const abs = workspaceAbsolutePath(
    workspaceRoot,
    normalizeWorkspaceRelPath(relPath),
  );
  await invoke('open_with_system_app', { path: abs });
}
