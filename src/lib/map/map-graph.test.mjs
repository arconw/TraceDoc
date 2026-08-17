import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { performance } from 'node:perf_hooks';
import test from 'node:test';
import { build } from 'esbuild';
import ELK from 'elkjs/lib/elk.bundled.js';

async function loadTypeScript(relativePath) {
  const url = new URL(relativePath, import.meta.url);
  await readFile(url, 'utf8');
  const result = await build({
    entryPoints: [url.pathname],
    bundle: true,
    write: false,
    format: 'esm',
    platform: 'node',
    packages: 'external',
    target: 'node22',
  });
  const moduleUrl = `data:text/javascript;base64,${Buffer.from(result.outputFiles[0].text).toString('base64')}`;
  return import(moduleUrl);
}

const { mapLayoutSignature, projectToMapGraph } =
  await loadTypeScript('./project-graph.ts');
const { layoutMapGraph } = await loadTypeScript('./elk-layout.ts');
const { routeMapLinks, segmentIntersectsRectInterior } =
  await loadTypeScript('./routing.ts');
const {
  beginMapLayout,
  beginQueuedMapLayout,
  cancelQueuedMapLayout,
  completeQueuedMapLayout,
  completeEmptyMapLayout,
  completeMapLayout,
  createMapEdgeTraceState,
  createMapLayoutSession,
  createMapLayoutRequestState,
  effectiveMapEdgeTraceId,
  failMapLayout,
  mapLayoutIsInteractive,
  queueMapLayout,
  reduceMapEdgeTrace,
  retryQueuedMapLayout,
} = await loadTypeScript('./map-view-state.ts');

test('ignores body-only changes in the map layout signature', () => {
  const project = nestedProject();
  const bodyOnly = structuredClone(project);
  bodyOnly.documents['document:readme.md'].headings = [
    { level: 2, text: 'Body heading' },
  ];
  assert.equal(mapLayoutSignature(bodyOnly), mapLayoutSignature(project));

  bodyOnly.documents['document:readme.md'].title = 'Layout title changed';
  assert.notEqual(mapLayoutSignature(bodyOnly), mapLayoutSignature(project));

  const graphChange = structuredClone(project);
  graphChange.links[0].resolved = false;
  graphChange.links[0].targetDocumentId = null;
  assert.notEqual(mapLayoutSignature(graphChange), mapLayoutSignature(project));
});

test('coalesces visible bursts and defers hidden layouts to the latest graph', () => {
  let state = createMapLayoutRequestState();
  state = queueMapLayout(state, 'graph-a', true);
  state = queueMapLayout(state, 'graph-b', true);
  state = queueMapLayout(state, 'graph-c', true);
  state = beginQueuedMapLayout(state);
  assert.equal(state.activeSignature, 'graph-c');
  assert.equal(state.requestCount, 1);

  state = queueMapLayout(state, 'hidden-a', false);
  state = queueMapLayout(state, 'hidden-b', false);
  assert.equal(state.requestCount, 1);
  assert.equal(state.pendingSignature, 'hidden-b');
  state = queueMapLayout(state, 'hidden-b', true);
  state = beginQueuedMapLayout(state);
  assert.equal(state.activeSignature, 'hidden-b');
  assert.equal(state.requestCount, 2);

  state = queueMapLayout(state, 'hidden-b', true);
  state = beginQueuedMapLayout(state);
  assert.equal(state.requestCount, 2);
});

test('cancels active layout work and starts only the latest queued job', () => {
  const jobs = [];
  let cancellations = 0;
  let state = createMapLayoutRequestState();
  const start = () => {
    state = beginQueuedMapLayout(state);
    jobs.push({ signature: state.activeSignature, cancelled: false });
  };
  const cancel = () => {
    jobs.at(-1).cancelled = true;
    cancellations += 1;
    state = cancelQueuedMapLayout(state);
  };

  state = queueMapLayout(state, 'graph-a', true);
  start();
  cancel();
  state = queueMapLayout(state, 'graph-b', true);
  state = queueMapLayout(state, 'graph-c', true);
  start();
  state = completeQueuedMapLayout(state);

  assert.deepEqual(jobs, [
    { signature: 'graph-a', cancelled: true },
    { signature: 'graph-c', cancelled: false },
  ]);
  assert.equal(cancellations, 1);
  assert.equal(state.running, false);

  state = queueMapLayout(state, 'graph-d', true);
  start();
  cancel();
  state = queueMapLayout(state, 'graph-d', false);
  assert.equal(cancellations, 2);
  assert.equal(state.pendingSignature, 'graph-d');
  assert.equal(state.running, false);
});

test('routes retry through cancellation for hide and signature changes', () => {
  let state = createMapLayoutRequestState();
  state = queueMapLayout(state, 'failed', true);
  state = beginQueuedMapLayout(state);
  state = completeQueuedMapLayout(state);

  state = retryQueuedMapLayout(state);
  state = beginQueuedMapLayout(state);
  assert.equal(state.running, true);
  state = cancelQueuedMapLayout(state);
  state = queueMapLayout(state, 'failed', false);
  assert.equal(state.running, false);
  assert.equal(state.pendingSignature, 'failed');

  state = queueMapLayout(state, 'failed', true);
  state = beginQueuedMapLayout(state);
  state = cancelQueuedMapLayout(state);
  state = queueMapLayout(state, 'changed', true);
  state = beginQueuedMapLayout(state);
  assert.equal(state.activeSignature, 'changed');
  assert.equal(state.requestCount, 4);
});

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

test('keeps pointer and keyboard edge traces independent', () => {
  const event = (state, source, edgeId) =>
    reduceMapEdgeTrace(state, { source, edgeId });

  let state = createMapEdgeTraceState();
  state = event(state, 'focus', 'AB');
  assert.equal(effectiveMapEdgeTraceId(state), 'AB');
  state = event(state, 'pointer', 'AC');
  assert.equal(effectiveMapEdgeTraceId(state), 'AC');
  state = event(state, 'pointer', null);
  assert.equal(effectiveMapEdgeTraceId(state), 'AB');
  assert.equal(state.focusedEdgeId, 'AB');

  state = event(state, 'pointer', 'AB');
  assert.equal(effectiveMapEdgeTraceId(state), 'AB');
  state = event(state, 'pointer', null);
  assert.equal(effectiveMapEdgeTraceId(state), 'AB');

  state = event(state, 'pointer', 'AC');
  state = event(state, 'focus', null);
  assert.equal(effectiveMapEdgeTraceId(state), 'AC');
  state = event(state, 'pointer', null);
  assert.equal(effectiveMapEdgeTraceId(state), null);
});

test('exposes edge controls only for the current successful layout', () => {
  const projectA = { project: 'A', edges: ['AB'] };
  const projectB = { project: 'B', edges: ['BC', 'BD'] };
  let session = createMapLayoutSession();

  session = beginMapLayout(session, 'loading');
  const requestA = session.requestId;
  assert.equal(session.layout, null);
  assert.equal(mapLayoutIsInteractive(session, true, true), false);
  session = completeMapLayout(session, requestA, projectA);
  assert.equal(session.layout, projectA);
  assert.equal(mapLayoutIsInteractive(session, true, true), true);

  session = beginMapLayout(session, 'loading');
  const failedRequestB = session.requestId;
  assert.equal(session.layout, null);
  assert.equal(mapLayoutIsInteractive(session, true, true), false);
  session = failMapLayout(session, failedRequestB, 'layout failed');
  assert.equal(session.status, 'error');
  assert.equal(session.layout, null);
  assert.equal(mapLayoutIsInteractive(session, true, true), false);

  session = beginMapLayout(session, 'loading');
  const requestB = session.requestId;
  const staleResult = completeMapLayout(session, requestA, projectA);
  assert.equal(staleResult, session);
  assert.equal(staleResult.layout, null);
  session = completeMapLayout(session, requestB, projectB);
  assert.equal(session.layout, projectB);
  assert.equal(mapLayoutIsInteractive(session, true, true), true);
  assert.equal(mapLayoutIsInteractive(session, false, true), false);
  assert.equal(mapLayoutIsInteractive(session, true, false), false);

  session = beginMapLayout(session, 'loading');
  const requestC = session.requestId;
  session = failMapLayout(session, requestB, 'stale failure');
  assert.equal(session.requestId, requestC);
  assert.equal(session.status, 'loading');
  assert.equal(session.layout, null);

  session = completeEmptyMapLayout(session, requestC);
  assert.equal(session.status, 'empty');
  assert.equal(session.layout, null);
  assert.equal(mapLayoutIsInteractive(session, true, true), false);
});

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
    const round = Math.floor(index / documentCount);
    let targetIndex = (sourceIndex * 17 + 23 + round * 11) % documentCount;
    if (targetIndex === sourceIndex) {
      targetIndex = (targetIndex + round + 1) % documentCount;
    }
    return link(
      `link:${String(index).padStart(4, '0')}`,
      Object.keys(documents)[sourceIndex],
      Object.keys(documents)[targetIndex],
      true,
    );
  });

  return { rootPath: '/synthetic', folders, documents, links };
}

function fanProject(direction) {
  const item = syntheticProject(21, 0);
  const documentIds = Object.keys(item.documents);
  const hubId = documentIds.at(-1);
  item.links = documentIds
    .slice(0, 20)
    .map((documentId, index) =>
      direction === 'in'
        ? link(`link:fan-in-${index}`, documentId, hubId, true)
        : link(`link:fan-out-${index}`, hubId, documentId, true),
    );
  return item;
}

function assertOrthogonal(edge) {
  assert.ok(edge.data.points.length >= 2);
  for (let index = 1; index < edge.data.points.length; index += 1) {
    const previous = edge.data.points[index - 1];
    const current = edge.data.points[index];
    assert.ok(
      previous.x === current.x || previous.y === current.y,
      `${edge.id} contains a diagonal segment`,
    );
  }
}

function absoluteRectangles(layout) {
  const nodes = new Map(layout.nodes.map((node) => [node.id, node]));
  const positions = new Map();
  const positionFor = (node) => {
    if (positions.has(node.id)) return positions.get(node.id);
    const parent = node.parentId
      ? positionFor(nodes.get(node.parentId))
      : { x: 0, y: 0 };
    const position = {
      x: parent.x + node.position.x,
      y: parent.y + node.position.y,
    };
    positions.set(node.id, position);
    return position;
  };
  return Object.fromEntries(
    layout.nodes.map((node) => [
      node.id,
      {
        ...positionFor(node),
        width: node.width,
        height: node.height,
      },
    ]),
  );
}

function routeIntersectsRect(points, rect) {
  for (let index = 1; index < points.length; index += 1) {
    if (segmentIntersectsRectInterior(points[index - 1], points[index], rect)) {
      return true;
    }
  }
  return false;
}

function assertAvoidsDocumentInteriors(layout) {
  const rectangles = absoluteRectangles(layout);
  const documents = layout.nodes.filter(
    (node) => node.data.kind === 'document',
  );
  for (const edge of layout.edges) {
    for (const documentNode of documents) {
      assert.equal(
        routeIntersectsRect(edge.data.points, rectangles[documentNode.id]),
        false,
        `${edge.id} crosses ${documentNode.id}`,
      );
    }
  }
}

function ancestorIds(graph, folderId) {
  const result = [];
  let current = graph.folders[folderId];
  while (current) {
    result.push(current.id);
    current = current.parentId ? graph.folders[current.parentId] : null;
  }
  return result;
}

function expectedBoundaryIds(graph, edge) {
  const source = ancestorIds(graph, graph.documents[edge.source].parentId);
  const target = ancestorIds(graph, graph.documents[edge.target].parentId);
  while (
    source.length > 0 &&
    target.length > 0 &&
    source.at(-1) === target.at(-1)
  ) {
    source.pop();
    target.pop();
  }
  return [...source, ...target.reverse()];
}

function assertFolderGatewayPolicy(graph, layout) {
  const rectangles = absoluteRectangles(layout);
  for (const edge of layout.edges) {
    assert.deepEqual(
      edge.data.boundaryGateways.map((gateway) => gateway.folderId),
      expectedBoundaryIds(graph, edge),
    );
    const allowed = new Set([
      ...ancestorIds(graph, graph.documents[edge.source].parentId),
      ...ancestorIds(graph, graph.documents[edge.target].parentId),
    ]);
    for (const folder of Object.values(graph.folders)) {
      if (allowed.has(folder.id)) continue;
      assert.equal(
        routeIntersectsRect(edge.data.points, rectangles[folder.id]),
        false,
        `${edge.id} traverses unrelated ${folder.id}`,
      );
    }
    for (const gateway of edge.data.boundaryGateways) {
      const rect = rectangles[gateway.folderId];
      const onBoundary =
        (gateway.side === 'left' && gateway.point.x === rect.x) ||
        (gateway.side === 'right' && gateway.point.x === rect.x + rect.width) ||
        (gateway.side === 'top' && gateway.point.y === rect.y) ||
        (gateway.side === 'bottom' && gateway.point.y === rect.y + rect.height);
      assert.equal(onBoundary, true);
      assert.ok(
        edge.data.points.some(
          (point) => point.x === gateway.point.x && point.y === gateway.point.y,
        ),
      );
      const gatewayIndex = edge.data.points.findIndex(
        (point) => point.x === gateway.point.x && point.y === gateway.point.y,
      );
      const adjacent = [
        edge.data.points[gatewayIndex - 1],
        edge.data.points[gatewayIndex + 1],
      ];
      const insideCount = adjacent.filter(
        (point) =>
          point &&
          point.x > rect.x &&
          point.x < rect.x + rect.width &&
          point.y > rect.y &&
          point.y < rect.y + rect.height,
      ).length;
      assert.equal(insideCount, 1);
    }
  }
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
  assert.deepEqual(
    first.edges.map((edge) => edge.data.points),
    second.edges.map((edge) => edge.data.points),
  );
  first.edges.forEach(assertOrthogonal);
});

test(
  'fans 20 incoming and outgoing links across independent orthogonal lanes',
  { timeout: 60_000 },
  async () => {
    for (const direction of ['in', 'out']) {
      const layout = await layoutMapGraph(
        projectToMapGraph(fanProject(direction)),
        new ELK(),
      );
      assert.equal(layout.edges.length, 20);
      layout.edges.forEach(assertOrthogonal);
      const endpoints = layout.edges.map((edge) =>
        JSON.stringify(
          direction === 'in' ? edge.data.points.at(-1) : edge.data.points.at(0),
        ),
      );
      assert.equal(new Set(endpoints).size, 20);
      assert.ok(
        layout.edges.every((edge) => edge.sourceHandle && edge.targetHandle),
      );
      assertAvoidsDocumentInteriors(layout);
    }
  },
);

test(
  'routes ten cross-folder links through deterministic boundary gateways',
  { timeout: 60_000 },
  async () => {
    const graph = projectToMapGraph(syntheticProject(20, 10));
    const first = await layoutMapGraph(graph, new ELK());
    const second = await layoutMapGraph(graph, new ELK());

    assert.equal(first.edges.length, 10);
    assert.ok(
      first.edges.every((edge) => edge.data.boundaryGateways.length >= 2),
    );
    first.edges.forEach(assertOrthogonal);
    assertAvoidsDocumentInteriors(first);
    assertFolderGatewayPolicy(graph, first);
    assert.deepEqual(
      first.edges.map((edge) => edge.data.points),
      second.edges.map((edge) => edge.data.points),
    );
  },
);

test(
  'keeps a mixed dense graph finite, orthogonal, and independently traceable',
  { timeout: 60_000 },
  async () => {
    const graph = projectToMapGraph(syntheticProject(60, 240));
    const layout = await layoutMapGraph(graph, new ELK());

    assert.equal(layout.edges.length, 240);
    assert.equal(
      new Set(layout.edges.map((edge) => `${edge.source}\u0000${edge.target}`))
        .size,
      240,
    );
    layout.edges.forEach(assertOrthogonal);
    assertAvoidsDocumentInteriors(layout);
    assertFolderGatewayPolicy(graph, layout);
    assert.ok(
      layout.edges.every((edge) =>
        edge.data.points.every(
          (point) => Number.isFinite(point.x) && Number.isFinite(point.y),
        ),
      ),
    );
    assert.equal(new Set(layout.edges.map((edge) => edge.id)).size, 240);
  },
);

function routableGraph(folders, documents, links) {
  return {
    rootFolderIds: [folders[0].id],
    folders: Object.fromEntries(folders.map((item) => [item.id, item])),
    documents: Object.fromEntries(
      documents.map((item) => [item.id, item]),
    ),
    links,
  };
}

function routableDocument(id, parentId, title) {
  return { id, name: `${id}.md`, title, path: `${id}.md`, parentId, headings: [] };
}

function routableLink(id, sourceDocumentId, targetDocumentId) {
  return {
    id,
    sourceDocumentId,
    targetDocumentId,
    rawTarget: targetDocumentId,
    resolved: true,
    unresolvedReason: null,
  };
}

function boundaryNoiseFixture() {
  const folder = {
    id: 'folder:zone',
    name: 'zone',
    path: 'zone',
    parentId: null,
    childFolderIds: [],
    documentIds: ['document:source', 'document:target'],
  };
  const documents = [
    routableDocument('document:source', folder.id, 'Source'),
    routableDocument('document:target', folder.id, 'Target'),
  ];
  const graph = routableGraph([folder], documents, [
    routableLink('link:source-target', 'document:source', 'document:target'),
  ]);
  const nodes = [
    {
      id: folder.id,
      position: { x: 0, y: 0 },
      width: 400,
      height: 300,
      data: { kind: 'folder' },
    },
    {
      id: 'document:target',
      parentId: folder.id,
      position: { x: 0.57, y: 20 },
      width: 71.2,
      height: 50,
      data: { kind: 'document' },
    },
    {
      id: 'document:source',
      parentId: folder.id,
      position: { x: 200, y: 150 },
      width: 100,
      height: 50,
      data: { kind: 'document' },
    },
  ];
  return { graph, nodes };
}

function unroutableZoneFixture() {
  const folder = {
    id: 'folder:zone',
    name: 'zone',
    path: 'zone',
    parentId: null,
    childFolderIds: [],
    documentIds: [
      'document:trapped-a',
      'document:trapped-b',
      'document:blocker',
      'document:good-a',
      'document:good-b',
    ],
  };
  const documents = [
    routableDocument('document:trapped-a', folder.id, 'Trapped A'),
    routableDocument('document:trapped-b', folder.id, 'Trapped B'),
    routableDocument('document:blocker', folder.id, 'Blocker'),
    routableDocument('document:good-a', folder.id, 'Good A'),
    routableDocument('document:good-b', folder.id, 'Good B'),
  ];
  const graph = routableGraph([folder], documents, [
    routableLink('link:trapped', 'document:trapped-a', 'document:trapped-b'),
    routableLink('link:good', 'document:good-a', 'document:good-b'),
  ]);
  const nodes = [
    {
      id: folder.id,
      position: { x: 0, y: 0 },
      width: 400,
      height: 400,
      data: { kind: 'folder' },
    },
    {
      id: 'document:trapped-a',
      parentId: folder.id,
      position: { x: 300, y: 300 },
      width: 60,
      height: 30,
      data: { kind: 'document' },
    },
    {
      id: 'document:blocker',
      parentId: folder.id,
      position: { x: 20, y: 20 },
      width: 200,
      height: 200,
      data: { kind: 'document' },
    },
    {
      id: 'document:trapped-b',
      parentId: folder.id,
      position: { x: 90, y: 90 },
      width: 60,
      height: 60,
      data: { kind: 'document' },
    },
    {
      id: 'document:good-a',
      parentId: folder.id,
      position: { x: 20, y: 320 },
      width: 60,
      height: 30,
      data: { kind: 'document' },
    },
    {
      id: 'document:good-b',
      parentId: folder.id,
      position: { x: 320, y: 20 },
      width: 60,
      height: 30,
      data: { kind: 'document' },
    },
  ];
  return { graph, nodes };
}

function crossZoneReservationRollbackFixture() {
  const outer = {
    id: 'folder:outer',
    name: 'outer',
    path: 'outer',
    parentId: null,
    childFolderIds: ['folder:inner'],
    documentIds: ['document:outside', 'document:blocker'],
  };
  const inner = {
    id: 'folder:inner',
    name: 'inner',
    path: 'outer/inner',
    parentId: 'folder:outer',
    childFolderIds: [],
    documentIds: [
      'document:doomed-source',
      'document:survivor-source',
      'document:survivor-target',
    ],
  };
  const documents = [
    routableDocument('document:outside', outer.id, 'Outside'),
    routableDocument('document:blocker', outer.id, 'Blocker'),
    routableDocument('document:doomed-source', inner.id, 'Doomed Source'),
    routableDocument('document:survivor-source', inner.id, 'Survivor Source'),
    routableDocument('document:survivor-target', inner.id, 'Survivor Target'),
  ];
  const graph = routableGraph([outer, inner], documents, [
    // Crosses out of `inner` into `outer` - reserving a segment inside
    // `inner` on the way - then fails once it reaches `outer`, since
    // `blocker` fully encloses `outside` with too little clearance to
    // route around, the same trapping pattern as `unroutableZoneFixture`.
    routableLink('link:doomed', 'document:doomed-source', 'document:outside'),
    // Stays entirely inside `inner`. Its cheapest route ties exactly, in
    // distance and bend count, with an equally valid alternate route, so
    // which one is chosen is decided purely by whichever segment
    // `link:doomed` reserved crossing the same zone and then - correctly -
    // released once it failed.
    routableLink(
      'link:survivor',
      'document:survivor-source',
      'document:survivor-target',
    ),
  ]);
  const nodes = [
    {
      id: outer.id,
      position: { x: 0, y: 0 },
      width: 700,
      height: 400,
      data: { kind: 'folder' },
    },
    {
      id: inner.id,
      parentId: outer.id,
      position: { x: 0, y: 0 },
      width: 300,
      height: 300,
      data: { kind: 'folder' },
    },
    {
      id: 'document:outside',
      parentId: outer.id,
      position: { x: 500, y: 150 },
      width: 80,
      height: 40,
      data: { kind: 'document' },
    },
    {
      id: 'document:blocker',
      parentId: outer.id,
      position: { x: 420, y: 70 },
      width: 240,
      height: 240,
      data: { kind: 'document' },
    },
    // Positioned identically to `survivor-source` so both links leave
    // `inner` from the exact same point, guaranteeing the zone-crossing
    // segment `link:doomed` reserves is one `link:survivor` would also use.
    {
      id: 'document:doomed-source',
      parentId: inner.id,
      position: { x: 30, y: 135 },
      width: 40,
      height: 30,
      data: { kind: 'document' },
    },
    {
      id: 'document:survivor-source',
      parentId: inner.id,
      position: { x: 30, y: 135 },
      width: 40,
      height: 30,
      data: { kind: 'document' },
    },
    // Sits at the same y the boundary gateway `link:doomed` exits through,
    // so `link:survivor`'s direct route ties exactly with the leg
    // `link:doomed` reserves crossing from `inner` into `outer`.
    {
      id: 'document:survivor-target',
      parentId: inner.id,
      position: { x: 300, y: 154 },
      width: 40,
      height: 30,
      data: { kind: 'document' },
    },
  ];
  return { graph, nodes };
}

function assertOrthogonalPoints(points) {
  assert.ok(points.length >= 2);
  for (let index = 1; index < points.length; index += 1) {
    const previous = points[index - 1];
    const current = points[index];
    assert.ok(previous.x === current.x || previous.y === current.y);
    assert.ok(Number.isFinite(current.x) && Number.isFinite(current.y));
  }
}

test('routes a link whose lead point lands on the floating-point boundary of its own inflated obstacle', () => {
  const { graph, nodes } = boundaryNoiseFixture();
  const route = routeMapLinks(graph, nodes)['link:source-target'];

  assert.ok(route);
  assertOrthogonalPoints(route.points);
});

test('keeps unrelated links routable and reports a diagnostic when a single zone is genuinely unroutable', () => {
  const { graph, nodes } = unroutableZoneFixture();
  const warnings = [];
  const originalWarn = console.warn;
  console.warn = (message) => warnings.push(message);

  let routes;
  try {
    routes = routeMapLinks(graph, nodes);
  } finally {
    console.warn = originalWarn;
  }

  assert.equal(Object.hasOwn(routes, 'link:trapped'), false);
  assert.ok(routes['link:good']);
  assertOrthogonalPoints(routes['link:good'].points);
  assert.ok(warnings.some((message) => message.includes('link:trapped')));
});

test('rolls back a doomed cross-folder link zone reservations instead of detouring a later link', () => {
  const { graph, nodes } = crossZoneReservationRollbackFixture();
  const warnings = [];
  const originalWarn = console.warn;
  console.warn = (message) => warnings.push(message);

  let routes;
  try {
    routes = routeMapLinks(graph, nodes);
  } finally {
    console.warn = originalWarn;
  }

  assert.equal(Object.hasOwn(routes, 'link:doomed'), false);
  assert.ok(warnings.some((message) => message.includes('link:doomed')));
  assert.ok(routes['link:survivor']);
  assertOrthogonalPoints(routes['link:survivor'].points);

  // Baseline: route `link:survivor` with no doomed link ever touching the
  // shared `inner` zone, so nothing could have left a stale reservation
  // behind. If reservations from `link:doomed`'s successfully-crossed
  // `inner` leg are correctly released once it fails in `outer`,
  // `link:survivor`'s route must be identical to this baseline.
  const baselineGraph = routableGraph(
    [
      { ...graph.folders['folder:outer'], documentIds: [] },
      graph.folders['folder:inner'],
    ],
    Object.values(graph.documents).filter(
      (document) =>
        document.id !== 'document:outside' && document.id !== 'document:blocker',
    ),
    [
      routableLink(
        'link:survivor',
        'document:survivor-source',
        'document:survivor-target',
      ),
    ],
  );
  const baselineNodes = nodes.filter(
    (node) => node.id !== 'document:outside' && node.id !== 'document:blocker',
  );
  const baselineRoute = routeMapLinks(baselineGraph, baselineNodes)['link:survivor'];

  assert.ok(baselineRoute);
  assert.deepEqual(
    routes['link:survivor'].points,
    baselineRoute.points,
    'reservations from a link that never renders must not detour a later link sharing the same zone',
  );
});

test('routes self-links and duplicate relationships as independent paths', async () => {
  const project = nestedProject();
  project.links.push(
    link(
      'link:app-api-duplicate',
      'document:frontend/app.md',
      'document:backend/api.md',
      true,
    ),
    link(
      'link:app-self',
      'document:frontend/app.md',
      'document:frontend/app.md',
      true,
    ),
  );
  const layout = await layoutMapGraph(projectToMapGraph(project), new ELK());
  const duplicateRoutes = layout.edges
    .filter((edge) => edge.target === 'document:backend/api.md')
    .map((edge) => JSON.stringify(edge.data.points));
  const selfLink = layout.edges.find((edge) => edge.id === 'link:app-self');

  assert.equal(new Set(duplicateRoutes).size, 2);
  assert.ok(selfLink.data.points.length >= 4);
  layout.edges.forEach(assertOrthogonal);
  assertAvoidsDocumentInteriors(layout);
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
      assert.ok(completedAt - started < 8_000);
      context.diagnostic(
        `${documentCount} documents, ${linkCount} links: adapter ${(adaptedAt - started).toFixed(1)} ms, ELK ${(completedAt - adaptedAt).toFixed(1)} ms`,
      );
    }
  },
);
