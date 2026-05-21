import type { TranslationKey } from '../i18n/keys';

export type WorkspaceFileErrorKey =
  | 'invalidRel'
  | 'pathTraversal'
  | 'needWorkspace'
  | 'binaryNeedsDesktop';

const ERROR_KEYS: Record<WorkspaceFileErrorKey, TranslationKey> = {
  invalidRel: 'workspaceFiles.errors.invalidRel',
  pathTraversal: 'workspaceFiles.errors.pathTraversal',
  needWorkspace: 'workspaceFiles.errors.needWorkspace',
  binaryNeedsDesktop: 'workspaceFiles.errors.binaryNeedsDesktop',
};

export class WorkspaceFileOpenError extends Error {
  readonly key: WorkspaceFileErrorKey;

  constructor(key: WorkspaceFileErrorKey) {
    super(key);
    this.name = 'WorkspaceFileOpenError';
    this.key = key;
  }
}

export function formatWorkspaceFileError(
  err: unknown,
  t: (key: TranslationKey, params?: Record<string, string>) => string,
): string {
  if (err instanceof WorkspaceFileOpenError) {
    return t(ERROR_KEYS[err.key]);
  }
  if (err instanceof Error) return err.message;
  return String(err);
}
