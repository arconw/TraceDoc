# ELK Layout Adapter

Source: `src/lib/map/elk-layout.ts`.

Builds deterministic recursive compound placement with ELK, then invokes [`routing`](routing.ts.md) and emits Svelte Flow nodes/edges.

## Contracts

- `MapNodeData` — folder/document label/path, degree, emphasis, open/trace callbacks.
- `MapFlowNode` — non-draggable folder/document node.
- `MapEdgeData` — orthogonal points, endpoints/sides/gateways, accessibility label, emphasis/trace callback.
- `MapFlowEdge` — nonselectable custom route edge.
- `ElkMapNode` — internal recursive layout node.
- `MapLayout` — final node/edge arrays.

## Functions

- `layoutMapGraph(graph, engine)` — lays out root folders in parallel, then flattens.
- `elkGraphToFlow(graph, layout)` — recursively converts geometry, counts degrees, routes links, and builds flow records.
- nested `visit(children, parentId)` — preserves compound parent relationships.
- `layoutFolder(graph, folderId, engine)` — bottom-up child-folder/document placement with header/padding.
- `layoutRootFolders(folders, engine)` — positions multiple top-level containers.
- `layoutEdgesForFolder(graph, folderId)` — deduplicates layout-only direct-child constraints.
- `directChildForDocument(graph, folderId, documentId)` — maps a deep document to the immediate child block at a layout level.
- `buildDocumentNode(graph, documentId)` — derives bounded width from title length.
- `layoutOptions()` — deterministic rightward layered ELK configuration and spacing.
- `fixedSidePorts(nodeId)` — declares top/right/bottom/left ELK ports.
- `linkCounts(graph, field)` — computes incoming/outgoing degree.

Invariant: ELK places blocks; custom routing owns final edge geometry.
