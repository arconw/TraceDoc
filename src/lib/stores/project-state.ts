import type {
  DocumentId,
  DocumentIndexUpdate,
  WorkspacePatch,
  WorkspaceEventError,
  WorkspaceSnapshot,
} from '../types/workspace';

export type ProjectState =
  | { status: 'empty' }
  | {
      status: 'loading';
      pendingPatches: WorkspacePatch[];
      pendingErrors: WorkspaceEventError[];
    }
  | {
      status: 'loaded';
      project: WorkspaceSnapshot['project'];
      workspaceGeneration: number;
      workspaceRevision: number;
      selectedDocumentId: DocumentId | null;
      documentChangeVersions: Record<DocumentId, number>;
      pendingRevisionPatches: Record<number, WorkspacePatch>;
      watchError: string | null;
    }
  | { status: 'error'; message: string };

export function applyDocumentIndex(
  state: ProjectState,
  updateValue: DocumentIndexUpdate,
): ProjectState {
  if (
    state.status !== 'loaded' ||
    state.workspaceGeneration !== updateValue.workspaceGeneration ||
    updateValue.workspaceRevision <= state.workspaceRevision ||
    !state.project.documents[updateValue.document.id]
  ) {
    return state;
  }

  if (updateValue.patches.length > 0) {
    return [...updateValue.patches]
      .sort((left, right) => left.workspaceRevision - right.workspaceRevision)
      .reduce<ProjectState>(applyWorkspacePatch, state);
  }

  return drainPendingPatches({
    ...state,
    workspaceRevision: updateValue.workspaceRevision,
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
  });
}

export function applyWorkspacePatch(
  state: ProjectState,
  patch: WorkspacePatch,
): ProjectState {
  if (state.status === 'loading') {
    return { ...state, pendingPatches: [...state.pendingPatches, patch] };
  }
  if (
    state.status !== 'loaded' ||
    state.workspaceGeneration !== patch.workspaceGeneration ||
    patch.workspaceRevision <= state.workspaceRevision
  ) {
    return state;
  }
  if (patch.workspaceRevision > state.workspaceRevision + 1) {
    return {
      ...state,
      pendingRevisionPatches: {
        ...state.pendingRevisionPatches,
        [patch.workspaceRevision]: patch,
      },
    };
  }

  return drainPendingPatches(applyContiguousPatch(state, patch));
}

function applyContiguousPatch(
  state: Extract<ProjectState, { status: 'loaded' }>,
  patch: WorkspacePatch,
): Extract<ProjectState, { status: 'loaded' }> {
  const folders = { ...state.project.folders };
  const documents = { ...state.project.documents };
  const versions = { ...state.documentChangeVersions };
  for (const id of patch.removedFolderIds) delete folders[id];
  for (const folder of patch.upsertedFolders) folders[folder.id] = folder;
  for (const id of patch.removedDocumentIds) delete documents[id];
  for (const document of patch.upsertedDocuments) {
    documents[document.id] = document;
  }
  for (const id of patch.externallyChangedDocumentIds) {
    versions[id] = (versions[id] ?? 0) + 1;
  }

  const removedLinks = new Set(patch.removedLinkIds);
  const upsertedLinks = new Map(
    patch.upsertedLinks.map((link) => [link.id, link]),
  );
  const links = state.project.links
    .filter((link) => !removedLinks.has(link.id))
    .map((link) => upsertedLinks.get(link.id) ?? link);
  const existingLinkIds = new Set(links.map((link) => link.id));
  for (const link of patch.upsertedLinks) {
    if (!existingLinkIds.has(link.id)) links.push(link);
  }
  const pendingRevisionPatches = { ...state.pendingRevisionPatches };
  delete pendingRevisionPatches[patch.workspaceRevision];

  return {
    ...state,
    workspaceRevision: patch.workspaceRevision,
    project: {
      ...state.project,
      folders,
      documents,
      links: canonicalLinks(links),
    },
    documentChangeVersions: versions,
    pendingRevisionPatches,
    watchError: null,
  };
}

function drainPendingPatches(
  state: Extract<ProjectState, { status: 'loaded' }>,
): Extract<ProjectState, { status: 'loaded' }> {
  let next = state;
  while (next.pendingRevisionPatches[next.workspaceRevision + 1]) {
    next = applyContiguousPatch(
      next,
      next.pendingRevisionPatches[next.workspaceRevision + 1],
    );
  }
  return next;
}

export function applyWorkspaceSnapshot(
  state: ProjectState,
  snapshot: WorkspaceSnapshot,
): ProjectState {
  if (
    state.status !== 'loaded' ||
    state.workspaceGeneration !== snapshot.workspaceGeneration ||
    snapshot.workspaceRevision <= state.workspaceRevision
  ) {
    return state;
  }
  const versions = { ...state.documentChangeVersions };
  if (state.selectedDocumentId) {
    versions[state.selectedDocumentId] =
      (versions[state.selectedDocumentId] ?? 0) + 1;
  }
  const pendingRevisionPatches = Object.fromEntries(
    Object.entries(state.pendingRevisionPatches).filter(
      ([revision]) => Number(revision) > snapshot.workspaceRevision,
    ),
  );
  return drainPendingPatches({
    ...state,
    workspaceRevision: snapshot.workspaceRevision,
    project: snapshot.project,
    documentChangeVersions: versions,
    pendingRevisionPatches,
    watchError: null,
  });
}

export function applyWorkspaceError(
  state: ProjectState,
  error: WorkspaceEventError,
): ProjectState {
  if (state.status === 'loading') {
    return { ...state, pendingErrors: [...state.pendingErrors, error] };
  }
  return state.status === 'loaded' &&
    state.workspaceGeneration === error.workspaceGeneration &&
    error.workspaceRevision >= state.workspaceRevision
    ? { ...state, watchError: error.message }
    : state;
}

export function closeSelectedDocument(
  state: ProjectState,
  documentId: DocumentId,
): ProjectState {
  if (state.status !== 'loaded' || state.selectedDocumentId !== documentId) {
    return state;
  }
  return { ...state, selectedDocumentId: null };
}

export function completeWorkspaceOpen(
  state: ProjectState,
  snapshot: WorkspaceSnapshot,
): ProjectState {
  let loaded: ProjectState = {
    status: 'loaded',
    project: snapshot.project,
    workspaceGeneration: snapshot.workspaceGeneration,
    workspaceRevision: snapshot.workspaceRevision,
    selectedDocumentId: null,
    documentChangeVersions: {},
    pendingRevisionPatches: {},
    watchError: null,
  };
  if (state.status !== 'loading') return loaded;
  for (const patch of [...state.pendingPatches].sort(
    (left, right) => left.workspaceRevision - right.workspaceRevision,
  )) {
    loaded = applyWorkspacePatch(loaded, patch);
  }
  for (const error of state.pendingErrors) {
    loaded = applyWorkspaceError(loaded, error);
  }
  return loaded;
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
