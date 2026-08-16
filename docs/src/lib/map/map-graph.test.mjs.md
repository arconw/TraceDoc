# Map Graph and Routing Tests

Source: `src/lib/map/map-graph.test.mjs`.

Vite-loads production TypeScript modules and exercises pure state, ELK layout, routing geometry, determinism, and performance.

## Helpers and fixtures

- `loadTypeScript(relativePath)` — Vite SSR module loader.
- controllable layout job `start()` / `cancel()` — cancellation oracle.
- `nestedProject`, `syntheticProject`, `fanProject`, `document`, `link` — deterministic graph fixtures.
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

Targets: all modules in this directory except viewport lifecycle.
