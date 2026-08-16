export type FolderId = string;
export type DocumentId = string;

export interface Heading {
  level: number;
  text: string;
}

export interface Folder {
  id: FolderId;
  name: string;
  path: string;
  parentId: FolderId | null;
  childFolderIds: FolderId[];
  documentIds: DocumentId[];
}

export interface Document {
  id: DocumentId;
  name: string;
  title: string | null;
  headings: Heading[];
  path: string;
  parentId: FolderId;
}

export interface DocumentLink {
  id: string;
  sourceDocumentId: DocumentId;
  targetDocumentId: DocumentId | null;
  rawTarget: string;
  resolved: boolean;
  unresolvedReason: string | null;
}

export interface DocumentIndexUpdate {
  workspaceGeneration: number;
  workspaceRevision: number;
  contentToken: string;
  saveWarning: string | null;
  document: Document;
  links: DocumentLink[];
  patches: WorkspacePatch[];
}

export interface DocumentReadResult {
  workspaceGeneration: number;
  workspaceRevision: number;
  content: string;
  contentToken: string;
}

export interface WorkspaceSnapshot {
  workspaceGeneration: number;
  workspaceRevision: number;
  project: ProjectModel;
}

export interface WorkspacePatch {
  workspaceGeneration: number;
  workspaceRevision: number;
  upsertedFolders: Folder[];
  removedFolderIds: FolderId[];
  upsertedDocuments: Document[];
  removedDocumentIds: DocumentId[];
  upsertedLinks: DocumentLink[];
  removedLinkIds: string[];
  externallyChangedDocumentIds: DocumentId[];
}

export interface WorkspaceEventError {
  workspaceGeneration: number;
  workspaceRevision: number;
  message: string;
}

export interface ProjectModel {
  rootPath: string;
  folders: Record<FolderId, Folder>;
  documents: Record<DocumentId, Document>;
  links: DocumentLink[];
}
