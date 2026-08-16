import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';
import test from 'node:test';
import ELK from 'elkjs/lib/elk.bundled.js';
import { transformWithEsbuild } from 'vite';

async function loadTypeScript(relativePath) {
  const url = new URL(relativePath, import.meta.url);
  const source = await readFile(url, 'utf8');
  const transformed = await transformWithEsbuild(source, url.pathname, {
    format: 'esm',
    loader: 'ts',
    target: 'es2022',
  });
  const moduleUrl = `data:text/javascript;base64,${Buffer.from(transformed.code).toString('base64')}`;
  return import(moduleUrl);
}

const { projectToMapGraph } = await loadTypeScript('./project-graph.ts');
const { layoutMapGraph } = await loadTypeScript('./elk-layout.ts');

function nestedProject() {
  return {
    rootPath: '/workspace',
    folders: {
      'folder:.': {
        id: 'folder:.',
        name: 'workspace',
        path: '',
        parentId: null,
        childFolderIds: ['folder:backend', 'folder:frontend'],
        documentIds: ['document:readme.md'],
      },
      'folder:frontend': {
        id: 'folder:frontend',
        name: 'frontend',
        path: 'frontend',
        parentId: 'folder:.',
        childFolderIds: ['folder:frontend/state'],
        documentIds: ['document:frontend/app.md'],
      },
      'folder:frontend/state': {
        id: 'folder:frontend/state',
        name: 'state',
        path: 'frontend/state',
        parentId: 'folder:frontend',
        childFolderIds: [],
        documentIds: ['document:frontend/state/store.md'],
      },
      'folder:backend': {
        id: 'folder:backend',
        name: 'backend',
        path: 'backend',
        parentId: 'folder:.',
        childFolderIds: [],
        documentIds: ['document:backend/api.md'],
      },
    },
    documents: {
      'document:readme.md': document('readme.md', 'folder:.', 'Read me'),
      'document:frontend/app.md': document(
        'frontend/app.md',
        'folder:frontend',
        'Application',
      ),
      'document:frontend/state/store.md': document(
        'frontend/state/store.md',
        'folder:frontend/state',
        'State store',
      ),
      'document:backend/api.md': document(
        'backend/api.md',
        'folder:backend',
        'API',
      ),
    },
    links: [
      link(
        'link:app-api',
        'document:frontend/app.md',
        'document:backend/api.md',
        true,
      ),
      link(
        'link:app-store',
        'document:frontend/app.md',
        'document:frontend/state/store.md',
        true,
      ),
      link('link:missing', 'document:readme.md', null, false),
    ],
  };
}

function document(path, parentId, title) {
  const name = path.split('/').at(-1);
  return {
    id: `document:${path}`,
    name,
    title,
    headings: [],
    path,
    parentId,
  };
}

function link(id, sourceDocumentId, targetDocumentId, resolved) {
  return {
    id,
    sourceDocumentId,
    targetDocumentId,
    rawTarget: targetDocumentId ?? 'missing.md',
    resolved,
    unresolvedReason: resolved ? null : 'Target document was not found',
  };
}

function syntheticProject(documentCount, linkCount) {
  const folders = {
    'folder:.': {
      id: 'folder:.',
      name: 'synthetic',
      path: '',
      parentId: null,
      childFolderIds: ['folder:area-0', 'folder:area-1'],
      documentIds: [],
    },
  };
  const documents = {};

  for (let areaIndex = 0; areaIndex < 2; areaIndex += 1) {
    const areaId = `folder:area-${areaIndex}`;
    const moduleIds = [];
    folders[areaId] = {
      id: areaId,
      name: `area-${areaIndex}`,
      path: `area-${areaIndex}`,
      parentId: 'folder:.',
      childFolderIds: moduleIds,
      documentIds: [],
    };
    for (let moduleIndex = 0; moduleIndex < 5; moduleIndex += 1) {
      const moduleId = `folder:area-${areaIndex}/module-${moduleIndex}`;
      moduleIds.push(moduleId);
      folders[moduleId] = {
        id: moduleId,
        name: `module-${moduleIndex}`,
        path: `area-${areaIndex}/module-${moduleIndex}`,
        parentId: areaId,
        childFolderIds: [],
        documentIds: [],
      };
    }
  }

  for (let index = 0; index < documentCount; index += 1) {
    const areaIndex = index % 2;
    const moduleIndex = index % 5;
    const path = `area-${areaIndex}/module-${moduleIndex}/document-${String(index).padStart(3, '0')}.md`;
    const parentId = `folder:area-${areaIndex}/module-${moduleIndex}`;
    const item = document(path, parentId, `Document ${index}`);
    documents[item.id] = item;
    folders[parentId].documentIds.push(item.id);
  }

  const links = Array.from({ length: linkCount }, (_, index) => {
    const sourceIndex = index % documentCount;
    const targetIndex = (index * 17 + 23) % documentCount;
    return link(
      `link:${String(index).padStart(4, '0')}`,
      Object.keys(documents)[sourceIndex],
      Object.keys(documents)[targetIndex],
      true,
    );
  });

  return { rootPath: '/synthetic', folders, documents, links };
}

test('derives nested folders, documents, and only resolved internal links', () => {
  const graph = projectToMapGraph(nestedProject());

  assert.deepEqual(graph.rootFolderIds, ['folder:.']);
  assert.deepEqual(graph.folders['folder:.'].childFolderIds, [
    'folder:backend',
    'folder:frontend',
  ]);
  assert.equal(
    graph.documents['document:frontend/app.md'].title,
    'Application',
  );
  assert.deepEqual(
    graph.links.map((item) => item.id),
    ['link:app-api', 'link:app-store'],
  );
});

test('builds and flattens a deterministic nested ELK layout', async () => {
  const graph = projectToMapGraph(nestedProject());
  const first = await layoutMapGraph(graph, new ELK());
  const second = await layoutMapGraph(graph, new ELK());
  const positions = (layout) =>
    layout.nodes.map((node) => [
      node.id,
      node.parentId ?? null,
      node.position.x,
      node.position.y,
      node.width,
      node.height,
    ]);

  assert.equal(first.edges.length, 2);
  assert.equal(
    first.nodes.find((node) => node.id === 'folder:frontend/state').parentId,
    'folder:frontend',
  );
  assert.equal(
    first.nodes.find((node) => node.id === 'document:frontend/state/store.md')
      .parentId,
    'folder:frontend/state',
  );
  assert.deepEqual(positions(first), positions(second));
});

test(
  'lays out synthetic maps with 100 and 500 documents',
  { timeout: 60_000 },
  async (context) => {
    for (const [documentCount, linkCount] of [
      [100, 200],
      [500, 1000],
    ]) {
      const started = performance.now();
      const graph = projectToMapGraph(
        syntheticProject(documentCount, linkCount),
      );
      const adaptedAt = performance.now();
      const layout = await layoutMapGraph(graph, new ELK());
      const completedAt = performance.now();

      assert.equal(layout.nodes.length, documentCount + 13);
      assert.equal(layout.edges.length, linkCount);
      assert.ok(layout.nodes.every((node) => Number.isFinite(node.position.x)));
      assert.ok(layout.nodes.every((node) => Number.isFinite(node.position.y)));
      context.diagnostic(
        `${documentCount} documents, ${linkCount} links: adapter ${(adaptedAt - started).toFixed(1)} ms, ELK ${(completedAt - adaptedAt).toFixed(1)} ms`,
      );
    }
  },
);
