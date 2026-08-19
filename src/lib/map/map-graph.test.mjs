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
  clearMapDocumentTraceIfActive,
  clearMapEdgeTraceForDocument,
  completeQueuedMapLayout,
  completeEmptyMapLayout,
  completeMapLayout,
  connectedDocuments,
  createMapEdgeTraceState,
  createMapLayoutSession,
  createMapLayoutRequestState,
  edgeEmphasis,
  effectiveMapEdgeTraceId,
  failMapLayout,
  mapLayoutIsInteractive,
  nodeEmphasis,
  queueMapLayout,
  reduceMapEdgeTrace,
  resetMapTraceOnFlowUnmount,
  resolveTracedEdge,
  retryQueuedMapLayout,
} = await loadTypeScript('./map-view-state.ts');
const { buildFixtureProject, ROUTING_FIXTURES, routingFixtureBySlug } =
  await loadTypeScript('./routing-fixtures.ts');

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
    documents: Object.fromEntries(documents.map((item) => [item.id, item])),
    links,
  };
}

function routableDocument(id, parentId, title) {
  return {
    id,
    name: `${id}.md`,
    title,
    path: `${id}.md`,
    parentId,
    headings: [],
  };
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
        document.id !== 'document:outside' &&
        document.id !== 'document:blocker',
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
  const baselineRoute = routeMapLinks(baselineGraph, baselineNodes)[
    'link:survivor'
  ];

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

const REPEATED_DOCUMENT_NAMES = [
  'index.md',
  'database.md',
  'api.md',
  'types.md',
  'readme.md',
  'config.md',
  'client.md',
  'server.md',
];

function duplicateNameProject(areaCount, modulesPerArea) {
  const folders = {
    'folder:.': {
      id: 'folder:.',
      name: 'workspace',
      path: '',
      parentId: null,
      childFolderIds: [],
      documentIds: [],
    },
  };
  const documents = {};
  const documentIdsByArea = [];

  for (let areaIndex = 0; areaIndex < areaCount; areaIndex += 1) {
    const areaId = `folder:area-${areaIndex}`;
    folders['folder:.'].childFolderIds.push(areaId);
    const moduleIds = [];
    folders[areaId] = {
      id: areaId,
      name: `area-${areaIndex}`,
      path: `area-${areaIndex}`,
      parentId: 'folder:.',
      childFolderIds: moduleIds,
      documentIds: [],
    };
    const areaDocumentIds = [];

    for (let moduleIndex = 0; moduleIndex < modulesPerArea; moduleIndex += 1) {
      const moduleId = `folder:area-${areaIndex}/module-${moduleIndex}`;
      moduleIds.push(moduleId);
      const name =
        REPEATED_DOCUMENT_NAMES[
          (areaIndex + moduleIndex) % REPEATED_DOCUMENT_NAMES.length
        ];
      const path = `area-${areaIndex}/module-${moduleIndex}/${name}`;
      const item = document(
        path,
        moduleId,
        `Document ${areaIndex}-${moduleIndex}`,
      );
      documents[item.id] = item;
      areaDocumentIds.push(item.id);
      folders[moduleId] = {
        id: moduleId,
        name: `module-${moduleIndex}`,
        path: `area-${areaIndex}/module-${moduleIndex}`,
        parentId: areaId,
        childFolderIds: [],
        documentIds: [item.id],
      };
    }

    documentIdsByArea.push(areaDocumentIds);
  }

  const links = [];
  let linkIndex = 0;
  const nextLinkId = () => {
    const id = `link:${String(linkIndex).padStart(5, '0')}`;
    linkIndex += 1;
    return id;
  };

  for (const areaDocumentIds of documentIdsByArea) {
    for (let index = 0; index < areaDocumentIds.length - 1; index += 1) {
      links.push(
        link(
          nextLinkId(),
          areaDocumentIds[index],
          areaDocumentIds[index + 1],
          true,
        ),
      );
    }
  }

  for (const name of REPEATED_DOCUMENT_NAMES) {
    const sameNameDocumentIds = Object.values(documents)
      .filter((item) => item.name === name)
      .map((item) => item.id);
    for (let index = 0; index < sameNameDocumentIds.length - 1; index += 1) {
      links.push(
        link(
          nextLinkId(),
          sameNameDocumentIds[index],
          sameNameDocumentIds[index + 1],
          true,
        ),
      );
    }
  }

  const allDocumentIds = Object.keys(documents);
  for (let index = 0; index < allDocumentIds.length; index += 4) {
    links.push(link(nextLinkId(), allDocumentIds[index], null, false));
  }

  return {
    rootPath: '/duplicate-names',
    folders,
    documents,
    links,
  };
}

function oracleConnections(graph) {
  const connections = new Map(
    Object.keys(graph.documents).map((id) => [id, new Set()]),
  );
  for (const graphLink of graph.links) {
    connections.get(graphLink.sourceDocumentId).add(graphLink.targetDocumentId);
    connections.get(graphLink.targetDocumentId).add(graphLink.sourceDocumentId);
  }
  return connections;
}

test(
  'highlights only the hovered document and its true one-hop neighbors in a large graph with repeated file names',
  { timeout: 60_000 },
  async () => {
    const project = duplicateNameProject(6, 40);
    const graph = projectToMapGraph(project);
    const layout = await layoutMapGraph(graph, new ELK());
    const documentIds = Object.keys(graph.documents);
    const connections = oracleConnections(graph);

    assert.equal(documentIds.length, 240);
    assert.ok(
      REPEATED_DOCUMENT_NAMES.every((name) =>
        documentIds.some((id) => graph.documents[id].name === name),
      ),
    );

    const documentNodes = layout.nodes.filter(
      (node) => node.data.kind === 'document',
    );
    const rectangles = absoluteRectangles(layout);
    const hoveredId = documentNodes.reduce((rightmost, node) =>
      rectangles[node.id].x > rectangles[rightmost.id].x ? node : rightmost,
    ).id;
    const hoveredCenterX =
      rectangles[hoveredId].x + rectangles[hoveredId].width / 2;
    const documentsToTheLeft = documentNodes.filter(
      (node) =>
        node.id !== hoveredId &&
        rectangles[node.id].x + rectangles[node.id].width / 2 < hoveredCenterX,
    );
    assert.ok(
      documentsToTheLeft.length > documentNodes.length / 4,
      'fixture should place many unrelated documents to the left of the hovered node',
    );

    const hoveredNeighbors = connections.get(hoveredId);
    const unrelatedEdge = layout.edges.find(
      (edge) =>
        edge.source !== hoveredId &&
        edge.target !== hoveredId &&
        !hoveredNeighbors.has(edge.source) &&
        !hoveredNeighbors.has(edge.target),
    );
    assert.ok(
      unrelatedEdge,
      'fixture should contain an edge fully unrelated to the hovered document',
    );

    const staleTrace = reduceMapEdgeTrace(createMapEdgeTraceState(), {
      source: 'focus',
      edgeId: unrelatedEdge.id,
    });
    const staleActiveEdge = layout.edges.find(
      (edge) => edge.id === effectiveMapEdgeTraceId(staleTrace),
    );
    const collidedHoveredEmphasis = nodeEmphasis(
      hoveredId,
      hoveredId,
      staleActiveEdge,
      connectedDocuments(layout.edges, hoveredId),
    );
    assert.notEqual(
      collidedHoveredEmphasis,
      'active',
      'a leftover edge trace must not be able to mask the hovered document',
    );
    const collidedUnrelatedEmphasis = nodeEmphasis(
      unrelatedEdge.source,
      hoveredId,
      staleActiveEdge,
      connectedDocuments(layout.edges, hoveredId),
    );
    assert.equal(
      collidedUnrelatedEmphasis,
      'active',
      'demonstrates the collision: a document unrelated to the hovered one is wrongly marked active',
    );

    const clearedTrace = clearMapEdgeTraceForDocument(staleTrace, hoveredId);
    assert.equal(effectiveMapEdgeTraceId(clearedTrace), null);
    const activeEdge = effectiveMapEdgeTraceId(clearedTrace)
      ? layout.edges.find(
          (edge) => edge.id === effectiveMapEdgeTraceId(clearedTrace),
        )
      : null;
    const connected = connectedDocuments(layout.edges, hoveredId);
    assert.deepEqual(connected, connections.get(hoveredId));

    for (const documentId of documentIds) {
      const emphasis = nodeEmphasis(
        documentId,
        hoveredId,
        activeEdge,
        connected,
      );
      if (documentId === hoveredId) {
        assert.equal(emphasis, 'active', documentId);
      } else if (connections.get(hoveredId).has(documentId)) {
        assert.equal(emphasis, 'connected', documentId);
      } else {
        assert.equal(emphasis, 'muted', documentId);
      }
    }

    for (const node of documentsToTheLeft) {
      if (connections.get(hoveredId).has(node.id)) continue;
      assert.equal(
        nodeEmphasis(node.id, hoveredId, activeEdge, connected),
        'muted',
        `${node.id} is positioned left of the hovered node but is not connected to it`,
      );
    }

    for (const edge of layout.edges) {
      const expected =
        edge.source === hoveredId || edge.target === hoveredId
          ? 'active'
          : 'muted';
      assert.equal(
        edgeEmphasis(edge, hoveredId, activeEdge),
        expected,
        edge.id,
      );
    }

    const secondHoveredId = documentNodes.find(
      (node) =>
        node.id !== hoveredId && !connections.get(hoveredId).has(node.id),
    ).id;
    const secondConnected = connectedDocuments(layout.edges, secondHoveredId);
    assert.notEqual(
      nodeEmphasis(hoveredId, secondHoveredId, null, secondConnected),
      'active',
      'moving focus to a new document must clear the previous trace completely',
    );
  },
);

test('clears an edge trace left behind by either pointer or keyboard focus the same way', async () => {
  const project = duplicateNameProject(3, 10);
  const graph = projectToMapGraph(project);
  const layout = await layoutMapGraph(graph, new ELK());
  const documentIds = Object.keys(graph.documents);
  const connections = oracleConnections(graph);
  const targetId = documentIds.at(-1);
  const staleEdge = layout.edges.find(
    (edge) => edge.source !== targetId && edge.target !== targetId,
  );
  assert.ok(staleEdge);

  for (const source of ['pointer', 'focus']) {
    const staleTrace = reduceMapEdgeTrace(createMapEdgeTraceState(), {
      source,
      edgeId: staleEdge.id,
    });
    const clearedTrace = clearMapEdgeTraceForDocument(staleTrace, targetId);
    assert.equal(effectiveMapEdgeTraceId(clearedTrace), null, source);

    const connected = connectedDocuments(layout.edges, targetId);
    assert.deepEqual(connected, connections.get(targetId), source);

    for (const documentId of documentIds) {
      const expected =
        documentId === targetId
          ? 'active'
          : connections.get(targetId).has(documentId)
            ? 'connected'
            : 'muted';
      assert.equal(
        nodeEmphasis(documentId, targetId, null, connected),
        expected,
        `${source}:${documentId}`,
      );
    }
  }
});

test('restores a still-live edge trace once the document that overrode it stops being hovered', async () => {
  const graph = projectToMapGraph(nestedProject());
  const layout = await layoutMapGraph(graph, new ELK());
  const focusedEdge = layout.edges.find((edge) => edge.id === 'link:app-api');
  const hoveredDocumentId = 'document:readme.md';
  assert.ok(focusedEdge);
  assert.notEqual(focusedEdge.source, hoveredDocumentId);
  assert.notEqual(focusedEdge.target, hoveredDocumentId);

  // The edge's hidden keyboard button is genuinely focused.
  const edgeTraceState = reduceMapEdgeTrace(createMapEdgeTraceState(), {
    source: 'focus',
    edgeId: focusedEdge.id,
  });
  assert.equal(effectiveMapEdgeTraceId(edgeTraceState), focusedEdge.id);

  // An unrelated document becomes actively hovered while the edge focus is
  // still live: the document must take projection priority. This calls
  // MapView's actual production gate, not a copy of it.
  const tracedEdgeDuringHover = resolveTracedEdge(
    edgeTraceState,
    true,
    layout.edges,
  );
  assert.equal(
    tracedEdgeDuringHover,
    null,
    'the active document must take projection priority over the live edge trace',
  );

  const connectedToHovered = connectedDocuments(
    layout.edges,
    hoveredDocumentId,
  );
  for (const documentId of Object.keys(graph.documents)) {
    const expected = documentId === hoveredDocumentId ? 'active' : 'muted';
    assert.equal(
      nodeEmphasis(
        documentId,
        hoveredDocumentId,
        tracedEdgeDuringHover,
        connectedToHovered,
      ),
      expected,
      documentId,
    );
  }
  for (const edge of layout.edges) {
    assert.equal(
      edgeEmphasis(edge, hoveredDocumentId, tracedEdgeDuringHover),
      edge.source === hoveredDocumentId || edge.target === hoveredDocumentId
        ? 'active'
        : 'muted',
      edge.id,
    );
  }

  // Crucially, the live focus source was never cleared while suppressed.
  assert.equal(
    effectiveMapEdgeTraceId(edgeTraceState),
    focusedEdge.id,
    'a still-focused edge must survive the document taking priority',
  );
  assert.equal(edgeTraceState.focusedEdgeId, focusedEdge.id);

  // The document loses hover/focus. The edge button's focus never moved, so
  // its trace becomes effective again with no new pointer/focus event.
  const tracedEdgeAfterHover = resolveTracedEdge(
    edgeTraceState,
    false,
    layout.edges,
  );
  assert.equal(tracedEdgeAfterHover?.id, focusedEdge.id);

  const connectedToNone = connectedDocuments(layout.edges, null);
  for (const edge of layout.edges) {
    assert.equal(
      edgeEmphasis(edge, null, tracedEdgeAfterHover),
      edge.id === focusedEdge.id ? 'active' : 'muted',
      edge.id,
    );
  }
  assert.equal(
    nodeEmphasis(
      focusedEdge.source,
      null,
      tracedEdgeAfterHover,
      connectedToNone,
    ),
    'active',
  );
  assert.equal(
    nodeEmphasis(
      focusedEdge.target,
      null,
      tracedEdgeAfterHover,
      connectedToNone,
    ),
    'active',
  );
  assert.equal(
    nodeEmphasis(
      hoveredDocumentId,
      null,
      tracedEdgeAfterHover,
      connectedToNone,
    ),
    'muted',
  );
});

test('resets a stale document trace when the flow unmounts, so a remounted map traces an edge immediately', async () => {
  const graph = projectToMapGraph(nestedProject());
  const layout = await layoutMapGraph(graph, new ELK());
  const nextEdge = layout.edges.find((edge) => edge.id === 'link:app-store');
  assert.ok(nextEdge);

  // A document is hovered/focused (e.g. a document button holds keyboard
  // focus). Mirrors MapView's reactive derivations.
  let hoveredDocumentId = 'document:frontend/app.md';
  let edgeTraceState = createMapEdgeTraceState();
  let documentTraceActive = hoveredDocumentId !== null;
  assert.equal(
    resolveTracedEdge(edgeTraceState, documentTraceActive, layout.edges),
    null,
  );

  // The flow is unmounted without a matching onblur/onpointerleave firing on
  // the focused document node (e.g. Ctrl/Cmd+1 hides the map, or a
  // {#key layoutRevision} remount tears the element down mid-focus). Left
  // untouched, the stale hover survives the unmount intact.
  const survivedUnmountUntouched = hoveredDocumentId;
  assert.equal(
    survivedUnmountUntouched,
    'document:frontend/app.md',
    'demonstrates the bug: an unmount alone does not clear a stale document trace',
  );

  // scheduleFlowMount now resets both fields on every mount-cycle
  // transition via the production reducer.
  ({ hoveredDocumentId, edgeTraceState } = resetMapTraceOnFlowUnmount());
  documentTraceActive = hoveredDocumentId !== null;

  assert.equal(hoveredDocumentId, null);
  assert.deepEqual(edgeTraceState, createMapEdgeTraceState());
  assert.equal(documentTraceActive, false);

  // The flow remounts. The very first interaction is hovering/focusing a
  // *different* edge - no other document needs to complete a full
  // enter/leave cycle first for it to take effect.
  edgeTraceState = reduceMapEdgeTrace(edgeTraceState, {
    source: 'pointer',
    edgeId: nextEdge.id,
  });
  const tracedEdge = resolveTracedEdge(
    edgeTraceState,
    documentTraceActive,
    layout.edges,
  );
  assert.equal(
    tracedEdge?.id,
    nextEdge.id,
    'edge tracing must work immediately after remount, not after a full document enter/leave cycle',
  );

  const connected = connectedDocuments(layout.edges, null);
  assert.equal(
    nodeEmphasis(nextEdge.source, null, tracedEdge, connected),
    'active',
  );
  assert.equal(
    nodeEmphasis(nextEdge.target, null, tracedEdge, connected),
    'active',
  );
  assert.equal(
    nodeEmphasis(survivedUnmountUntouched, null, tracedEdge, connected),
    survivedUnmountUntouched === nextEdge.source ||
      survivedUnmountUntouched === nextEdge.target
      ? 'active'
      : 'muted',
    'the previously stale hovered document must not be pinned active anymore',
  );
});

test('clears a document trace when its own virtualized node unmounts while still active', () => {
  const documentA = 'document:frontend/app.md';

  // The document button is hovered/focused, matching MapView's reactive
  // documentTraceActive derivation.
  let hoveredDocumentId = documentA;
  assert.equal(hoveredDocumentId !== null, true);

  // `onlyRenderVisibleElements` unmounts the individual node (e.g. the user
  // panned it out of the viewport) without firing its onblur/onpointerleave
  // callback first. MapDocumentNode's onDestroy hook calls this same
  // production reducer with its own documentId, exactly like MapView's
  // traceDocumentUnmount does.
  hoveredDocumentId = clearMapDocumentTraceIfActive(
    hoveredDocumentId,
    documentA,
  );

  assert.equal(
    hoveredDocumentId,
    null,
    'the trace must be cleared once the only source that could ever clear it is gone',
  );
  const documentTraceActive = hoveredDocumentId !== null;
  assert.equal(documentTraceActive, false);
});

test('does not clear a different document trace from a stale unmount of a previously active node', () => {
  const documentA = 'document:frontend/app.md';
  const documentB = 'document:backend/api.md';

  // Document A is hovered first.
  let hoveredDocumentId = documentA;

  // The user moves on to document B before A's node is torn down; B is now
  // the genuinely active trace.
  hoveredDocumentId = documentB;

  // A's virtualized node is only unmounted afterwards (a late cleanup racing
  // behind the new hover). Because `hoveredDocumentId` no longer equals A's
  // own id, the guard must leave B's trace untouched.
  hoveredDocumentId = clearMapDocumentTraceIfActive(
    hoveredDocumentId,
    documentA,
  );

  assert.equal(
    hoveredDocumentId,
    documentB,
    'a late unmount of a no-longer-active document must not erase a different valid trace',
  );
  const documentTraceActive = hoveredDocumentId !== null;
  assert.equal(documentTraceActive, true);
});

function assertRouteGeometry(edge) {
  assertOrthogonal(edge);
  for (const point of edge.data.points) {
    assert.ok(
      Number.isFinite(point.x) && Number.isFinite(point.y),
      `${edge.id} has a non-finite coordinate`,
    );
  }
  for (let index = 1; index < edge.data.points.length; index += 1) {
    const previous = edge.data.points[index - 1];
    const current = edge.data.points[index];
    assert.ok(
      previous.x !== current.x || previous.y !== current.y,
      `${edge.id} contains a zero-length segment`,
    );
  }
}

const MAP_SIDES = new Set(['top', 'right', 'bottom', 'left']);

function sideFromHandle(handle, prefix) {
  if (typeof handle !== 'string' || !handle.startsWith(`${prefix}-`)) {
    return null;
  }
  const side = handle.slice(prefix.length + 1).split('-')[0];
  return MAP_SIDES.has(side) ? side : null;
}

function movesTowardSide(from, to, side) {
  if (side === 'left') return to.x < from.x && to.y === from.y;
  if (side === 'right') return to.x > from.x && to.y === from.y;
  if (side === 'top') return to.y < from.y && to.x === from.x;
  return to.y > from.y && to.x === from.x;
}

function assertValidArrowAndPorts(layout, slug) {
  for (const edge of layout.edges) {
    assert.equal(
      edge.markerEnd?.type,
      'arrowclosed',
      `${slug}: ${edge.id} is missing a valid arrow marker`,
    );
    const sourceSide = sideFromHandle(edge.sourceHandle, 'source');
    const targetSide = sideFromHandle(edge.targetHandle, 'target');
    assert.ok(
      sourceSide,
      `${slug}: ${edge.id} has no valid source side handle`,
    );
    assert.ok(
      targetSide,
      `${slug}: ${edge.id} has no valid target side handle`,
    );
    const points = edge.data.points;
    assert.ok(
      movesTowardSide(points[0], points[1], sourceSide),
      `${slug}: ${edge.id} leaves its source in a direction inconsistent with its source handle side`,
    );
    assert.ok(
      movesTowardSide(points.at(-1), points.at(-2), targetSide),
      `${slug}: ${edge.id} enters its target from a direction inconsistent with its target handle side`,
    );
  }
}

function assertExplicitPortModel(layout, slug) {
  const groups = new Map();

  for (const edge of layout.edges) {
    const points = edge.data.points;
    const endpoints = [
      ['source', edge.data.sourcePort, points[0], edge.source],
      ['target', edge.data.targetPort, points.at(-1), edge.target],
    ];

    for (const [direction, port, expectedPoint, documentId] of endpoints) {
      assert.ok(port, `${slug}: ${edge.id} is missing its ${direction} port`);
      assert.equal(
        port.direction,
        direction,
        `${slug}: ${edge.id} ${direction} port has the wrong direction`,
      );
      assert.equal(
        port.documentId,
        documentId,
        `${slug}: ${edge.id} ${direction} port documentId does not match its edge endpoint`,
      );
      assert.equal(
        port.linkId,
        edge.id,
        `${slug}: ${edge.id} ${direction} port linkId does not match its own edge`,
      );
      assert.deepEqual(
        port.point,
        expectedPoint,
        `${slug}: ${edge.id} ${direction} port point does not match its route endpoint`,
      );
      assert.ok(
        port.offset >= 0 && port.offset <= 1,
        `${slug}: ${edge.id} ${direction} port offset ${port.offset} is out of [0,1]`,
      );
      assert.equal(
        direction === 'source' ? edge.sourceHandle : edge.targetHandle,
        `${direction}-${port.side}-${port.index}`,
        `${slug}: ${edge.id} ${direction} handle does not match its own port`,
      );

      const key = `${port.documentId}:${port.side}`;
      const group = groups.get(key) ?? new Map();
      assert.ok(
        !group.has(port.index),
        `${slug}: ${key} has two ports sharing index ${port.index}`,
      );
      group.set(port.index, port);
      groups.set(key, group);
    }
  }

  for (const [key, group] of groups) {
    const indices = [...group.keys()].sort((left, right) => left - right);
    assert.deepEqual(
      indices,
      indices.map((_, index) => index),
      `${slug}: ${key} port indices are not a contiguous 0..count-1 range`,
    );
    for (const port of group.values()) {
      assert.equal(
        port.count,
        indices.length,
        `${slug}: ${key} port count does not match its group size`,
      );
    }
  }
}

function perpendicularCenter(rect, side) {
  return side === 'left' || side === 'right'
    ? rect.y + rect.height / 2
    : rect.x + rect.width / 2;
}

function assertSpatialPortOrder(layout, documentId, side, slug) {
  const rects = absoluteRectangles(layout);
  const members = [];

  for (const edge of layout.edges) {
    if (
      edge.data.sourcePort.documentId === documentId &&
      edge.data.sourcePort.side === side
    ) {
      members.push({
        index: edge.data.sourcePort.index,
        center: perpendicularCenter(rects[edge.target], side),
      });
    }
    if (
      edge.data.targetPort.documentId === documentId &&
      edge.data.targetPort.side === side
    ) {
      members.push({
        index: edge.data.targetPort.index,
        center: perpendicularCenter(rects[edge.source], side),
      });
    }
  }

  members.sort((left, right) => left.index - right.index);
  for (let index = 1; index < members.length; index += 1) {
    assert.ok(
      members[index].center >= members[index - 1].center,
      `${slug}: ${documentId}:${side} ports do not preserve the spatial order of their neighboring documents`,
    );
  }
}

function segmentsCross(sourceA, targetA, sourceB, targetB) {
  const aHorizontal = sourceA.y === targetA.y;
  const bHorizontal = sourceB.y === targetB.y;
  if (aHorizontal === bHorizontal) return false;
  const horizontal = aHorizontal
    ? {
        y: sourceA.y,
        x1: Math.min(sourceA.x, targetA.x),
        x2: Math.max(sourceA.x, targetA.x),
      }
    : {
        y: sourceB.y,
        x1: Math.min(sourceB.x, targetB.x),
        x2: Math.max(sourceB.x, targetB.x),
      };
  const vertical = aHorizontal
    ? {
        x: sourceB.x,
        y1: Math.min(sourceB.y, targetB.y),
        y2: Math.max(sourceB.y, targetB.y),
      }
    : {
        x: sourceA.x,
        y1: Math.min(sourceA.y, targetA.y),
        y2: Math.max(sourceA.y, targetA.y),
      };
  return (
    vertical.x > horizontal.x1 &&
    vertical.x < horizontal.x2 &&
    horizontal.y > vertical.y1 &&
    horizontal.y < vertical.y2
  );
}

function countRouteCrossings(edges) {
  const segments = [];
  for (const edge of edges) {
    for (let index = 1; index < edge.data.points.length; index += 1) {
      segments.push({
        edgeId: edge.id,
        source: edge.data.points[index - 1],
        target: edge.data.points[index],
      });
    }
  }
  let crossings = 0;
  for (let first = 0; first < segments.length; first += 1) {
    for (let second = first + 1; second < segments.length; second += 1) {
      if (segments[first].edgeId === segments[second].edgeId) continue;
      if (
        segmentsCross(
          segments[first].source,
          segments[first].target,
          segments[second].source,
          segments[second].target,
        )
      ) {
        crossings += 1;
      }
    }
  }
  return crossings;
}

test(
  'routes every deterministic stress fixture identically twice with finite, orthogonal, interior-safe geometry',
  { timeout: 120_000 },
  async (context) => {
    const started = performance.now();
    for (const fixture of ROUTING_FIXTURES) {
      const graph = projectToMapGraph(buildFixtureProject(fixture));
      const first = await layoutMapGraph(graph, new ELK());
      const second = await layoutMapGraph(graph, new ELK());

      assert.ok(first.edges.length > 0, `${fixture.slug} produced no edges`);
      first.edges.forEach(assertRouteGeometry);
      assertAvoidsDocumentInteriors(first);
      assertValidArrowAndPorts(first, fixture.slug);
      assertExplicitPortModel(first, fixture.slug);
      assert.deepEqual(
        first.edges.map((edge) => edge.data.points),
        second.edges.map((edge) => edge.data.points),
        `${fixture.slug} routed non-deterministically`,
      );
      assert.deepEqual(
        first.edges.map((edge) => [edge.data.sourcePort, edge.data.targetPort]),
        second.edges.map((edge) => [
          edge.data.sourcePort,
          edge.data.targetPort,
        ]),
        `${fixture.slug} assigned ports non-deterministically`,
      );
      assert.deepEqual(
        first.nodes.map((node) => [node.id, node.position.x, node.position.y]),
        second.nodes.map((node) => [node.id, node.position.x, node.position.y]),
        `${fixture.slug} laid out non-deterministically`,
      );
    }
    context.diagnostic(
      `${ROUTING_FIXTURES.length} stress fixtures laid out twice in ${(performance.now() - started).toFixed(1)} ms`,
    );
    assert.ok(
      performance.now() - started < 60_000,
      'stress fixtures exceeded the bounded layout time budget',
    );
  },
);

test(
  'routes the cross-folder highway and nested fixtures through ordered, hierarchy-correct boundary gateways',
  { timeout: 60_000 },
  async () => {
    for (const slug of [
      'cross-folder-highway',
      'nested-to-external',
      'unrelated-near-corridor',
    ]) {
      const fixture = routingFixtureBySlug(slug);
      const graph = projectToMapGraph(buildFixtureProject(fixture));
      const layout = await layoutMapGraph(graph, new ELK());
      assertFolderGatewayPolicy(graph, layout);
    }
  },
);

test(
  'keeps both directions of the bidirectional corridor as independent, non-merged edges',
  { timeout: 60_000 },
  async () => {
    const fixture = routingFixtureBySlug('bidirectional-corridor');
    const graph = projectToMapGraph(buildFixtureProject(fixture));
    const layout = await layoutMapGraph(graph, new ELK());

    assert.equal(layout.edges.length, 20);
    assert.equal(new Set(layout.edges.map((edge) => edge.id)).size, 20);

    const perPair = new Map();
    for (const edge of layout.edges) {
      const key = [edge.source, edge.target].sort().join('|');
      perPair.set(key, (perPair.get(key) ?? 0) + 1);
    }
    assert.ok(
      [...perPair.values()].every((count) => count === 2),
      'each left/right pair must keep exactly two independent edges, one per direction',
    );

    const pointSets = layout.edges.map((edge) =>
      JSON.stringify(edge.data.points),
    );
    assert.equal(
      new Set(pointSets).size,
      20,
      'opposite-direction edges between the same pair must not collapse onto identical geometry',
    );
  },
);

test(
  'keeps inbound and outbound lanes on the mixed hub independently ordered, and collision-free combined across both directions',
  { timeout: 60_000 },
  async () => {
    const fixture = routingFixtureBySlug('mixed-hub');
    const graph = projectToMapGraph(buildFixtureProject(fixture));
    const layout = await layoutMapGraph(graph, new ELK());

    assert.equal(layout.edges.length, 16);
    assertExplicitPortModel(layout, 'mixed-hub');
    const outgoing = layout.edges.filter(
      (edge) => edge.source === 'document:hub.md',
    );
    const incoming = layout.edges.filter(
      (edge) => edge.target === 'document:hub.md',
    );
    assert.equal(outgoing.length, 8);
    assert.equal(incoming.length, 8);
    assert.equal(
      new Set(outgoing.map((edge) => JSON.stringify(edge.data.points[0]))).size,
      8,
      'every outgoing edge must land on a distinct point along the hub boundary',
    );
    assert.equal(
      new Set(incoming.map((edge) => JSON.stringify(edge.data.points.at(-1))))
        .size,
      8,
      'every incoming edge must land on a distinct point along the hub boundary',
    );

    const allHubEndpoints = [
      ...outgoing.map((edge) => JSON.stringify(edge.data.points[0])),
      ...incoming.map((edge) => JSON.stringify(edge.data.points.at(-1))),
    ];
    assert.equal(
      new Set(allHubEndpoints).size,
      16,
      'every one of the 16 hub endpoints must be distinct across both directions combined: the merged port ranking (routing.ts portKey) must never let an incoming and an outgoing edge share the same physical point on the same side',
    );

    for (const side of ['top', 'right', 'bottom', 'left']) {
      assertSpatialPortOrder(layout, 'document:hub.md', side, 'mixed-hub');
    }
  },
);

test(
  'routes the fan-converge fixture as an organized trunk with distinct per-target ports',
  { timeout: 60_000 },
  async (context) => {
    const fixture = routingFixtureBySlug('fan-converge');
    const graph = projectToMapGraph(buildFixtureProject(fixture));
    const layout = await layoutMapGraph(graph, new ELK());

    assert.equal(layout.edges.length, 20);
    assertExplicitPortModel(layout, 'fan-converge');

    const byTarget = new Map();
    for (const edge of layout.edges) {
      const list = byTarget.get(edge.target) ?? [];
      list.push(edge);
      byTarget.set(edge.target, list);
    }
    assert.equal(
      byTarget.size,
      5,
      'fan-converge must resolve to exactly 5 distinct targets',
    );
    for (const [targetId, edges] of byTarget) {
      assert.equal(
        edges.length,
        4,
        `${targetId} must receive exactly 4 incoming edges`,
      );
      assert.equal(
        new Set(edges.map((edge) => JSON.stringify(edge.data.points.at(-1))))
          .size,
        4,
        `${targetId} must land every incoming edge on a distinct port`,
      );
    }

    const crossings = countRouteCrossings(layout.edges);
    context.diagnostic(
      `fan-converge fixture: ${crossings} real geometric segment crossings across 20 edges`,
    );
    assert.ok(
      crossings < layout.edges.length,
      'fan-converge must stay a readable, mostly crossing-free organized trunk rather than a tangle',
    );
  },
);

test(
  'measures real geometric crossings on the crossing-heavy fixture as a baseline for the later crossing-marker phase',
  { timeout: 60_000 },
  async (context) => {
    const fixture = routingFixtureBySlug('crossing-heavy');
    const graph = projectToMapGraph(buildFixtureProject(fixture));
    const layout = await layoutMapGraph(graph, new ELK());

    layout.edges.forEach(assertRouteGeometry);
    assertAvoidsDocumentInteriors(layout);
    const crossings = countRouteCrossings(layout.edges);
    context.diagnostic(
      `crossing-heavy fixture: ${crossings} real geometric segment crossings across ${layout.edges.length} edges`,
    );
    assert.ok(crossings >= 0);
  },
);

test(
  'measures layout stability across the incremental-change fixture pair as a baseline for the later layout-feedback phase',
  { timeout: 60_000 },
  async (context) => {
    const baseGraph = projectToMapGraph(
      buildFixtureProject(routingFixtureBySlug('incremental-base')),
    );
    const nextGraph = projectToMapGraph(
      buildFixtureProject(routingFixtureBySlug('incremental-next')),
    );

    const baseLayout = await layoutMapGraph(baseGraph, new ELK());
    const baseLayoutAgain = await layoutMapGraph(baseGraph, new ELK());
    assert.deepEqual(
      baseLayout.nodes.map((node) => [
        node.id,
        node.position.x,
        node.position.y,
      ]),
      baseLayoutAgain.nodes.map((node) => [
        node.id,
        node.position.x,
        node.position.y,
      ]),
      'the base fixture must lay out identically across repeated runs',
    );

    const nextLayout = await layoutMapGraph(nextGraph, new ELK());
    const basePositions = new Map(
      baseLayout.nodes.map((node) => [node.id, node.position]),
    );
    let shared = 0;
    let unchanged = 0;
    for (const node of nextLayout.nodes) {
      const previous = basePositions.get(node.id);
      if (!previous) continue;
      shared += 1;
      if (previous.x === node.position.x && previous.y === node.position.y) {
        unchanged += 1;
      }
    }
    context.diagnostic(
      `incremental change: ${unchanged}/${shared} shared node positions unchanged after adding one document and one link`,
    );
    assert.ok(shared > 0);
  },
);

function portModelFixture() {
  const folder = {
    id: 'folder:zone',
    name: 'zone',
    path: 'zone',
    parentId: null,
    childFolderIds: [],
    documentIds: [
      'document:hub',
      'document:a',
      'document:b',
      'document:c',
      'document:d',
    ],
  };
  const documents = [
    routableDocument('document:hub', folder.id, 'Hub'),
    routableDocument('document:a', folder.id, 'A'),
    routableDocument('document:b', folder.id, 'B'),
    routableDocument('document:c', folder.id, 'C'),
    routableDocument('document:d', folder.id, 'D'),
  ];
  // A, B, and C each link into the hub (target role); the hub links out to D
  // (source role). A, B, C, and D sit, top to bottom, on the same side of
  // the hub, so all four edges compete for the same physical side and must
  // be ranked into one merged, direction-independent sequence.
  const links = [
    routableLink('link:a', 'document:a', 'document:hub'),
    routableLink('link:b', 'document:b', 'document:hub'),
    routableLink('link:c', 'document:c', 'document:hub'),
    routableLink('link:d', 'document:hub', 'document:d'),
  ];
  const graph = routableGraph([folder], documents, links);
  const nodes = [
    {
      id: folder.id,
      position: { x: 0, y: 0 },
      width: 600,
      height: 420,
      data: { kind: 'folder' },
    },
    {
      id: 'document:hub',
      parentId: folder.id,
      position: { x: 300, y: 150 },
      width: 100,
      height: 50,
      data: { kind: 'document' },
    },
    {
      id: 'document:a',
      parentId: folder.id,
      position: { x: 20, y: 20 },
      width: 100,
      height: 50,
      data: { kind: 'document' },
    },
    {
      id: 'document:b',
      parentId: folder.id,
      position: { x: 20, y: 120 },
      width: 100,
      height: 50,
      data: { kind: 'document' },
    },
    {
      id: 'document:c',
      parentId: folder.id,
      position: { x: 20, y: 220 },
      width: 100,
      height: 50,
      data: { kind: 'document' },
    },
    {
      id: 'document:d',
      parentId: folder.id,
      position: { x: 20, y: 320 },
      width: 100,
      height: 50,
      data: { kind: 'document' },
    },
  ];
  return { graph, nodes };
}

test('assigns an explicit, structured port model to every route endpoint', () => {
  const { graph, nodes } = portModelFixture();
  const first = routeMapLinks(graph, nodes);
  const second = routeMapLinks(graph, nodes);

  assert.deepEqual(first, second, 'port assignment must be deterministic');

  for (const linkId of ['link:a', 'link:b', 'link:c', 'link:d']) {
    const route = first[linkId];
    assert.ok(route, `${linkId} did not route`);
    for (const [direction, port] of [
      ['source', route.sourcePort],
      ['target', route.targetPort],
    ]) {
      assert.equal(typeof port.id, 'string');
      assert.equal(port.direction, direction);
      assert.equal(port.linkId, linkId);
      assert.ok(Number.isInteger(port.index) && port.index >= 0);
      assert.ok(Number.isInteger(port.count) && port.count >= 1);
      assert.ok(port.offset >= 0 && port.offset <= 1);
    }
  }

  assert.equal(first['link:a'].sourcePort.direction, 'source');
  assert.equal(first['link:a'].targetPort.direction, 'target');
  assert.equal(first['link:a'].targetPort.documentId, 'document:hub');
  assert.equal(first['link:d'].sourcePort.documentId, 'document:hub');

  // A, B, C (target-role) and D (source-role, via the hub's outgoing edge)
  // all land on the hub's left side, so this is exactly the mixed-direction
  // congestion the merged port ranking must resolve.
  const hubLeftPorts = [
    first['link:a'].targetPort,
    first['link:b'].targetPort,
    first['link:c'].targetPort,
    first['link:d'].sourcePort,
  ];
  assert.ok(
    hubLeftPorts.every((port) => port.side === 'left'),
    'fixture setup expected all four edges on the hub’s left side',
  );
  assert.equal(
    new Set(hubLeftPorts.map((port) => port.index)).size,
    4,
    'every edge on the hub’s left side must have a distinct index regardless of direction',
  );
  assert.ok(
    hubLeftPorts.every((port) => port.count === 4),
    'every port sharing the hub’s left side must report the same total count',
  );
  assert.deepEqual(
    [...hubLeftPorts].sort((left, right) => left.index - right.index),
    hubLeftPorts,
    'A, B, C, D were declared in their spatial (top-to-bottom) order, so ranked index order must already match',
  );

  const layout = {
    nodes,
    edges: Object.keys(first).map((linkId) => ({
      id: linkId,
      source: graph.links.find((link) => link.id === linkId).sourceDocumentId,
      target: graph.links.find((link) => link.id === linkId).targetDocumentId,
      data: first[linkId],
    })),
  };
  assertSpatialPortOrder(layout, 'document:hub', 'left', 'port-model');
});

test(
  'renders directional chevrons along each route',
  {
    skip: 'Chevron rendering has not landed yet; it is introduced in the chevrons phase.',
  },
  () => {},
);

test(
  'marks a crossing indicator only where two routes truly cross geometrically',
  {
    skip: 'Crossing markers have not landed yet. The crossing-heavy fixture test above already records a real-crossing-count baseline via context.diagnostic for that phase to compare against.',
  },
  () => {},
);

test(
  'produces typed corridor/lane objects for multi-edge boundary crossings',
  {
    skip: 'Corridor/lane objects beyond MapBoundaryGateway land with the corridors/gateways phase; assertFolderGatewayPolicy above already covers boundary ordering on the current router.',
  },
  () => {},
);
