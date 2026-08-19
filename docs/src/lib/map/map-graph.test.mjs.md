# Map Graph and Routing Tests

Source: `src/lib/map/map-graph.test.mjs`.

esbuild-loads production TypeScript modules and exercises pure state, ELK layout, routing geometry, determinism, and performance.

## Helpers and fixtures

- `loadTypeScript(relativePath)` — esbuild-based module loader.
- `buildFixtureProject`, `ROUTING_FIXTURES`, `routingFixtureBySlug` — loaded from [`routing-fixtures.ts`](routing-fixtures.ts.md); the deterministic stress-graph fixtures shared with the on-disk debug workspace generator.
- controllable layout job `start()` / `cancel()` — cancellation oracle.
- `nestedProject`, `syntheticProject`, `fanProject`, `document`, `link` — deterministic ProjectModel-shaped graph fixtures.
- `duplicateNameProject(areaCount,modulesPerArea)` — large deterministic fixture with one document per nested area/module folder, filenames drawn from a small repeating pool (`REPEATED_DOCUMENT_NAMES`) so identical names recur across different folders; links include an in-area chain, cross-area links between same-named documents, and interleaved unresolved links.
- `oracleConnections(graph)` — per-document one-hop neighbor `Set`, derived directly from `MapGraph.links`, independent of the code under test.
- `event` — edge-trace event builder.
- `assertOrthogonal`, `absoluteRectangles`, nested `positionFor`, `routeIntersectsRect`, `assertAvoidsDocumentInteriors` — geometry assertions.
- `sideFromHandle(handle, prefix)`, `movesTowardSide(from, to, side)` — parse a `source-<side>`/`target-<side>` handle into a `MapSide`, and check a point pair moves in that side's direction; used by `assertValidArrowAndPorts`.
- `ancestorIds`, `expectedBoundaryIds`, `assertFolderGatewayPolicy` — hierarchy/gateway oracle.
- `positions(layout)` — deterministic node-position projection.
- `routableGraph(folders, documents, links)`, `routableDocument`, `routableLink` — minimal `MapGraph`/`RoutableMapNode` fixtures that call `routeMapLinks` directly, bypassing ELK; `routableGraph` accepts an array of folders (the first is the root) so fixtures can nest zones.
- `boundaryNoiseFixture` — reproduces a zone-router obstacle whose inflated boundary and lead point diverge by a floating-point ULP.
- `unroutableZoneFixture` — one genuinely unroutable zone (a port strictly enclosed by an unrelated obstacle) alongside one healthy link.
- `crossZoneReservationRollbackFixture` — a link that reserves a segment crossing from an inner zone into an outer one, then fails once it reaches the outer zone (its target is unrelatedly enclosed there), alongside a second link confined to the inner zone whose cheapest route exactly ties with an alternate one, so it is only decided by whether the doomed link's inner-zone reservation was released.
- `assertOrthogonalPoints` — orthogonal/finite point-list assertion shared by the fixtures above.

## Cases

- Body-only signature stability; visible burst coalescing; hidden deferral.
- Active-job cancellation, latest-only execution, retry/hide/signature transitions.
- Independent pointer/keyboard edge traces and current-layout-only controls.
- Resolved-link graph derivation and deterministic nested ELK flattening.
- 20-link fan-in/fan-out independent lanes.
- Ten cross-folder links through deterministic gateways.
- Dense 60-document/240-link finite orthogonal obstacle-safe routing.
- Routes a link whose lead point lands on the floating-point boundary of its own inflated obstacle (regression for `Unable to route inside`).
- Keeps unrelated links routable and logs a diagnostic when one zone is genuinely unroutable, instead of throwing.
- A doomed cross-folder link's reservations in a zone it successfully crossed are rolled back once it fails in a later zone, so a later link sharing that zone routes identically to a baseline where the doomed link never existed.
- Self-links and duplicate relations remain independent.
- 100/200 and 500/1000 document/link performance gates.
- 240-document/duplicate-filename regression: hovering the rightmost (and thus most left-neighbored) document must, against the `oracleConnections` oracle, mark exactly the hovered document `'active'`, its true one-hop neighbors `'connected'`, and every other document — including everything positioned to its left and every repeated-name document — `'muted'`; also proves a leftover unrelated edge trace collides (masks the hovered document, wrongly activates an unrelated one) until `clearMapEdgeTraceForDocument` resets it, and that moving the trace to a second document clears the first.
- Edge trace left behind by pointer vs. keyboard focus: `clearMapEdgeTraceForDocument` resets either source identically, producing the same oracle-matching highlighted set.
- Live edge trace survives an overriding document hover: with an edge's `focusedEdgeId` genuinely set, calling the production `resolveTracedEdge(state,documentTraceActive,layoutEdges)` — the same function MapView's reactive `tracedEdge` calls, not a reimplementation — with `documentTraceActive: true` makes `nodeEmphasis`/`edgeEmphasis` resolve purely from the document while `edgeTraceState.focusedEdgeId` stays untouched; calling `resolveTracedEdge` again with `documentTraceActive: false` against that same untouched state resolves back to the original edge with no new trace event.
- Flow unmount resets a stale document trace via `resetMapTraceOnFlowUnmount`, restoring immediate edge tracing on remount with no flush-first document required.
- Per-node unmount via `clearMapDocumentTraceIfActive`: a still-active document's trace is cleared when its own node is destroyed (`onlyRenderVisibleElements` unmounting an individual virtualized node without an `onblur`/`onpointerleave`); a stale destroy for a document that is no longer the active trace (the user already moved on to a different document before the destroy callback ran) leaves the new, genuinely active trace untouched.

### Deterministic stress-fixture assertions (phase 1 of the routing/readability epic)

Every case below runs against all or specific fixtures in [`routing-fixtures.ts`](routing-fixtures.ts.md) — the same specs `scripts/generate-map-fixtures.mjs` renders to the on-disk debug workspaces under `test-fixtures/map-routing/`. They assert the invariants from the epic's phase-1 issue that already apply to the current router; invariants that only make sense once a later phase lands (ports, chevrons, typed corridor/lane objects, crossing markers) are `skip`-marked with a reason instead of deleted, per that issue.

- `assertRouteGeometry(edge)` — orthogonal, finite, and free of zero-length segments.
- `assertValidArrowAndPorts(layout, slug)` — every edge has an `arrowclosed` marker and a valid source/target side handle (`sideFromHandle`), and (`movesTowardSide`) the first and last points of `edge.data.points` actually move away from the source / into the target in the direction that handle side names — a real cross-check between the encoded side and the route's own geometry, not just presence.
- `segmentsCross(...)`, `countRouteCrossings(edges)` — proper (non-parallel, non-touching-endpoint) orthogonal segment crossing detector, used only to record a diagnostic baseline; a real crossing is never itself treated as a fault.
- 'routes every deterministic stress fixture identically twice with finite, orthogonal, interior-safe geometry' — for every fixture in `ROUTING_FIXTURES`: lays it out twice and asserts identical node positions and edge points (determinism), asserts `assertRouteGeometry`/`assertAvoidsDocumentInteriors`/`assertValidArrowAndPorts`, and asserts the whole fixture set stays under a 60s bounded-layout-time budget (reported via `context.diagnostic`).
- 'routes the cross-folder highway and nested fixtures through ordered, hierarchy-correct boundary gateways' — runs `assertFolderGatewayPolicy` (the existing hierarchy/gateway oracle) against `cross-folder-highway`, `nested-to-external`, and `unrelated-near-corridor`.
- 'keeps both directions of the bidirectional corridor as independent, non-merged edges' — 20 edges, exactly two per left/right pair, and no two edges share identical point geometry: proves `elk.layered.mergeEdges: false` holds for opposite-direction edges between the same pair.
- 'keeps inbound and outbound lanes on the mixed hub independently ordered within each direction' — the 8 outgoing and 8 incoming hub endpoints are each internally distinct (guaranteed by the current per-direction `portKey` grouping); also records, via `context.diagnostic` only, how many of the 16 endpoints stay distinct _across_ both directions combined — the current router ranks a side's incoming and outgoing ports in separate groups (`routing.ts` `portKey`), so an edge at the extreme of the hub's boundary can share a physical point with an edge of the opposite direction on the same side. This is the current-router baseline for the ports-phase invariant skipped below, not a hard pass/fail assertion yet.
- 'measures real geometric crossings on the crossing-heavy fixture as a baseline for the later crossing-marker phase' — asserts geometry/interior invariants, then reports `countRouteCrossings` via `context.diagnostic` as a baseline for the future crossing-marker phase.
- 'measures layout stability across the incremental-change fixture pair as a baseline for the later layout-feedback phase' — asserts `incremental-base` lays out identically across two runs (determinism), then lays out `incremental-next` (`incremental-base` plus one document and one link) and reports, via `context.diagnostic`, how many shared node IDs kept an identical position. The current router has no stability guarantee for a small local edit, so this is a measured metric, not an assertion.
- Four `skip`-marked pending cases, each naming the future phase and, where one exists, the current-router metric already recorded above that phase will need to satisfy fully: explicit per-endpoint port model (ports phase), chevron rendering (chevrons phase), crossing markers restricted to real crossings (crossing-marker phase, baseline above), and typed corridor/lane objects for multi-edge boundary crossings (corridors/gateways phase; `assertFolderGatewayPolicy` already covers boundary _ordering_ on the current router).

"Stale and cancelled worker results never reach the UI" is already covered generically (not fixture-specific) by the existing layout-session/request-queue cases above (`exposes edge controls only for the current successful layout`, `cancels active layout work and starts only the latest queued job`, etc.); no new stress-fixture case duplicates it.

Targets: all modules in this directory except viewport lifecycle.
