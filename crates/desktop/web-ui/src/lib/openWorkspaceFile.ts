import { invoke } from '@tauri-apps/api/core';
import {
  readThreadWorkspaceFile,
  readComposerWorkspaceFile,
} from '../api/client';
import { detectFileType, isBinaryFileType } from '../components/preview';
import type { PreviewState } from '../components/preview/types';
import { WorkspaceFileOpenError } from './workspaceFileOpenError';

export function normalizeWorkspaceRelPath(raw: string): string {
  let s = raw.trim().replace(/\\/g, '/');
  s = s.replace(/^[/\\]+/, '');
  if (s.startsWith('./')) {
    s = s.slice(2);
  }
  return s;
}

/**
 * Load a file under the composer workspace or active thread workspace into a preview payload.
 * Same resolution rules as the file tree in RightPanel (runtime + optional Tauri for binary).
 */
export async function loadWorkspaceFileIntoPreview(opts: {
  relPath: string;
  title?: string;
  workspaceRoot: string;
  resumedThreadId: string | null;
  desktopHost: boolean;
}): Promise<PreviewState> {
  const relPath = normalizeWorkspaceRelPath(opts.relPath);
  if (!relPath) {
    throw new WorkspaceFileOpenError('invalidRel');
  }
  if (relPath.includes('..')) {
    throw new WorkspaceFileOpenError('pathTraversal');
  }

  const title = opts.title?.trim() || relPath.split('/').pop() || relPath;
  const fileType = detectFileType(title);
  const root = opts.workspaceRoot.trim();

  if (opts.resumedThreadId) {
    if (isBinaryFileType(fileType)) {
      const bin = await invoke<{
        mime_type: string;
        base64: string;
        size: number;
        truncated: boolean;
      }>('read_thread_workspace_binary', {
        threadId: opts.resumedThreadId,
        relativePath: relPath,
      });
      return {
        title,
        fileName: relPath.split('/').pop(),
        workspaceRelPath: relPath,
        content: bin.base64,
        fileType,
        size: bin.size,
        mimeType: bin.mime_type,
        truncated: bin.truncated,
      };
    }
    const file = await readThreadWorkspaceFile(opts.resumedThreadId, relPath);
    return {
      title,
      fileName: relPath.split('/').pop(),
      workspaceRelPath: relPath,
      content: file.content,
      language: file.language_hint ?? undefined,
      fileType: detectFileType(relPath.split('/').pop(), file.language_hint),
    };
  }

  if (!root) {
    throw new WorkspaceFileOpenError('needWorkspace');
  }

  if (isBinaryFileType(fileType)) {
    if (!opts.desktopHost) {
      throw new WorkspaceFileOpenError('binaryNeedsDesktop');
    }
    const bin = await invoke<{
      mime_type: string;
      base64: string;
      size: number;
      truncated: boolean;
    }>('read_workspace_binary_at_root', {
      workspaceRoot: root,
      relativePath: relPath,
    });
    return {
      title,
      fileName: relPath.split('/').pop(),
      workspaceRelPath: relPath,
      content: bin.base64,
      fileType,
      size: bin.size,
      mimeType: bin.mime_type,
      truncated: bin.truncated,
    };
  }

  const file = await readComposerWorkspaceFile(root, relPath);
  return {
    title,
    fileName: relPath.split('/').pop(),
    workspaceRelPath: relPath,
    content: file.content,
    language: file.language_hint ?? undefined,
    fileType: detectFileType(relPath.split('/').pop(), file.language_hint),
  };
}
