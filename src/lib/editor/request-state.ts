export interface EditorReadRequest {
  version: number;
  documentId: string;
  workspaceGeneration: number;
}

export function editorReadIsCurrent(
  request: EditorReadRequest,
  currentVersion: number,
  currentDocumentId: string | null,
  currentWorkspaceGeneration: number,
  resultWorkspaceGeneration: number,
): boolean {
  return (
    request.version === currentVersion &&
    request.documentId === currentDocumentId &&
    request.workspaceGeneration === currentWorkspaceGeneration &&
    resultWorkspaceGeneration === request.workspaceGeneration
  );
}

export function retainedLocalBaseline(
  localContent: string,
  diskContent: string,
  diskContentToken: string,
) {
  return {
    savedContent: diskContent,
    savedContentToken: diskContentToken,
    dirty: localContent !== diskContent,
  };
}

export function closesDeletedBufferWithoutDiskAccess(
  conflict: 'modified' | 'deleted' | null,
) {
  return conflict === 'deleted';
}

/**
 * Decides whether a `write_document` result should be treated as a lost
 * save-conflict race.
 *
 * The backend serializes every workspace-revision bump behind a single
 * write lock, so a save that started when the client last knew the
 * revision was `requestRevision` is unambiguously successful and current
 * whenever the backend's returned `resultWorkspaceRevision` is greater
 * than that request-time snapshot. This must only ever compare against
 * the stable value captured when the save started -- never against a
 * live/reactive workspace-revision value read after the fact, because
 * that value can already have advanced through a separate, independent
 * `workspace-patch` event (for example an edit to a completely different
 * document) while the save request was still in flight. Comparing
 * against that racing live value produced false "external conflict"
 * results on saves that had, in fact, already succeeded on disk.
 */
export function writeResultIsStale(
  resultWorkspaceRevision: number,
  requestRevision: number,
): boolean {
  return resultWorkspaceRevision <= requestRevision;
}
