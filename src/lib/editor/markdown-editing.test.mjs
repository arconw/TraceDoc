import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { transformWithEsbuild } from 'vite';

const source = await readFile(
  new URL('./markdown-editing.ts', import.meta.url),
  'utf8',
);
const transformed = await transformWithEsbuild(source, 'markdown-editing.ts', {
  format: 'esm',
  loader: 'ts',
  target: 'es2022',
});
const moduleUrl = `data:text/javascript;base64,${Buffer.from(transformed.code).toString('base64')}`;
const { markdownToggle } = await import(moduleUrl);

function applyChanges(content, changes) {
  return [...changes]
    .sort((left, right) => right.from - left.from)
    .reduce(
      (value, change) =>
        value.slice(0, change.from) + change.insert + value.slice(change.to),
      content,
    );
}

test('wraps a forward selection without changing its selected text', () => {
  const result = markdownToggle('plain text', 0, 5, '**');
  assert.equal(applyChanges('plain text', result.changes), '**plain** text');
  assert.deepEqual([result.anchor, result.head], [2, 7]);
});

test('preserves a backward selection while wrapping', () => {
  const result = markdownToggle('plain text', 5, 0, '*');
  assert.equal(applyChanges('plain text', result.changes), '*plain* text');
  assert.deepEqual([result.anchor, result.head], [6, 1]);
});

test('toggles existing surrounding markers off', () => {
  const result = markdownToggle('**plain** text', 2, 7, '**');
  assert.equal(applyChanges('**plain** text', result.changes), 'plain text');
  assert.deepEqual([result.anchor, result.head], [0, 5]);
});

test('toggles a selection that includes its bold markers off', () => {
  const result = markdownToggle('**plain**', 0, 9, '**');
  assert.equal(applyChanges('**plain**', result.changes), 'plain');
  assert.deepEqual([result.anchor, result.head], [0, 5]);
});

test('toggles a selection that includes its italic markers off', () => {
  const result = markdownToggle('*plain*', 0, 7, '*');
  assert.equal(applyChanges('*plain*', result.changes), 'plain');
  assert.deepEqual([result.anchor, result.head], [0, 5]);
});

test('preserves selection direction when selected markers are removed', () => {
  const result = markdownToggle('**plain**', 9, 0, '**');
  assert.equal(applyChanges('**plain**', result.changes), 'plain');
  assert.deepEqual([result.anchor, result.head], [5, 0]);
});

test('inserts an editable marker pair at an empty cursor', () => {
  const result = markdownToggle('text', 2, 2, '**');
  assert.equal(applyChanges('text', result.changes), 'te****xt');
  assert.deepEqual([result.anchor, result.head], [4, 4]);
});

test('does not create asymmetric or whitespace-delimited emphasis', () => {
  assert.deepEqual(markdownToggle('**plain', 2, 7, '**').changes, []);
  assert.deepEqual(markdownToggle(' plain ', 0, 7, '**').changes, []);
  assert.deepEqual(markdownToggle('one\n\ntwo', 0, 8, '*').changes, []);
});

test('nests italic around an existing strong selection without crossing it', () => {
  const result = markdownToggle('**plain**', 0, 9, '*');
  assert.equal(applyChanges('**plain**', result.changes), '***plain***');
  assert.deepEqual([result.anchor, result.head], [1, 10]);
});
