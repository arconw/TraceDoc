import type { Edge, Node } from '@xyflow/svelte';
import type { ELK, ElkExtendedEdge, ElkNode } from 'elkjs/lib/elk-api';
import type { MapGraph } from './project-graph';
import {
  absoluteRectangles,
  computeRoutingCost,
  routeMapLinks,
  type MapBoundaryGateway,
  type MapChevron,
  type MapCorridorAssignment,
  type MapPoint,
  type MapPort,
  type MapRect,
  type MapRoute,
  type MapRoutingCost,
  type MapSide,
} from './routing';

const DOCUMENT_HEIGHT = 58;
const DOCUMENT_MIN_WIDTH = 176;
const DOCUMENT_MAX_WIDTH = 272;
const FOLDER_HEADER = 44;
const FOLDER_PADDING = 20;
const HIGH_DEGREE_THRESHOLD = 8;
const LAYOUT_FEEDBACK_ITERATIONS = 3;
const LAYOUT_SPACING_MULTIPLIERS = [1, 1.35, 1.75];
const LAYOUT_FEEDBACK_TIME_BUDGET_MS = 4000;

export interface MapNodeData extends Record<string, unknown> {
  kind: 'folder' | 'document';
  label: string;
  name: string;
  path: string;
  documentId?: string;
  incomingCount?: number;
  outgoingCount?: number;
  ports?: MapPort[];
  emphasis?: 'normal' | 'active' | 'connected' | 'muted';
  activeEdgeId?: string | null;
  onOpenDocument?: (documentId: string) => void;
  onTraceDocument?: (documentId: string | null) => void;
  onTraceDocumentUnmount?: (documentId: string) => void;
}

export type MapFlowNode = Node<MapNodeData, 'mapFolder' | 'mapDocument'>;
export interface MapEdgeData extends Record<string, unknown> {
  points: MapPoint[];
  sourceDocumentId: string;
  targetDocumentId: string;
  sourceSide: MapSide;
  targetSide: MapSide;
  sourcePort: MapPort;
  targetPort: MapPort;
  boundaryGateways: MapBoundaryGateway[];
  chevrons: MapChevron[];
  crossingGaps: MapPoint[];
  corridor: MapCorridorAssignment | null;
  ariaLabel: string;
  emphasis?: 'normal' | 'active' | 'muted';
  onTracePointerEdge?: (edgeId: string | null) => void;
}

export type MapFlowEdge = Edge<MapEdgeData, 'mapRoute'>;

interface ElkMapNode extends ElkNode {
  mapKind: 'folder' | 'document';
  children?: ElkMapNode[];
}

export interface MapLayout {
  nodes: MapFlowNode[];
  edges: MapFlowEdge[];
}

export interface MapLayoutOptions {
  maxIterations?: number;
}

export async function layoutMapGraph(
  graph: MapGraph,
  engine: Pick<ELK, 'layout'>,
  options: MapLayoutOptions = {},
): Promise<MapLayout> {
  const degrees = computeNodeDegrees(graph);
  const startedAt = performance.now();
  const maxIterations = Math.max(
    1,
    Math.min(
      LAYOUT_FEEDBACK_ITERATIONS,
      options.maxIterations ?? LAYOUT_FEEDBACK_ITERATIONS,
    ),
  );
  let best: { layout: MapLayout; total: number } | null = null;
  let previousTotal = Number.POSITIVE_INFINITY;

  for (let iteration = 0; iteration < maxIterations; iteration += 1) {
    const spacingMultiplier =
      LAYOUT_SPACING_MULTIPLIERS[iteration] ??
      LAYOUT_SPACING_MULTIPLIERS.at(-1)!;
    const layout = await layoutPass(graph, engine, degrees, spacingMultiplier);
    const cost = measureLayoutCost(layout);

    if (!best || cost.total < best.total) best = { layout, total: cost.total };
    const improved = cost.total < previousTotal;
    previousTotal = cost.total;
    const elapsed = performance.now() - startedAt;

    if (cost.structural === 0) break;
    if (!improved && iteration > 0) break;
    if (elapsed >= LAYOUT_FEEDBACK_TIME_BUDGET_MS) break;
  }

  return best!.layout;
}

async function layoutPass(
  graph: MapGraph,
  engine: Pick<ELK, 'layout'>,
  degrees: Map<string, number>,
  spacingMultiplier: number,
): Promise<MapLayout> {
  const rootFolders = await Promise.all(
    graph.rootFolderIds.map((folderId) =>
      layoutFolder(graph, folderId, engine, degrees, spacingMultiplier),
    ),
  );
  const root = await layoutRootFolders(rootFolders, engine, spacingMultiplier);
  return elkGraphToFlow(graph, root);
}

function measureLayoutCost(layout: MapLayout): MapRoutingCost {
  const routes: Record<string, MapRoute> = {};
  for (const edge of layout.edges) routes[edge.id] = edge.data as MapRoute;

  const rectangles = absoluteRectangles(layout.nodes);
  const documentRects: Record<string, MapRect> = {};
  for (const node of layout.nodes) {
    if (node.data.kind === 'document')
      documentRects[node.id] = rectangles[node.id];
  }

  return computeRoutingCost(routes, documentRects);
}

function computeNodeDegrees(graph: MapGraph): Map<string, number> {
  const degrees = new Map<string, number>();
  for (const link of graph.links) {
    degrees.set(
      link.sourceDocumentId,
      (degrees.get(link.sourceDocumentId) ?? 0) + 1,
    );
    degrees.set(
      link.targetDocumentId,
      (degrees.get(link.targetDocumentId) ?? 0) + 1,
    );
  }
  return degrees;
}

export function elkGraphToFlow(graph: MapGraph, layout: ElkMapNode): MapLayout {
  const nodes: MapFlowNode[] = [];
  const incomingCounts = linkCounts(graph, 'targetDocumentId');
  const outgoingCounts = linkCounts(graph, 'sourceDocumentId');

  function visit(children: ElkMapNode[], parentId?: string) {
    for (const child of children) {
      const position = { x: child.x ?? 0, y: child.y ?? 0 };

      if (child.mapKind === 'folder') {
        const folder = graph.folders[child.id];
        if (!folder) continue;
        nodes.push({
          id: folder.id,
          type: 'mapFolder',
          position,
          width: child.width ?? 200,
          height: child.height ?? 96,
          parentId,
          zIndex: 0,
          draggable: false,
          selectable: false,
          connectable: false,
          focusable: false,
          deletable: false,
          data: {
            kind: 'folder',
            label: folder.name,
            name: folder.name,
            path: folder.path,
          },
        });
        visit(child.children ?? [], folder.id);
      } else {
        const document = graph.documents[child.id];
        if (!document) continue;
        nodes.push({
          id: document.id,
          type: 'mapDocument',
          position,
          width: child.width ?? DOCUMENT_MIN_WIDTH,
          height: child.height ?? DOCUMENT_HEIGHT,
          parentId,
          zIndex: 2,
          draggable: false,
          selectable: false,
          connectable: false,
          focusable: false,
          deletable: false,
          data: {
            kind: 'document',
            label: document.title,
            name: document.name,
            path: document.path,
            documentId: document.id,
            incomingCount: incomingCounts[document.id] ?? 0,
            outgoingCount: outgoingCounts[document.id] ?? 0,
          },
        });
      }
    }
  }

  visit(layout.children ?? []);
  const routes = routeMapLinks(graph, nodes);
  attachDocumentPorts(nodes, routes);

  return {
    nodes,
    edges: graph.links.flatMap((link) => {
      const route = routes[link.id];
      if (!route) return [];
      const source = graph.documents[link.sourceDocumentId];
      const target = graph.documents[link.targetDocumentId];
      return [
        {
          id: link.id,
          source: link.sourceDocumentId,
          target: link.targetDocumentId,
          sourceHandle: mapHandleId(route.sourcePort),
          targetHandle: mapHandleId(route.targetPort),
          type: 'mapRoute' as const,
          zIndex: 1,
          markerEnd: {
            type: 'arrowclosed' as const,
            color: '#7c899e',
            width: 11,
            height: 11,
          },
          ariaRole: 'presentation' as const,
          domAttributes: { 'aria-hidden': 'true' },
          focusable: false,
          selectable: false,
          deletable: false,
          interactionWidth: 12,
          data: {
            ...route,
            sourceDocumentId: link.sourceDocumentId,
            targetDocumentId: link.targetDocumentId,
            ariaLabel: `${source.title} references ${target.title}`,
          },
        },
      ];
    }),
  };
}

async function layoutFolder(
  graph: MapGraph,
  folderId: string,
  engine: Pick<ELK, 'layout'>,
  degrees: Map<string, number>,
  spacingMultiplier: number,
): Promise<ElkMapNode> {
  const folder = graph.folders[folderId];
  const childFolders = await Promise.all(
    folder.childFolderIds.map((childFolderId) =>
      layoutFolder(graph, childFolderId, engine, degrees, spacingMultiplier),
    ),
  );
  const documents = folder.documentIds
    .filter((documentId) => Boolean(graph.documents[documentId]))
    .map((documentId) => buildDocumentNode(graph, documentId, degrees));
  const children = [...childFolders, ...documents];

  if (children.length === 0) {
    return {
      id: folder.id,
      mapKind: 'folder',
      width: 200,
      height: 96,
      children: [],
    };
  }

  const layoutInput: ElkNode = {
    id: `layout:${folder.id}`,
    layoutOptions: layoutOptions(spacingMultiplier),
    children: children.map(({ id, width, height, layoutOptions: hints }) => ({
      id,
      width,
      height,
      layoutOptions: { 'elk.portConstraints': 'FIXED_SIDE', ...hints },
      ports: fixedSidePorts(id),
    })),
    edges: layoutEdgesForFolder(graph, folder.id),
  };
  const positioned = await engine.layout(layoutInput);
  const positions = new Map(
    (positioned.children ?? []).map((child) => [child.id, child]),
  );
  const positionedChildren = children.map((child) => {
    const position = positions.get(child.id);
    return {
      ...child,
      x: (position?.x ?? 0) + FOLDER_PADDING,
      y: (position?.y ?? 0) + FOLDER_HEADER,
    };
  });

  return {
    id: folder.id,
    mapKind: 'folder',
    width: (positioned.width ?? 160) + FOLDER_PADDING * 2,
    height: (positioned.height ?? 52) + FOLDER_HEADER + FOLDER_PADDING,
    children: positionedChildren,
  };
}

async function layoutRootFolders(
  folders: ElkMapNode[],
  engine: Pick<ELK, 'layout'>,
  spacingMultiplier: number,
): Promise<ElkMapNode> {
  if (folders.length <= 1) {
    return {
      id: 'tracedoc-map-root',
      mapKind: 'folder',
      children: folders.map((folder) => ({ ...folder, x: 0, y: 0 })),
    };
  }

  const positioned = await engine.layout({
    id: 'tracedoc-map-root-layout',
    layoutOptions: layoutOptions(spacingMultiplier),
    children: folders.map(({ id, width, height }) => ({ id, width, height })),
  });
  const positions = new Map(
    (positioned.children ?? []).map((child) => [child.id, child]),
  );

  return {
    id: 'tracedoc-map-root',
    mapKind: 'folder',
    children: folders.map((folder) => ({
      ...folder,
      x: positions.get(folder.id)?.x ?? 0,
      y: positions.get(folder.id)?.y ?? 0,
    })),
  };
}

function layoutEdgesForFolder(
  graph: MapGraph,
  folderId: string,
): ElkExtendedEdge[] {
  const edges: ElkExtendedEdge[] = [];
  const relationships = new Set<string>();

  for (const link of graph.links) {
    const source = directChildForDocument(
      graph,
      folderId,
      link.sourceDocumentId,
    );
    const target = directChildForDocument(
      graph,
      folderId,
      link.targetDocumentId,
    );

    if (!source || !target || source === target) continue;
    const relationship = `${source}\u0000${target}`;
    if (relationships.has(relationship)) continue;
    relationships.add(relationship);
    edges.push({
      id: `layout:${folderId}:${link.id}`,
      sources: [`${source}:right`],
      targets: [`${target}:left`],
    });
  }

  return edges;
}

function directChildForDocument(
  graph: MapGraph,
  folderId: string,
  documentId: string,
) {
  const document = graph.documents[documentId];
  if (!document) return null;
  if (document.parentId === folderId) return document.id;

  let currentFolder = graph.folders[document.parentId];
  while (currentFolder?.parentId) {
    if (currentFolder.parentId === folderId) return currentFolder.id;
    currentFolder = graph.folders[currentFolder.parentId];
  }

  return null;
}

function buildDocumentNode(
  graph: MapGraph,
  documentId: string,
  degrees: Map<string, number>,
): ElkMapNode {
  const document = graph.documents[documentId];
  const measuredWidth = 64 + Array.from(document.title).length * 7.2;
  const degree = degrees.get(documentId) ?? 0;

  return {
    id: document.id,
    mapKind: 'document',
    width: Math.min(
      DOCUMENT_MAX_WIDTH,
      Math.max(DOCUMENT_MIN_WIDTH, measuredWidth),
    ),
    height: DOCUMENT_HEIGHT,
    layoutOptions:
      degree >= HIGH_DEGREE_THRESHOLD
        ? { 'elk.priority': String(Math.min(degree, 999)) }
        : undefined,
  };
}

function layoutOptions(spacingMultiplier: number) {
  const scaled = (value: number) =>
    String(Math.round(value * spacingMultiplier));

  return {
    'elk.algorithm': 'layered',
    'elk.direction': 'RIGHT',
    'elk.edgeRouting': 'ORTHOGONAL',
    'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
    'elk.layered.considerModelOrder.strategy': 'NODES_AND_EDGES',
    'elk.layered.highDegreeNodes.threshold': '8',
    'elk.layered.highDegreeNodes.treatment': 'true',
    'elk.layered.mergeEdges': 'false',
    'elk.layered.nodePlacement.favorStraightEdges': 'true',
    'elk.layered.nodePlacement.strategy': 'SIMPLE',
    'elk.layered.spacing.edgeEdgeBetweenLayers': scaled(10),
    'elk.layered.spacing.edgeNodeBetweenLayers': scaled(24),
    'elk.layered.spacing.nodeNodeBetweenLayers': scaled(88),
    'elk.layered.unnecessaryBendpoints': 'true',
    'elk.randomSeed': '1',
    'elk.separateConnectedComponents': 'false',
    'elk.spacing.componentComponent': scaled(52),
    'elk.spacing.edgeEdge': scaled(10),
    'elk.spacing.edgeNode': scaled(24),
    'elk.spacing.nodeNode': scaled(44),
  };
}

function fixedSidePorts(nodeId: string) {
  return (['top', 'right', 'bottom', 'left'] as const).map((side) => ({
    id: `${nodeId}:${side}`,
    width: 1,
    height: 1,
    layoutOptions: {
      'elk.port.side': side.toUpperCase(),
    },
  }));
}

export function mapHandleId(port: MapPort) {
  return `${port.direction}-${port.side}-${port.index}`;
}

function attachDocumentPorts(
  nodes: MapFlowNode[],
  routes: Record<string, { sourcePort: MapPort; targetPort: MapPort }>,
) {
  const portsByNode = new Map<string, MapPort[]>();
  for (const route of Object.values(routes)) {
    for (const port of [route.sourcePort, route.targetPort]) {
      const existing = portsByNode.get(port.documentId) ?? [];
      existing.push(port);
      portsByNode.set(port.documentId, existing);
    }
  }
  for (const node of nodes) {
    if (node.data.kind !== 'document') continue;
    const ports = portsByNode.get(node.id) ?? [];
    ports.sort(
      (left, right) =>
        left.side.localeCompare(right.side) ||
        left.index - right.index ||
        left.direction.localeCompare(right.direction),
    );
    node.data.ports = ports;
  }
}

function linkCounts(
  graph: MapGraph,
  field: 'sourceDocumentId' | 'targetDocumentId',
) {
  return graph.links.reduce<Record<string, number>>((counts, link) => {
    counts[link[field]] = (counts[link[field]] ?? 0) + 1;
    return counts;
  }, {});
}
