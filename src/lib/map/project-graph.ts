import type { DocumentId, FolderId, ProjectModel } from '../types/workspace';

export interface MapFolder {
  id: FolderId;
  name: string;
  path: string;
  parentId: FolderId | null;
  childFolderIds: FolderId[];
  documentIds: DocumentId[];
}

export interface MapDocument {
  id: DocumentId;
  name: string;
  title: string;
  path: string;
  parentId: FolderId;
}

export interface MapLink {
  id: string;
  sourceDocumentId: DocumentId;
  targetDocumentId: DocumentId;
}

export interface MapGraph {
  rootFolderIds: FolderId[];
  folders: Record<FolderId, MapFolder>;
  documents: Record<DocumentId, MapDocument>;
  links: MapLink[];
}

export function projectToMapGraph(project: ProjectModel): MapGraph {
  const folders = Object.values(project.folders)
    .sort((left, right) => compareCodePoints(left.path, right.path))
    .reduce<Record<FolderId, MapFolder>>((result, folder) => {
      result[folder.id] = {
        id: folder.id,
        name: folder.name,
        path: folder.path,
        parentId: folder.parentId,
        childFolderIds: sortIdsByPath(
          folder.childFolderIds.filter((id) => Boolean(project.folders[id])),
          project.folders,
        ),
        documentIds: sortIdsByPath(
          folder.documentIds.filter((id) => Boolean(project.documents[id])),
          project.documents,
        ),
      };
      return result;
    }, {});

  const documents = Object.values(project.documents)
    .filter((document) => Boolean(folders[document.parentId]))
    .sort((left, right) => compareCodePoints(left.path, right.path))
    .reduce<Record<DocumentId, MapDocument>>((result, document) => {
      result[document.id] = {
        id: document.id,
        name: document.name,
        title: document.title ?? document.name,
        path: document.path,
        parentId: document.parentId,
      };
      return result;
    }, {});

  const links = project.links
    .filter(
      (link): link is typeof link & { targetDocumentId: DocumentId } =>
        link.resolved &&
        link.targetDocumentId !== null &&
        Boolean(documents[link.sourceDocumentId]) &&
        Boolean(documents[link.targetDocumentId]),
    )
    .map((link) => ({
      id: link.id,
      sourceDocumentId: link.sourceDocumentId,
      targetDocumentId: link.targetDocumentId,
    }))
    .sort((left, right) => compareCodePoints(left.id, right.id));

  return {
    rootFolderIds: Object.values(folders)
      .filter((folder) => folder.parentId === null)
      .map((folder) => folder.id),
    folders,
    documents,
    links,
  };
}

function sortIdsByPath<T extends { path: string }>(
  ids: string[],
  records: Record<string, T>,
) {
  return [...ids].sort((leftId, rightId) => {
    const pathDifference = compareCodePoints(
      records[leftId].path,
      records[rightId].path,
    );
    return pathDifference || compareCodePoints(leftId, rightId);
  });
}

function compareCodePoints(left: string, right: string) {
  const leftCharacters = Array.from(left);
  const rightCharacters = Array.from(right);
  const sharedLength = Math.min(leftCharacters.length, rightCharacters.length);

  for (let index = 0; index < sharedLength; index += 1) {
    const difference =
      leftCharacters[index].codePointAt(0)! -
      rightCharacters[index].codePointAt(0)!;
    if (difference !== 0) return difference;
  }

  return leftCharacters.length - rightCharacters.length;
}
