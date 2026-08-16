import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { transformWithEsbuild } from 'vite';

const source = await readFile(
  new URL('./viewport-lifecycle.ts', import.meta.url),
  'utf8',
);
const transformed = await transformWithEsbuild(
  source,
  'viewport-lifecycle.ts',
  { format: 'esm', loader: 'ts', target: 'es2022' },
);
const moduleUrl = `data:text/javascript;base64,${Buffer.from(transformed.code).toString('base64')}`;
const {
  captureMapViewport,
  mapViewportMountAction,
  mapViewportRequestIsCurrent,
} = await import(moduleUrl);

test('captures and restores a valid viewport for the same graph revision', () => {
  const calls = [];
  const flow = {
    fit: () => calls.push(['fit']),
    restore: (viewport) => calls.push(['setViewport', viewport, 0]),
  };
  const viewport = { x: -120.5, y: 48.25, zoom: 1.35 };
  const saved = captureMapViewport('graph-a', 7, viewport);
  const action = mapViewportMountAction(saved, 'graph-a', 7, false);

  if (action.kind === 'restore') flow.restore(action.viewport);
  else flow.fit();

  assert.deepEqual(calls, [['setViewport', viewport, 0]]);
});

test('changed revisions and explicit fit use fit without altering its behavior', () => {
  const calls = [];
  const apply = (action) => {
    if (action.kind === 'fit') calls.push('fit');
    else calls.push('setViewport');
  };
  const saved = captureMapViewport('graph-a', 7, {
    x: 10,
    y: 20,
    zoom: 0.8,
  });
  apply(mapViewportMountAction(saved, 'graph-a', 8, false));
  apply(mapViewportMountAction(saved, 'graph-b', 7, false));
  apply(mapViewportMountAction(saved, 'graph-a', 7, true));
  assert.deepEqual(calls, ['fit', 'fit', 'fit']);
});

test('ignores invalid viewports and stale remount callbacks', () => {
  const invalid = captureMapViewport('graph-a', 4, {
    x: 0,
    y: 0,
    zoom: Number.NaN,
  });
  assert.equal(invalid, null);
  assert.deepEqual(mapViewportMountAction(invalid, 'graph-a', 4, false), {
    kind: 'fit',
  });
  assert.equal(captureMapViewport('graph-a', 4, { x: 0, y: 0, zoom: 0 }), null);
  assert.equal(mapViewportRequestIsCurrent(3, 4, 3, 4, true), true);
  assert.equal(mapViewportRequestIsCurrent(3, 4, 4, 4, true), false);
  assert.equal(mapViewportRequestIsCurrent(3, 4, 3, 5, true), false);
  assert.equal(mapViewportRequestIsCurrent(3, 4, 3, 4, false), false);
});
