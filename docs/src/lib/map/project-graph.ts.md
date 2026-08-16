# Project Graph Adapter

Source: `src/lib/map/project-graph.ts`.

Pure projection from canonical [`ProjectModel`](../types/workspace.ts.md) to map-only hierarchy and resolved internal edges.

## Types

- `MapFolder`, `MapDocument`, `MapLink` — minimal layout records.
- `MapGraph` — root folder IDs plus normalized folder/document maps and links.

## Functions

- `projectToMapGraph(project)` — filters missing/unresolved/external links, normalizes titles, and code-point sorts hierarchy/edges.
- `mapLayoutSignature(project)` — stable JSON signature of layout-relevant data; body/headings are intentionally excluded.
- `sortIdsByPath(ids, records)` — stable path then ID ordering.
- `compareCodePoints(left, right)` — locale-independent Unicode code-point comparator.

Consumers: [`elk-layout`](elk-layout.ts.md), [`MapView`](../views/MapView.svelte.md).
