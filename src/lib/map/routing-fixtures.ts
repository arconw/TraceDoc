import type {
  Document,
  DocumentId,
  DocumentLink,
  Folder,
  FolderId,
  ProjectModel,
} from '../types/workspace';

export interface RoutingFixtureDocument {
  path: string;
  title: string;
  links: string[];
}

export interface RoutingFixture {
  slug: string;
  name: string;
  invariant: string;
  documents: RoutingFixtureDocument[];
}

export function buildFixtureProject(fixture: RoutingFixture): ProjectModel {
  const folders: Record<FolderId, Folder> = {};
  const documents: Record<DocumentId, Document> = {};

  function ensureFolder(path: string): FolderId {
    const id = folderId(path);
    if (folders[id]) return id;
    const segments = path === '' ? [] : path.split('/');
    const name = segments.at(-1) ?? 'workspace';
    const parentId = path === '' ? null : ensureFolder(parentPath(path));
    folders[id] = {
      id,
      name,
      path,
      parentId,
      childFolderIds: [],
      documentIds: [],
    };
    if (parentId) folders[parentId].childFolderIds.push(id);
    return id;
  }

  ensureFolder('');

  for (const item of fixture.documents) {
    const id = documentId(item.path);
    const parentId = ensureFolder(parentPath(item.path));
    documents[id] = {
      id,
      name: item.path.split('/').at(-1)!,
      title: item.title,
      headings: [],
      path: item.path,
      parentId,
    };
    folders[parentId].documentIds.push(id);
  }

  for (const folder of Object.values(folders)) {
    folder.childFolderIds.sort();
    folder.documentIds.sort();
  }

  const links: DocumentLink[] = [];
  let linkIndex = 0;

  for (const item of fixture.documents) {
    const sourceId = documentId(item.path);
    for (const targetPath of item.links) {
      const targetId = documentId(targetPath);
      const targetExists = Boolean(documents[targetId]);
      links.push({
        id: `link:${fixture.slug}:${String(linkIndex).padStart(4, '0')}`,
        sourceDocumentId: sourceId,
        targetDocumentId: targetExists ? targetId : null,
        rawTarget: targetPath,
        resolved: targetExists,
        unresolvedReason: targetExists ? null : 'Target document was not found',
      });
      linkIndex += 1;
    }
  }

  return {
    rootPath: `/fixtures/${fixture.slug}`,
    folders,
    documents,
    links,
  };
}

export function routingFixtureBySlug(slug: string): RoutingFixture {
  const fixture = ROUTING_FIXTURES.find((item) => item.slug === slug);
  if (!fixture) throw new Error(`Unknown routing fixture: ${slug}`);
  return fixture;
}

function folderId(path: string): FolderId {
  return `folder:${path === '' ? '.' : path}`;
}

function documentId(path: string): DocumentId {
  return `document:${path}`;
}

function parentPath(path: string): string {
  const segments = path.split('/');
  return segments.slice(0, -1).join('/');
}

function doc(
  path: string,
  title: string,
  links: string[] = [],
): RoutingFixtureDocument {
  return { path, title, links };
}

function padded(index: number): string {
  return String(index).padStart(2, '0');
}

function fanInFixture(): RoutingFixture {
  const documents: RoutingFixtureDocument[] = [doc('hub.md', 'Hub')];
  for (let index = 1; index <= 20; index += 1) {
    documents.push(
      doc(`sources/module-${padded(index)}.md`, `Source ${padded(index)}`, [
        'hub.md',
      ]),
    );
  }
  return {
    slug: 'fan-in',
    name: 'Fan-in',
    invariant:
      '20 independent source documents each link to one shared target. Every edge must keep a distinct lane and a distinct endpoint along the target boundary, and none may cross an unrelated document.',
    documents,
  };
}

function fanOutFixture(): RoutingFixture {
  const targets = Array.from(
    { length: 20 },
    (_, index) => `targets/module-${padded(index + 1)}.md`,
  );
  const documents: RoutingFixtureDocument[] = [
    doc('hub.md', 'Hub', targets),
    ...targets.map((path, index) => doc(path, `Target ${padded(index + 1)}`)),
  ];
  return {
    slug: 'fan-out',
    name: 'Fan-out',
    invariant:
      'One shared source links to 20 independent targets. Every edge must keep a distinct lane and a distinct endpoint along the source boundary, and none may cross an unrelated document.',
    documents,
  };
}

function fanConvergeFixture(): RoutingFixture {
  const targetCount = 5;
  const targets = Array.from(
    { length: targetCount },
    (_, index) => `targets/module-${padded(index + 1)}.md`,
  );
  const sources: RoutingFixtureDocument[] = Array.from(
    { length: 20 },
    (_, index) =>
      doc(
        `sources/module-${padded(index + 1)}.md`,
        `Source ${padded(index + 1)}`,
        [targets[index % targetCount]],
      ),
  );
  const documents: RoutingFixtureDocument[] = [
    ...sources,
    ...targets.map((path, index) => doc(path, `Target ${padded(index + 1)}`)),
  ];
  return {
    slug: 'fan-converge',
    name: 'Fan-converge',
    invariant:
      '20 independent source documents distribute across 5 shared targets (4 sources per target). Every edge must keep a distinct lane and a distinct endpoint along its target boundary, and the shared source-to-target corridor must stay readable as an organized trunk rather than a merged bundle.',
    documents,
  };
}

function denseCorridorFixture(): RoutingFixture {
  const documents: RoutingFixtureDocument[] = [];
  for (let index = 1; index <= 20; index += 1) {
    const source = `west/module-${padded(index)}.md`;
    const target = `east/module-${padded(index)}.md`;
    documents.push(doc(source, `West ${padded(index)}`, [target]));
    documents.push(doc(target, `East ${padded(index)}`));
  }
  return {
    slug: 'dense-corridor',
    name: 'Dense same-direction corridor',
    invariant:
      '20 west-to-east links travel through the same region between two folders, all in the same direction. Every edge must stay orthogonal and finite, and hold an independent lane through the shared corridor.',
    documents,
  };
}

function bidirectionalCorridorFixture(): RoutingFixture {
  const documents: RoutingFixtureDocument[] = [];
  for (let index = 1; index <= 10; index += 1) {
    const left = `left/module-${padded(index)}.md`;
    const right = `right/module-${padded(index)}.md`;
    documents.push(doc(left, `Left ${padded(index)}`, [right]));
    documents.push(doc(right, `Right ${padded(index)}`, [left]));
  }
  return {
    slug: 'bidirectional-corridor',
    name: 'Bidirectional corridor',
    invariant:
      '10 document pairs link to each other in both directions through the same left/right corridor. The two opposite edges between one pair must never merge into a single logical edge and must resolve to distinct point sequences.',
    documents,
  };
}

function crossFolderHighwayFixture(): RoutingFixture {
  const documents: RoutingFixtureDocument[] = [];
  const count = 16;
  for (let index = 1; index <= count; index += 1) {
    const source = `frontend/module-${padded(index)}.md`;
    const target = `backend/module-${padded(index)}.md`;
    documents.push(doc(source, `Frontend ${padded(index)}`, [target]));
    documents.push(doc(target, `Backend ${padded(index)}`));
  }
  return {
    slug: 'cross-folder-highway',
    name: 'Cross-folder highway',
    invariant:
      '16 links cross from a frontend folder to a backend folder, a realistic TraceDoc scenario. Every edge must cross the frontend and backend folder boundaries through ordered, deterministic gateways and never traverse an unrelated folder.',
    documents,
  };
}

function nestedToExternalFixture(): RoutingFixture {
  const documents: RoutingFixtureDocument[] = [
    doc('frontend/components/widgets/panel.md', 'Panel', [
      'integrations/external/service.md',
    ]),
    doc('frontend/components/widgets/button.md', 'Button'),
    doc('integrations/external/service.md', 'External Service'),
    doc('integrations/external/other-service.md', 'Other External Service'),
  ];
  return {
    slug: 'nested-to-external',
    name: 'Nested folders',
    invariant:
      'A document nested three folders deep links to a document in an unrelated, similarly nested destination folder. The route must ascend every source boundary and descend every target boundary in hierarchy order, never cutting through a sibling folder or document.',
    documents,
  };
}

function crossingHeavyFixture(): RoutingFixture {
  const count = 8;
  const leftDocs = Array.from(
    { length: count },
    (_, index) => `left/module-${padded(index + 1)}.md`,
  );
  const rightDocs = Array.from(
    { length: count },
    (_, index) => `right/module-${padded(index + 1)}.md`,
  );
  const documents: RoutingFixtureDocument[] = leftDocs.map((path, index) =>
    doc(path, `Left ${padded(index + 1)}`, [rightDocs[count - 1 - index]]),
  );
  documents.push(
    ...rightDocs.map((path, index) => doc(path, `Right ${padded(index + 1)}`)),
  );
  return {
    slug: 'crossing-heavy',
    name: 'Crossing-heavy graph',
    invariant:
      'Left document i links to right document (count - 1 - i), reversing order so most edges geometrically cross each other. Every edge must still stay orthogonal, finite, and free of unrelated document interiors; a real geometric crossing is not itself a fault.',
    documents,
  };
}

function mixedHubFixture(): RoutingFixture {
  const inbound = Array.from(
    { length: 8 },
    (_, index) => `inbound/module-${padded(index + 1)}.md`,
  );
  const outbound = Array.from(
    { length: 8 },
    (_, index) => `outbound/module-${padded(index + 1)}.md`,
  );
  const documents: RoutingFixtureDocument[] = [
    doc('hub.md', 'Hub', outbound),
    ...inbound.map((path, index) =>
      doc(path, `Inbound ${padded(index + 1)}`, ['hub.md']),
    ),
    ...outbound.map((path, index) =>
      doc(path, `Outbound ${padded(index + 1)}`),
    ),
  ];
  return {
    slug: 'mixed-hub',
    name: 'High-degree mixed hub',
    invariant:
      'One hub document carries 8 incoming and 8 outgoing edges at once. Incoming and outgoing lanes on the hub boundary must remain independently ordered and never collide.',
    documents,
  };
}

function unrelatedNearCorridorFixture(): RoutingFixture {
  const documents: RoutingFixtureDocument[] = [];
  for (let index = 1; index <= 5; index += 1) {
    const source = `corridor/left/module-${padded(index)}.md`;
    const target = `corridor/right/module-${padded(index)}.md`;
    documents.push(doc(source, `Corridor Left ${padded(index)}`, [target]));
    documents.push(doc(target, `Corridor Right ${padded(index)}`));
  }
  for (let index = 1; index <= 4; index += 1) {
    const source = `nearby/module-${padded(index)}.md`;
    const target = `nearby/module-${padded(index + 1)}.md`;
    documents.push(doc(source, `Nearby ${padded(index)}`, [target]));
  }
  documents.push(doc('nearby/module-05.md', 'Nearby 05'));
  return {
    slug: 'unrelated-near-corridor',
    name: 'Unrelated routes near a valid corridor',
    invariant:
      'A valid five-link left/right corridor sits next to an unrelated folder with its own independent link chain. Neither graph may route through the interior of a document belonging to the other.',
    documents,
  };
}

function denseSkipChainFixture(): RoutingFixture {
  const count = 18;
  const skip = 4;
  const documents: RoutingFixtureDocument[] = [];
  for (let index = 1; index <= count; index += 1) {
    const links: string[] = [];
    if (index + 1 <= count)
      links.push(`modules/module-${padded(index + 1)}.md`);
    if (index + skip <= count)
      links.push(`modules/module-${padded(index + skip)}.md`);
    documents.push(
      doc(
        `modules/module-${padded(index)}.md`,
        `Module ${padded(index)}`,
        links,
      ),
    );
  }
  return {
    slug: 'dense-skip-chain',
    name: 'Dense chain with skip-ahead links',
    invariant:
      '18 densely packed documents in one folder form a sequential chain, each also linking 4 positions ahead. The tightly spaced initial layout forces several of those skip-ahead links into real geometric crossings; the bounded layout/routing feedback pass (phase 5) must reduce total routing cost relative to a single, unadjusted ELK pass without exceeding its bounded iteration/time budget.',
    documents,
  };
}

function incrementalModuleGraph(extra: boolean): RoutingFixtureDocument[] {
  const moduleCount = 12;
  const documents: RoutingFixtureDocument[] = [];
  for (let index = 1; index <= moduleCount; index += 1) {
    const path = `modules/module-${padded(index)}.md`;
    const nextIndex = index === moduleCount ? 1 : index + 1;
    const links = [`modules/module-${padded(nextIndex)}.md`];
    if (extra && index === 1) links.push('modules/module-13.md');
    documents.push(doc(path, `Module ${padded(index)}`, links));
  }
  if (extra) {
    documents.push(
      doc('modules/module-13.md', 'Module 13', ['modules/module-01.md']),
    );
  }
  return documents;
}

function incrementalBaseFixture(): RoutingFixture {
  return {
    slug: 'incremental-base',
    name: 'Incremental base',
    invariant:
      'A 12-document ring graph, the baseline for incremental-change stability testing. Layout and routing must be identical across repeated runs of this exact graph.',
    documents: incrementalModuleGraph(false),
  };
}

function incrementalNextFixture(): RoutingFixture {
  return {
    slug: 'incremental-next',
    name: 'Incremental next',
    invariant:
      'The same ring graph as incremental-base with one document and one link added. Compare against incremental-base to measure how much of the layout changes for a small, local graph edit; the current router does not guarantee a minimal diff, so this is a measurable metric rather than a hard pass/fail invariant.',
    documents: incrementalModuleGraph(true),
  };
}

export const ROUTING_FIXTURES: RoutingFixture[] = [
  fanInFixture(),
  fanOutFixture(),
  fanConvergeFixture(),
  denseCorridorFixture(),
  bidirectionalCorridorFixture(),
  crossFolderHighwayFixture(),
  nestedToExternalFixture(),
  crossingHeavyFixture(),
  mixedHubFixture(),
  unrelatedNearCorridorFixture(),
  denseSkipChainFixture(),
  incrementalBaseFixture(),
  incrementalNextFixture(),
];
