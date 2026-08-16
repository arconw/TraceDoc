export type FolderId = string;
export type DocumentId = string;

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
  path: string;
  parentId: FolderId;
}

export interface DocumentLink {
  id: string;
  sourceDocumentId: DocumentId;
  targetDocumentId: DocumentId;
  rawTarget: string;
  resolved: boolean;
}

export interface ProjectModel {
  rootPath: string;
  folders: Record<FolderId, Folder>;
  documents: Record<DocumentId, Document>;
  links: DocumentLink[];
}
