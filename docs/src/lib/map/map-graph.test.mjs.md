# Map Graph and Routing Tests

Source: `src/lib/map/map-graph.test.mjs`.

esbuild-loads production TypeScript modules and exercises pure state, ELK layout, routing geometry, determinism, and performance.

## Helpers and fixtures

- `loadTypeScript(relativePath)` — esbuild-based module loader.
- controllable layout job `start()` / `cancel()` — cancellation oracle.
- `nestedProject`, `syntheticProject`, `fanProject`, `document`, `link` — deterministic graph fixtures.
- `duplicateNameProject(areaCount,modulesPerArea)` — large deterministic fixture with one document per nested area/module folder, filenames drawn from a small repeating pool (`REPEATED_DOCUMENT_NAMES`) so identical names recur across different folders; links include an in-area chain, cross-area links between same-named documents, and interleaved unresolved links.
- `oracleConnections(graph)` — per-document one-hop neighbor `Set`, derived directly from `MapGraph.links`, independent of the code under test.
- `event` — edge-trace event builder.
- `assertOrthogonal`, `absoluteRectangles`, nested `positionFor`, `routeIntersectsRect`, `assertAvoidsDocumentInteriors` — geometry assertions.
- `ancestorIds`, `expectedBoundaryIds`, `assertFolderGatewayPolicy` — hierarchy/gateway oracle.
- `positions(layout)` — deterministic node-position projection.

## Cases

- Body-only signature stability; visible burst coalescing; hidden deferral.
- Active-job cancellation, latest-only execution, retry/hide/signature transitions.
- Independent pointer/keyboard edge traces and current-layout-only controls.
- Resolved-link graph derivation and deterministic nested ELK flattening.
- 20-link fan-in/fan-out independent lanes.
- Ten cross-folder links through deterministic gateways.
- Dense 60-document/240-link finite orthogonal obstacle-safe routing.
- Self-links and duplicate relations remain independent.
- 100/200 and 500/1000 document/link performance gates.
- 240-document/duplicate-filename regression: hovering the rightmost (and thus most left-neighbored) document must, against the `oracleConnections` oracle, mark exactly the hovered document `'active'`, its true one-hop neighbors `'connected'`, and every other document — including everything positioned to its left and every repeated-name document — `'muted'`; also proves a leftover unrelated edge trace collides (masks the hovered document, wrongly activates an unrelated one) until `clearMapEdgeTraceForDocument` resets it, and that moving the trace to a second document clears the first.
- Edge trace left behind by pointer vs. keyboard focus: `clearMapEdgeTraceForDocument` resets either source identically, producing the same oracle-matching highlighted set.
- Live edge trace survives an overriding document hover: with an edge's `focusedEdgeId` genuinely set, calling the production `resolveTracedEdge(state,documentTraceActive,layoutEdges)` — the same function MapView's reactive `tracedEdge` calls, not a reimplementation — with `documentTraceActive: true` makes `nodeEmphasis`/`edgeEmphasis` resolve purely from the document while `edgeTraceState.focusedEdgeId` stays untouched; calling `resolveTracedEdge` again with `documentTraceActive: false` against that same untouched state resolves back to the original edge with no new trace event.
- Flow unmount resets a stale document trace via `resetMapTraceOnFlowUnmount`, restoring immediate edge tracing on remount with no flush-first document required.
- Per-node unmount via `clearMapDocumentTraceIfActive`: a still-active document's trace is cleared when its own node is destroyed (`onlyRenderVisibleElements` unmounting an individual virtualized node without an `onblur`/`onpointerleave`); a stale destroy for a document that is no longer the active trace (the user already moved on to a different document before the destroy callback ran) leaves the new, genuinely active trace untouched.

Targets: all modules in this directory except viewport lifecycle.
