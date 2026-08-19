# Architecture Map

Pure graph projection, deterministic compound layout, obstacle-aware orthogonal routing, a bounded layout/routing quality feedback pass, interaction state, and viewport lifecycle.

- [`project-graph.ts`](project-graph.ts.md) — ProjectModel → resolved MapGraph adapter/signature.
- [`elk-layout.ts`](elk-layout.ts.md) — recursive ELK placement and Svelte Flow conversion, wrapped in a bounded, deterministic layout/routing feedback loop that escalates spacing, then degree-based hub model-order placement, when [`routing.ts`](routing.ts.md)'s routing-quality cost shows genuine structural defects.
- [`routing.ts`](routing.ts.md) — absolute geometry, ports, gateways, obstacles, lanes, shared corridors, gateway regions, and the routing-quality cost function the feedback loop scores each pass with.
- [`map-view-state.ts`](map-view-state.ts.md) — layout queue/session and edge trace reducers.
- [`viewport-lifecycle.ts`](viewport-lifecycle.ts.md) — capture/restore/fit decisions.
- [`routing-fixtures.ts`](routing-fixtures.ts.md) — deterministic stress-graph fixtures shared by the routing tests and the on-disk debug workspace generator.
- [`map-graph.test.mjs`](map-graph.test.mjs.md) — graph/layout/routing/state/performance regressions.
- [`viewport-lifecycle.test.mjs`](viewport-lifecycle.test.mjs.md) — viewport regressions.

`MapView.svelte` orchestrates these modules; none reads the filesystem.
