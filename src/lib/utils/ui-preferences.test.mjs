import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { transformWithEsbuild } from 'vite';

const source = await readFile(
  new URL('./ui-preferences.ts', import.meta.url),
  'utf8',
);
const transformed = await transformWithEsbuild(source, 'ui-preferences.ts', {
  format: 'esm',
  loader: 'ts',
  target: 'es2022',
});
const moduleUrl = `data:text/javascript;base64,${Buffer.from(transformed.code).toString('base64')}`;
const {
  adjustSidebarWidth,
  clampSidebarWidth,
  defaultUiPreferences,
  parseUiPreferences,
  sidebarMaximumWidth,
} = await import(moduleUrl);

test('uses defaults for missing or malformed preferences', () => {
  assert.deepEqual(parseUiPreferences(null), defaultUiPreferences);
  assert.deepEqual(parseUiPreferences('{'), defaultUiPreferences);
});

test('validates the active view and clamps sidebar width', () => {
  assert.deepEqual(
    parseUiPreferences('{"activeView":"map","sidebarWidth":900}'),
    { activeView: 'map', sidebarWidth: 384 },
  );
  assert.deepEqual(
    parseUiPreferences('{"activeView":"other","sidebarWidth":20}'),
    { activeView: 'editor', sidebarWidth: 192 },
  );
  assert.equal(clampSidebarWidth(240), 240);
});

test('uses one responsive width for state, rendering, and accessibility', () => {
  const persisted = parseUiPreferences(
    '{"activeView":"editor","sidebarWidth":384}',
  );
  assert.equal(sidebarMaximumWidth(720), 273.6);
  assert.equal(clampSidebarWidth(persisted.sidebarWidth, 720), 273.6);
  assert.equal(adjustSidebarWidth(273.6, 'decrease', 720), 257.6);
  assert.equal(adjustSidebarWidth(257.6, 'maximum', 720), 273.6);
  assert.equal(clampSidebarWidth(384, 1000), 380);
  assert.equal(clampSidebarWidth(384, 1200), 384);
});
