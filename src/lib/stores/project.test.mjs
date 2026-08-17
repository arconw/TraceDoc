import assert from 'node:assert/strict';
import { readFile, unlink, writeFile } from 'node:fs/promises';
import test from 'node:test';
import { get } from 'svelte/store';
import { transformWithEsbuild } from 'vite';

async function loadProjectStoreModule() {
  const uniqueId = `${process.pid}-${Math.random().toString(36).slice(2)}`;
  const stateSource = await readFile(
    new URL('./project-state.ts', import.meta.url),
    'utf8',
  );
  const stateTransformed = await transformWithEsbuild(
    stateSource,
    'project-state.ts',
    { format: 'esm', loader: 'ts', target: 'es2022' },
  );
  const stateFileUrl = new URL(
    `./.generated-project-state-${uniqueId}.mjs`,
    import.meta.url,
  );
  await writeFile(stateFileUrl, stateTransformed.code, 'utf8');

  const storeSource = await readFile(
    new URL('./project.ts', import.meta.url),
    'utf8',
  );
  const storeTransformed = await transformWithEsbuild(
    storeSource,
    'project.ts',
    { format: 'esm', loader: 'ts', target: 'es2022' },
  );
  const rewrittenCode = storeTransformed.code.replace(
    '"./project-state"',
    `"./${stateFileUrl.pathname.split('/').pop()}"`,
  );
  const storeFileUrl = new URL(
    `./.generated-project-${uniqueId}.mjs`,
    import.meta.url,
  );
  await writeFile(storeFileUrl, rewrittenCode, 'utf8');

  try {
    return await import(storeFileUrl.href);
  } finally {
    await unlink(stateFileUrl).catch(() => {});
    await unlink(storeFileUrl).catch(() => {});
  }
}

const { createProjectStore } = await loadProjectStoreModule();

function snapshotFixture(rootPath) {
  return {
    workspaceGeneration: 1,
    workspaceRevision: 1,
    project: {
      rootPath,
      folders: {
        'folder:.': {
          id: 'folder:.',
          name: 'workspace',
          path: '',
          parentId: null,
          childFolderIds: [],
          documentIds: ['document:a.md'],
        },
      },
      documents: {
        'document:a.md': {
          id: 'document:a.md',
          name: 'a.md',
          title: 'A',
          headings: [],
          path: 'a.md',
          parentId: 'folder:.',
        },
      },
      links: [],
    },
  };
}

test('openFolder loads the scanned document tree after a valid folder is selected', async () => {
  const invokeCalls = [];
  const store = createProjectStore({
    open: async () => '/workspace',
    invoke: async (command, args) => {
      invokeCalls.push([command, args]);
      return snapshotFixture('/workspace');
    },
    listen: async () => () => {},
  });

  await store.openFolder();

  const state = get(store);
  assert.equal(state.status, 'loaded');
  assert.equal(state.project.rootPath, '/workspace');
  assert.ok(state.project.documents['document:a.md']);
  assert.deepEqual(invokeCalls, [
    ['open_workspace', { rootPath: '/workspace' }],
  ]);
});

test('openFolder leaves the current workspace unchanged when the picker is cancelled', async () => {
  let invokeCount = 0;
  let cancelPick = false;
  const store = createProjectStore({
    open: async () => (cancelPick ? null : '/workspace'),
    invoke: async () => {
      invokeCount += 1;
      return snapshotFixture('/workspace');
    },
    listen: async () => () => {},
  });

  await store.openFolder();
  const loaded = get(store);
  assert.equal(loaded.status, 'loaded');

  cancelPick = true;
  await store.openFolder();

  assert.equal(invokeCount, 1);
  assert.deepEqual(get(store), loaded);
});

test('openFolder surfaces a dialog failure as an actionable error', async () => {
  const store = createProjectStore({
    open: async () => {
      throw new Error('Permission denied opening the folder picker');
    },
    invoke: async () => {
      throw new Error('should not be invoked');
    },
    listen: async () => () => {},
  });

  await store.openFolder();

  assert.deepEqual(get(store), {
    status: 'error',
    message: 'Permission denied opening the folder picker',
  });
});

test('openFolder surfaces a backend scan failure as an actionable error', async () => {
  const store = createProjectStore({
    open: async () => '/missing-workspace',
    invoke: async () => {
      throw new Error(
        "Unable to access workspace '/missing-workspace': Permission denied",
      );
    },
    listen: async () => () => {},
  });

  await store.openFolder();

  const state = get(store);
  assert.equal(state.status, 'error');
  assert.match(state.message, /Unable to access workspace/);
});
