import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { transformWithEsbuild } from 'vite';

const source = await readFile(
  new URL('./request-state.ts', import.meta.url),
  'utf8',
);
const transformed = await transformWithEsbuild(source, 'request-state.ts', {
  format: 'esm',
  loader: 'ts',
  target: 'es2022',
});
const moduleUrl = `data:text/javascript;base64,${Buffer.from(transformed.code).toString('base64')}`;
const {
  closesDeletedBufferWithoutDiskAccess,
  editorReadIsCurrent,
  retainedLocalBaseline,
} = await import(moduleUrl);

test('rejects slow keep-mine success and catch paths after switching documents', () => {
  const request = {
    version: 7,
    documentId: 'document:first.md',
    workspaceGeneration: 3,
  };
  assert.equal(
    editorReadIsCurrent(request, 8, 'document:second.md', 3, 3),
    false,
  );
  assert.equal(
    editorReadIsCurrent(
      request,
      8,
      'document:second.md',
      3,
      request.workspaceGeneration,
    ),
    false,
  );
  assert.equal(
    editorReadIsCurrent(request, 7, 'document:first.md', 4, 3),
    false,
  );
  assert.equal(
    editorReadIsCurrent(request, 7, 'document:first.md', 3, 3),
    true,
  );
});

test('keeps a clean local A dirty after disk changes to B and Keep mine', () => {
  const retained = retainedLocalBaseline('# A', '# B', 'token-b');
  assert.deepEqual(retained, {
    savedContent: '# B',
    savedContentToken: 'token-b',
    dirty: true,
  });
});

test('keeps an edit back to the old baseline dirty after a pending save conflicts', () => {
  const pendingSaveContent = '# Local pending';
  const editedBackContent = '# A';
  const retained = retainedLocalBaseline(
    editedBackContent,
    '# External B',
    'token-external-b',
  );
  assert.notEqual(pendingSaveContent, editedBackContent);
  assert.equal(retained.savedContent, '# External B');
  assert.equal(retained.dirty, true);
});

test('closes an externally deleted dirty buffer without reading the missing path', () => {
  assert.equal(closesDeletedBufferWithoutDiskAccess('deleted'), true);
  assert.equal(closesDeletedBufferWithoutDiskAccess('modified'), false);
  assert.equal(closesDeletedBufferWithoutDiskAccess(null), false);
});
