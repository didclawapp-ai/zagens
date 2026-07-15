import { invoke } from '@tauri-apps/api/core';
import {
  readThreadWorkspaceFile,
  readComposerWorkspaceFile,
} from '../api/client';
import { detectFileType, isBinaryFileType } from '../components/preview';
import type { PreviewState } from '../components/preview/types';
import { isOfficePreviewExternal } from './openWorkspaceSystem';
import { WorkspaceFileOpenError } from './workspaceFileOpenError';

export function normalizeWorkspaceRelPath(raw: string): string {
  let s = raw.trim().replace(/\\/g, '/');
  s = s.replace(/^[/\\]+/, '');
  if (s.startsWith('./')) {
    s = s.slice(2);
  }
  // markdown-it percent-encodes non-ASCII hrefs (e.g. 全库 → %E5%85%A8%E5%BA%93).
  // Decode once so filesystem / runtime APIs see the real path (thr_6f9c).
  if (/%[0-9A-Fa-f]{2}/.test(s)) {
    try {
      s = decodeURIComponent(s);
    } catch {
      /* keep percent-encoded form */
    }
  }
  return s;
}

/** Office / deliverable UTF-8 HTML sidecars (not ordinary workspace pages). */
export function isHtmlPreviewSidecar(fileName: string | undefined): boolean {
  const name = fileName?.toLowerCase() ?? '';
  return name.endsWith('.preview.html');
}

/**
 * Load a file under the composer workspace or active thread workspace into a preview payload.
 * DOCX/PPTX/XLSX (etc.) must use {@link openWorkspaceFileWithSystemApp} instead.
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
  const fileName = relPath.split('/').pop() ?? relPath;
  if (isOfficePreviewExternal(fileName)) {
    throw new WorkspaceFileOpenError('officeUseSystemApp');
  }

  const fileType = detectFileType(fileName);
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
        fileName,
        workspaceRelPath: relPath,
        content: bin.base64,
        fileType,
        size: bin.size,
        mimeType: bin.mime_type,
        truncated: bin.truncated,
        workspaceRoot: root || undefined,
        threadId: opts.resumedThreadId,
        desktopHost: opts.desktopHost,
      };
    }
    const file = await readThreadWorkspaceFile(opts.resumedThreadId, relPath);
    const resolved = detectFileType(fileName, file.language_hint);
    return {
      title,
      fileName,
      workspaceRelPath: relPath,
      content: file.content,
      language: file.language_hint ?? undefined,
      fileType: resolved,
      htmlPreview: isHtmlPreviewSidecar(fileName) || undefined,
      workspaceRoot: root || undefined,
      threadId: opts.resumedThreadId,
      desktopHost: opts.desktopHost,
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
      fileName,
      workspaceRelPath: relPath,
      content: bin.base64,
      fileType,
      size: bin.size,
      mimeType: bin.mime_type,
      truncated: bin.truncated,
      workspaceRoot: root,
      threadId: null,
      desktopHost: opts.desktopHost,
    };
  }

  const file = await readComposerWorkspaceFile(root, relPath);
  const resolved = detectFileType(fileName, file.language_hint);
  return {
    title,
    fileName,
    workspaceRelPath: relPath,
    content: file.content,
    language: file.language_hint ?? undefined,
    fileType: resolved,
    htmlPreview: isHtmlPreviewSidecar(fileName) || undefined,
    workspaceRoot: root,
    threadId: null,
    desktopHost: opts.desktopHost,
  };
}
