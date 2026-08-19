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
  writeResultIsStale,
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

test('does not flag a successful save as stale when the backend revision advanced past the request', () => {
  assert.equal(writeResultIsStale(6, 5), false);
  assert.equal(writeResultIsStale(101, 100), false);
});

test('flags a write result as stale only when the backend revision failed to move past the request', () => {
  assert.equal(writeResultIsStale(5, 5), true);
  assert.equal(writeResultIsStale(4, 5), true);
});

test('an unrelated concurrent patch racing in over the event channel must not turn a successful save into a false conflict', () => {
  // Reproduces the reported "Markdown documents cannot be saved" failure.
  // The backend serializes every workspace-revision bump behind a single
  // write lock, so a save that started when the client believed the
  // revision was 5 and that the backend committed as revision 6 is
  // unambiguously successful and newer than the request.
  //
  // Before this fix, MarkdownEditor.svelte's save() additionally compared
  // the backend's result against the *live* reactive `workspaceRevision`
  // store value. That value travels over a separate 'workspace-patch'
  // event channel and can already have ticked forward -- for example
  // because of an edit to a completely unrelated document -- by the time
  // the save() promise resolves. The old inline predicate below is kept
  // here only to document the bug being fixed; production code no longer
  // contains it.
  const resultRevision = 6;
  const requestRevision = 5;
  const racingLiveRevision = 7; // arrived via a separate channel mid-flight

  const oldPredicateFlaggedThisAsAConflict =
    resultRevision <= requestRevision || resultRevision < racingLiveRevision;
  assert.equal(oldPredicateFlaggedThisAsAConflict, true);

  assert.equal(writeResultIsStale(resultRevision, requestRevision), false);
});
