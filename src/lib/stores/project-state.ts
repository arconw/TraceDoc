import type {
  DocumentId,
  DocumentIndexUpdate,
  WorkspaceSnapshot,
} from '../types/workspace';

export type ProjectState =
  | { status: 'empty' }
  | { status: 'loading' }
  | {
      status: 'loaded';
      project: WorkspaceSnapshot['project'];
      workspaceGeneration: number;
      selectedDocumentId: DocumentId | null;
    }
  | { status: 'error'; message: string };

export function applyDocumentIndex(
  state: ProjectState,
  updateValue: DocumentIndexUpdate,
): ProjectState {
  if (
    state.status !== 'loaded' ||
    state.workspaceGeneration !== updateValue.workspaceGeneration ||
    !state.project.documents[updateValue.document.id]
  ) {
    return state;
  }

  return {
    ...state,
    project: {
      ...state.project,
      documents: {
        ...state.project.documents,
        [updateValue.document.id]: updateValue.document,
      },
      links: canonicalLinks([
        ...state.project.links.filter(
          (link) => link.sourceDocumentId !== updateValue.document.id,
        ),
        ...updateValue.links,
      ]),
    },
  };
}

function canonicalLinks(links: WorkspaceSnapshot['project']['links']) {
  return [...links].sort((left, right) => {
    if (left.sourceDocumentId !== right.sourceDocumentId) {
      return compareCodePoints(left.sourceDocumentId, right.sourceDocumentId);
    }
    return compareCodePoints(left.id, right.id);
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
