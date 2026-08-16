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
