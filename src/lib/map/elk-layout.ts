import type { Edge, Node } from '@xyflow/svelte';
import type { ELK, ElkExtendedEdge, ElkNode } from 'elkjs/lib/elk-api';
import type { MapGraph } from './project-graph';

const DOCUMENT_HEIGHT = 58;
const DOCUMENT_MIN_WIDTH = 176;
const DOCUMENT_MAX_WIDTH = 272;
const FOLDER_HEADER = 44;
const FOLDER_PADDING = 20;

export interface MapNodeData extends Record<string, unknown> {
  kind: 'folder' | 'document';
  label: string;
  name: string;
  path: string;
}

export type MapFlowNode = Node<MapNodeData, 'mapFolder' | 'mapDocument'>;
export type MapFlowEdge = Edge<Record<string, never>, 'smoothstep'>;

interface ElkMapNode extends ElkNode {
  mapKind: 'folder' | 'document';
  children?: ElkMapNode[];
}

export interface MapLayout {
  nodes: MapFlowNode[];
  edges: MapFlowEdge[];
}

export async function layoutMapGraph(
  graph: MapGraph,
  engine: Pick<ELK, 'layout'>,
): Promise<MapLayout> {
  const rootFolders = await Promise.all(
    graph.rootFolderIds.map((folderId) =>
      layoutFolder(graph, folderId, engine),
    ),
  );
  const root = await layoutRootFolders(rootFolders, engine);
  return elkGraphToFlow(graph, root);
}

export function elkGraphToFlow(graph: MapGraph, layout: ElkMapNode): MapLayout {
  const nodes: MapFlowNode[] = [];

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
          },
        });
      }
    }
  }

  visit(layout.children ?? []);

  return {
    nodes,
    edges: graph.links.map((link) => ({
      id: link.id,
      source: link.sourceDocumentId,
      target: link.targetDocumentId,
      type: 'smoothstep',
      markerEnd: {
        type: 'arrowclosed',
        color: '#7c899e',
        width: 14,
        height: 14,
      },
      focusable: false,
      selectable: false,
      deletable: false,
      interactionWidth: 10,
    })),
  };
}

async function layoutFolder(
  graph: MapGraph,
  folderId: string,
  engine: Pick<ELK, 'layout'>,
): Promise<ElkMapNode> {
  const folder = graph.folders[folderId];
  const childFolders = await Promise.all(
    folder.childFolderIds.map((childFolderId) =>
      layoutFolder(graph, childFolderId, engine),
    ),
  );
  const documents = folder.documentIds
    .filter((documentId) => Boolean(graph.documents[documentId]))
    .map((documentId) => buildDocumentNode(graph, documentId));
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
    layoutOptions: layoutOptions(),
    children: children.map(({ id, width, height }) => ({ id, width, height })),
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
    layoutOptions: layoutOptions(),
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
  const edges = new Map<string, ElkExtendedEdge>();

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
    const pairId = `${source}\u0000${target}`;
    if (!edges.has(pairId)) {
      edges.set(pairId, {
        id: `layout:${folderId}:${link.id}`,
        sources: [source],
        targets: [target],
      });
    }
  }

  return [...edges.values()];
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

function buildDocumentNode(graph: MapGraph, documentId: string): ElkMapNode {
  const document = graph.documents[documentId];
  const measuredWidth = 64 + Array.from(document.title).length * 7.2;

  return {
    id: document.id,
    mapKind: 'document',
    width: Math.min(
      DOCUMENT_MAX_WIDTH,
      Math.max(DOCUMENT_MIN_WIDTH, measuredWidth),
    ),
    height: DOCUMENT_HEIGHT,
  };
}

function layoutOptions() {
  return {
    'elk.algorithm': 'layered',
    'elk.direction': 'RIGHT',
    'elk.edgeRouting': 'ORTHOGONAL',
    'elk.layered.considerModelOrder.strategy': 'NODES_AND_EDGES',
    'elk.layered.spacing.nodeNodeBetweenLayers': '72',
    'elk.randomSeed': '1',
    'elk.separateConnectedComponents': 'false',
    'elk.spacing.componentComponent': '52',
    'elk.spacing.nodeNode': '36',
  };
}
