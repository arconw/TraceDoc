import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { transformWithEsbuild } from 'vite';

const source = await readFile(
  new URL('./app-identity.ts', import.meta.url),
  'utf8',
);
const transformed = await transformWithEsbuild(source, 'app-identity.ts', {
  format: 'esm',
  loader: 'ts',
  target: 'es2022',
});
const moduleUrl = `data:text/javascript;base64,${Buffer.from(transformed.code).toString('base64')}`;
const { applyAppInfo } = await import(moduleUrl);

test('updates the header identity and document title after app info resolves', async () => {
  let resolveInfo;
  let headerName = 'Fallback';
  let documentTitle = 'Fallback';
  const request = () =>
    new Promise((resolve) => {
      resolveInfo = resolve;
    });

  const pending = applyAppInfo(
    request,
    (name) => {
      headerName = name;
    },
    (title) => {
      documentTitle = title;
    },
  );

  assert.equal(headerName, 'Fallback');
  assert.equal(documentTitle, 'Fallback');
  resolveInfo({ name: 'TraceDoc', version: '0.1.0' });
  await pending;
  assert.equal(headerName, 'TraceDoc');
  assert.equal(documentTitle, 'TraceDoc 0.1.0');
});
