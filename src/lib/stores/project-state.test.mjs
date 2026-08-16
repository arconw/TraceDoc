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
const { applyDocumentIndex } = await import(moduleUrl);

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
    selectedDocumentId: secondDocument.id,
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
