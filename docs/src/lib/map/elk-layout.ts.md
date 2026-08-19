# ELK Layout Adapter

Source: `src/lib/map/elk-layout.ts`.

Builds deterministic recursive compound placement with ELK, then invokes [`routing`](routing.ts.md) and emits Svelte Flow nodes/edges.

## Contracts

- `MapNodeData` — folder/document label/path, degree, this document's `ports` (`MapPort[]`, from [`routing`](routing.ts.md)), emphasis, the currently traced edge ID (`activeEdgeId`), open/trace/trace-unmount callbacks.
- `MapFlowNode` — non-draggable folder/document node.
- `MapEdgeData` — orthogonal points, endpoint sides, this edge's own `sourcePort`/`targetPort`, gateways, accessibility label, emphasis/trace callback.
- `MapFlowEdge` — nonselectable custom route edge.
- `ElkMapNode` — internal recursive layout node.
- `MapLayout` — final node/edge arrays.

## Functions

- `layoutMapGraph(graph, engine)` — lays out root folders in parallel, then flattens.
- `elkGraphToFlow(graph, layout)` — recursively converts geometry, counts degrees, routes links, attaches each document's own ports, and builds flow records.
- nested `visit(children, parentId)` — preserves compound parent relationships.
- `mapHandleId(port)` — the single place a `MapPort` becomes a Svelte Flow handle ID (`<direction>-<side>-<index>`); shared by the edge's `sourceHandle`/`targetHandle` here and by [`MapDocumentNode`](../components/MapDocumentNode.svelte.md)'s own `Handle` rendering, so the two always agree.
- `attachDocumentPorts(nodes, routes)` — after routing, groups every route's `sourcePort`/`targetPort` by `documentId` and writes each document node's own `data.ports`, sorted by side then index then direction for stable iteration; a document with no incident edges gets an empty array.
- `layoutFolder(graph, folderId, engine)` — bottom-up child-folder/document placement with header/padding.
- `layoutRootFolders(folders, engine)` — positions multiple top-level containers.
- `layoutEdgesForFolder(graph, folderId)` — deduplicates layout-only direct-child constraints.
- `directChildForDocument(graph, folderId, documentId)` — maps a deep document to the immediate child block at a layout level.
- `buildDocumentNode(graph, documentId)` — derives bounded width from title length.
- `layoutOptions()` — deterministic rightward layered ELK configuration and spacing.
- `fixedSidePorts(nodeId)` — declares top/right/bottom/left ELK ports (compound-layout placement only; unrelated to the visual `MapPort` model above).
- `linkCounts(graph, field)` — computes incoming/outgoing degree.

Invariant: ELK places blocks; custom routing owns final edge geometry and the port model built on top of it.
