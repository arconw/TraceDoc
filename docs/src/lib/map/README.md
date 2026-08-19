# Architecture Map

Pure graph projection, deterministic compound layout, obstacle-aware orthogonal routing, interaction state, and viewport lifecycle.

- [`project-graph.ts`](project-graph.ts.md) — ProjectModel → resolved MapGraph adapter/signature.
- [`elk-layout.ts`](elk-layout.ts.md) — recursive ELK placement and Svelte Flow conversion.
- [`routing.ts`](routing.ts.md) — absolute geometry, ports, gateways, obstacles, lanes, shared corridors, and gateway regions.
- [`map-view-state.ts`](map-view-state.ts.md) — layout queue/session and edge trace reducers.
- [`viewport-lifecycle.ts`](viewport-lifecycle.ts.md) — capture/restore/fit decisions.
- [`routing-fixtures.ts`](routing-fixtures.ts.md) — deterministic stress-graph fixtures shared by the routing tests and the on-disk debug workspace generator.
- [`map-graph.test.mjs`](map-graph.test.mjs.md) — graph/layout/routing/state/performance regressions.
- [`viewport-lifecycle.test.mjs`](viewport-lifecycle.test.mjs.md) — viewport regressions.

`MapView.svelte` orchestrates these modules; none reads the filesystem.
