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
  document: Document;
  links: DocumentLink[];
}

export interface DocumentReadResult {
  workspaceGeneration: number;
  content: string;
}

export interface WorkspaceSnapshot {
  workspaceGeneration: number;
  project: ProjectModel;
}

export interface ProjectModel {
  rootPath: string;
  folders: Record<FolderId, Folder>;
  documents: Record<DocumentId, Document>;
  links: DocumentLink[];
}
