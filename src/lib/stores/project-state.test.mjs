import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { transformWithEsbuild } from 'vite';

const source = await readFile(
  new URL('./project-state.ts', import.meta.url),
  'utf8',
);
const transformed = await transformWithEsbuild(source, 'project-state.ts', {
  format: 'esm',
  loader: 'ts',
  target: 'es2022',
});
const moduleUrl = `data:text/javascript;base64,${Buffer.from(transformed.code).toString('base64')}`;
const {
  applyDocumentIndex,
  applyWorkspaceError,
  applyWorkspacePatch,
  applyWorkspaceSnapshot,
  closeSelectedDocument,
  completeWorkspaceOpen,
} = await import(moduleUrl);

const firstDocument = {
  id: 'document:first.md',
  name: 'first.md',
  title: 'First',
  headings: [{ level: 1, text: 'First' }],
  path: 'first.md',
  parentId: 'folder:.',
};

const secondDocument = {
  id: 'document:second.md',
  name: 'second.md',
  title: 'Second',
  headings: [{ level: 1, text: 'Second' }],
  path: 'second.md',
  parentId: 'folder:.',
};

function loadedState(workspaceGeneration) {
  return {
    status: 'loaded',
    workspaceGeneration,
    workspaceRevision: 10,
    selectedDocumentId: secondDocument.id,
    documentChangeVersions: {},
    pendingRevisionPatches: {},
    watchError: null,
    project: {
      rootPath: '/workspace',
      folders: {},
      documents: {
        [firstDocument.id]: firstDocument,
        [secondDocument.id]: secondDocument,
      },
      links: [
        {
          id: 'link:second',
          sourceDocumentId: secondDocument.id,
          targetDocumentId: firstDocument.id,
          rawTarget: 'first.md',
          resolved: true,
          unresolvedReason: null,
        },
      ],
    },
  };
}

const update = {
  workspaceGeneration: 7,
  workspaceRevision: 11,
  contentToken: 'saved',
  patches: [],
  document: {
    ...firstDocument,
    title: 'Changed',
    headings: [{ level: 1, text: 'Changed' }],
  },
  links: [
    {
      id: 'link:z',
      sourceDocumentId: firstDocument.id,
      targetDocumentId: secondDocument.id,
      rawTarget: 'second.md',
      resolved: true,
      unresolvedReason: null,
    },
    {
      id: 'link:a',
      sourceDocumentId: firstDocument.id,
      targetDocumentId: null,
      rawTarget: 'missing.md',
      resolved: false,
      unresolvedReason: 'Target document was not found',
    },
  ],
};

test('applies a completed save after selection changes in the same workspace', () => {
  const state = loadedState(7);
  const next = applyDocumentIndex(state, update);

  assert.equal(next.status, 'loaded');
  assert.equal(next.selectedDocumentId, secondDocument.id);
  assert.equal(next.project.documents[firstDocument.id].title, 'Changed');
  assert.deepEqual(
    next.project.links.map((link) => link.id),
    ['link:a', 'link:z', 'link:second'],
  );
});

test('rejects a completed save from an older workspace generation', () => {
  const state = loadedState(8);
  const next = applyDocumentIndex(state, update);

  assert.strictEqual(next, state);
  assert.equal(state.project.documents[firstDocument.id].title, 'First');
});

test('applies one live patch to the shared project model', () => {
  const state = loadedState(7);
  const next = applyWorkspacePatch(state, {
    workspaceGeneration: 7,
    workspaceRevision: 11,
    upsertedFolders: [],
    removedFolderIds: [],
    upsertedDocuments: [
      {
        id: 'document:new.md',
        name: 'new.md',
        title: 'New',
        headings: [{ level: 1, text: 'New' }],
        path: 'new.md',
        parentId: 'folder:.',
      },
    ],
    removedDocumentIds: [firstDocument.id],
    upsertedLinks: [],
    removedLinkIds: ['link:second'],
    externallyChangedDocumentIds: [secondDocument.id],
  });

  assert.equal(next.status, 'loaded');
  assert.equal(next.project.documents[firstDocument.id], undefined);
  assert.equal(next.project.documents['document:new.md'].title, 'New');
  assert.deepEqual(next.project.links, []);
  assert.equal(next.selectedDocumentId, secondDocument.id);
  assert.equal(next.documentChangeVersions[secondDocument.id], 1);
});

test('rejects a live patch from an older workspace generation', () => {
  const state = loadedState(8);
  const next = applyWorkspacePatch(state, {
    workspaceGeneration: 7,
    workspaceRevision: 11,
    upsertedFolders: [],
    removedFolderIds: [],
    upsertedDocuments: [],
    removedDocumentIds: [secondDocument.id],
    upsertedLinks: [],
    removedLinkIds: [],
    externallyChangedDocumentIds: [],
  });

  assert.strictEqual(next, state);
});

test('rejects an older save response after a newer watcher patch', () => {
  const state = loadedState(7);
  const watched = applyWorkspacePatch(state, {
    workspaceGeneration: 7,
    workspaceRevision: 12,
    upsertedFolders: [],
    removedFolderIds: [],
    upsertedDocuments: [
      {
        ...firstDocument,
        title: 'External',
        headings: [{ level: 1, text: 'External' }],
      },
    ],
    removedDocumentIds: [],
    upsertedLinks: [],
    removedLinkIds: [],
    externallyChangedDocumentIds: [firstDocument.id],
  });
  const afterLateSave = applyDocumentIndex(watched, update);

  assert.equal(watched.workspaceRevision, 10);
  assert.equal(watched.pendingRevisionPatches[12].workspaceRevision, 12);
  assert.equal(afterLateSave.workspaceRevision, 12);
  assert.equal(
    afterLateSave.project.documents[firstDocument.id].title,
    'External',
  );
});

test('a newer save response catches up a delayed unrelated watcher patch', () => {
  const state = loadedState(7);
  const watcherPatch = {
    workspaceGeneration: 7,
    workspaceRevision: 11,
    upsertedFolders: [],
    removedFolderIds: [],
    upsertedDocuments: [{ ...secondDocument, title: 'External second' }],
    removedDocumentIds: [],
    upsertedLinks: [],
    removedLinkIds: [],
    externallyChangedDocumentIds: [secondDocument.id],
  };
  const savePatch = {
    workspaceGeneration: 7,
    workspaceRevision: 12,
    upsertedFolders: [],
    removedFolderIds: [],
    upsertedDocuments: [{ ...firstDocument, title: 'Saved first' }],
    removedDocumentIds: [],
    upsertedLinks: [],
    removedLinkIds: [],
    externallyChangedDocumentIds: [],
  };
  const next = applyDocumentIndex(state, {
    ...update,
    workspaceRevision: 12,
    patches: [watcherPatch, savePatch],
  });

  assert.equal(next.workspaceRevision, 12);
  assert.equal(
    next.project.documents[secondDocument.id].title,
    'External second',
  );
  assert.equal(next.project.documents[firstDocument.id].title, 'Saved first');
});

test('ignores watcher errors from an older workspace generation', () => {
  const state = loadedState(8);
  const next = applyWorkspaceError(state, {
    workspaceGeneration: 7,
    workspaceRevision: 10,
    message: 'old watcher failed',
  });

  assert.strictEqual(next, state);
  assert.equal(next.watchError, null);
});

test('ignores a delayed same-generation error older than current success', () => {
  const state = { ...loadedState(8), workspaceRevision: 14 };
  const stale = applyWorkspaceError(state, {
    workspaceGeneration: 8,
    workspaceRevision: 13,
    message: 'stale failure',
  });
  assert.equal(stale.watchError, null);

  const current = applyWorkspaceError(state, {
    workspaceGeneration: 8,
    workspaceRevision: 14,
    message: 'current failure',
  });
  assert.equal(current.watchError, 'current failure');
  const cleared = applyWorkspacePatch(current, {
    workspaceGeneration: 8,
    workspaceRevision: 15,
    upsertedFolders: [],
    removedFolderIds: [],
    upsertedDocuments: [],
    removedDocumentIds: [],
    upsertedLinks: [],
    removedLinkIds: [],
    externallyChangedDocumentIds: [],
  });
  assert.equal(cleared.watchError, null);
});

test('ignores an old manual refresh completion after a workspace switch', () => {
  const state = loadedState(8);
  const next = applyWorkspaceSnapshot(state, {
    workspaceGeneration: 7,
    workspaceRevision: 99,
    project: { rootPath: '/old', folders: {}, documents: {}, links: [] },
  });

  assert.strictEqual(next, state);
  assert.equal(next.project.rootPath, '/workspace');
});

test('replays a newer watcher patch buffered while a workspace opens', () => {
  const snapshot = {
    workspaceGeneration: 9,
    workspaceRevision: 2,
    project: loadedState(9).project,
  };
  const patch = {
    workspaceGeneration: 9,
    workspaceRevision: 3,
    upsertedFolders: [],
    removedFolderIds: [],
    upsertedDocuments: [{ ...firstDocument, title: 'After snapshot' }],
    removedDocumentIds: [],
    upsertedLinks: [],
    removedLinkIds: [],
    externallyChangedDocumentIds: [firstDocument.id],
  };
  const loading = applyWorkspacePatch(
    { status: 'loading', pendingPatches: [], pendingErrors: [] },
    patch,
  );
  const loadingWithError = applyWorkspaceError(loading, {
    workspaceGeneration: 9,
    workspaceRevision: 1,
    message: 'delayed old error',
  });
  const loaded = completeWorkspaceOpen(loadingWithError, snapshot);

  assert.equal(loaded.workspaceRevision, 3);
  assert.equal(
    loaded.project.documents[firstDocument.id].title,
    'After snapshot',
  );
  assert.equal(loaded.watchError, null);
});

test('closes a selected buffer after its document was deleted externally', () => {
  const state = loadedState(10);
  const deleted = {
    ...state,
    project: {
      ...state.project,
      documents: {
        [firstDocument.id]: firstDocument,
      },
    },
  };
  const closed = closeSelectedDocument(deleted, secondDocument.id);
  assert.equal(closed.selectedDocumentId, null);
  assert.equal(closed.project.documents[secondDocument.id], undefined);
  assert.strictEqual(closeSelectedDocument(closed, secondDocument.id), closed);
});
