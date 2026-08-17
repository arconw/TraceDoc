# Map Graph and Routing Tests

Source: `src/lib/map/map-graph.test.mjs`.

esbuild-loads production TypeScript modules and exercises pure state, ELK layout, routing geometry, determinism, and performance.

## Helpers and fixtures

- `loadTypeScript(relativePath)` — esbuild-based module loader.
- controllable layout job `start()` / `cancel()` — cancellation oracle.
- `nestedProject`, `syntheticProject`, `fanProject`, `document`, `link` — deterministic ProjectModel-shaped graph fixtures.
- `event` — edge-trace event builder.
- `assertOrthogonal`, `absoluteRectangles`, nested `positionFor`, `routeIntersectsRect`, `assertAvoidsDocumentInteriors` — geometry assertions.
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

Targets: all modules in this directory except viewport lifecycle.
