import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { transformWithEsbuild } from 'vite';

const source = await readFile(
  new URL('./ui-behavior.ts', import.meta.url),
  'utf8',
);
const transformed = await transformWithEsbuild(source, 'ui-behavior.ts', {
  format: 'esm',
  loader: 'ts',
  target: 'es2022',
});
const moduleUrl = `data:text/javascript;base64,${Buffer.from(transformed.code).toString('base64')}`;
const {
  beginLazyViewLoad,
  completeLazyViewLoad,
  createLazyViewState,
  failLazyViewLoad,
  lazyViewPresentation,
  lazyViewShouldLoad,
  mapFitDuration,
  saveShortcutAction,
} = await import(moduleUrl);

test('hides a delayed lazy-view resolve after switching away', async () => {
  let active = true;
  let state = beginLazyViewLoad(createLazyViewState());
  const requestId = state.requestId;
  let resolveLoad;
  const pending = new Promise((resolve) => {
    resolveLoad = resolve;
  }).then(() => {
    state = completeLazyViewLoad(state, requestId);
  });

  active = false;
  resolveLoad();
  await pending;
  assert.equal(state.status, 'ready');
  assert.equal(lazyViewPresentation(active, state), 'ready-hidden');
});

test('hides a delayed lazy-view rejection after switching away', async () => {
  let active = true;
  let state = beginLazyViewLoad(createLazyViewState());
  const requestId = state.requestId;
  let rejectLoad;
  const pending = new Promise((_, reject) => {
    rejectLoad = reject;
  }).catch((error) => {
    state = failLazyViewLoad(state, requestId, String(error));
  });

  active = false;
  rejectLoad(new Error('chunk failed'));
  await pending;
  assert.equal(state.status, 'error');
  assert.equal(lazyViewPresentation(active, state), 'hidden');
});

test('retains one ready map instance, layout, and viewport across view switches', () => {
  let active = true;
  let state = createLazyViewState();
  let moduleLoads = 0;
  let mounts = 0;
  let layoutLoads = 0;
  let instance = null;

  const initialize = () => {
    if (!lazyViewShouldLoad(state)) return;
    moduleLoads += 1;
    state = beginLazyViewLoad(state);
  };
  const render = () => {
    const presentation = lazyViewPresentation(active, state);
    if (presentation !== 'ready-visible' && presentation !== 'ready-hidden') {
      return;
    }
    if (!instance) {
      mounts += 1;
      instance = { viewportToken: 'pan-42', layoutReady: false };
    }
    if (active && !instance.layoutReady) {
      layoutLoads += 1;
      instance.layoutReady = true;
    }
  };

  initialize();
  const requestId = state.requestId;
  state = completeLazyViewLoad(state, requestId);
  render();
  const retainedInstance = instance;

  active = false;
  render();
  initialize();
  active = true;
  render();
  initialize();

  assert.equal(moduleLoads, 1);
  assert.equal(mounts, 1);
  assert.equal(layoutLoads, 1);
  assert.equal(instance, retainedInstance);
  assert.equal(instance.viewportToken, 'pan-42');
});

test('keeps pending and failed map states unmounted while editor is active', () => {
  const loading = beginLazyViewLoad(createLazyViewState());
  const failed = failLazyViewLoad(loading, loading.requestId, 'chunk failed');
  let mounts = 0;

  for (const state of [loading, failed]) {
    const presentation = lazyViewPresentation(false, state);
    if (presentation === 'ready-visible' || presentation === 'ready-hidden') {
      mounts += 1;
    }
    assert.equal(presentation, 'hidden');
  }

  assert.equal(mounts, 0);
});

test('ignores stale lazy-view completions', () => {
  const first = beginLazyViewLoad(createLazyViewState());
  const second = beginLazyViewLoad(first);
  assert.equal(completeLazyViewLoad(second, first.requestId), second);
  assert.equal(failLazyViewLoad(second, first.requestId, 'old'), second);
});

test('blocks modified and modal save shortcuts without invoking save', () => {
  const base = {
    altKey: false,
    ctrlKey: true,
    metaKey: false,
    shiftKey: false,
    key: 's',
  };
  assert.equal(saveShortcutAction(base, true), 'save');
  assert.equal(saveShortcutAction({ ...base, shiftKey: true }, true), 'block');
  assert.equal(saveShortcutAction(base, false), 'block');
  assert.equal(saveShortcutAction({ ...base, key: 'x' }, true), 'ignore');
});

test('fits instantly when reduced motion is requested', () => {
  assert.equal(mapFitDuration(false), 180);
  assert.equal(mapFitDuration(true), 0);
});
