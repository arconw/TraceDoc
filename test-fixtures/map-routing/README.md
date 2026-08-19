# Map Routing Debug Workspaces

Deterministic, generated Markdown workspaces for manually exercising TraceDoc's architecture map router/layout. Each `<slug>/` folder below is a self-contained workspace: open it with `Ctrl/Cmd+O` (or the equivalent native "Open a documentation folder" action) like any other TraceDoc workspace, then switch to the architecture map (`Ctrl/Cmd+2`).

These are generated files. Do not hand-edit anything under this directory — edit the fixture definitions in [`src/lib/map/routing-fixtures.ts`](../../docs/src/lib/map/routing-fixtures.ts.md) instead and regenerate:

```sh
node scripts/generate-map-fixtures.mjs
```

The same fixture definitions back the automated stress-fixture assertions in [`src/lib/map/map-graph.test.mjs`](../../docs/src/lib/map/map-graph.test.mjs.md), so what you see here is exactly what those tests route and lay out.

## Fixtures

| Folder                                                 | Scenario                                                | Open and look for                                                                                                                                            |
| ------------------------------------------------------ | ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| [`fan-in/`](fan-in/)                                   | 20 sources → 1 target                                   | Every incoming edge keeps its own lane into the hub; none overlaps or crosses an unrelated document.                                                         |
| [`fan-out/`](fan-out/)                                 | 1 source → 20 targets                                   | Every outgoing edge keeps its own lane out of the hub; none overlaps or crosses an unrelated document.                                                       |
| [`fan-converge/`](fan-converge/)                       | 20 sources → 5 shared targets                           | Sources bound for the same target still land on distinct ports; the shared west corridor reads as one organized trunk before it fans out into the 5 targets. |
| [`dense-corridor/`](dense-corridor/)                   | 20 same-direction west→east links                       | All 20 links travel through the same corridor region between the two folders without visual tangling.                                                        |
| [`bidirectional-corridor/`](bidirectional-corridor/)   | 10 left→right + 10 right→left                           | Each of the 10 pairs shows two distinct edges, one per direction; they never draw as a single merged line.                                                   |
| [`cross-folder-highway/`](cross-folder-highway/)       | 16 frontend→backend links                               | Every edge crosses the `frontend`/`backend` boundary through a clean, ordered set of gateway points, not through folder interiors.                           |
| [`nested-to-external/`](nested-to-external/)           | 3-levels-deep source → similarly nested external target | The route ascends out of `frontend/components/widgets/` and descends into `integrations/external/` one boundary at a time, never cutting through a sibling.  |
| [`crossing-heavy/`](crossing-heavy/)                   | Reversed-order left/right pairing                       | Deliberately produces overlapping/crossing routes; the map must stay readable and every route must stay orthogonal and interior-safe regardless.             |
| [`mixed-hub/`](mixed-hub/)                             | 1 hub with 8 in + 8 out edges                           | Incoming and outgoing lanes share the hub's boundary; every one of the 16 endpoints now lands on its own explicit port, including the top/bottom corners.    |
| [`unrelated-near-corridor/`](unrelated-near-corridor/) | A 5-link corridor beside an unrelated 5-document chain  | The unrelated `nearby/` chain sits close to the corridor but its links never cross into `corridor/left` or `corridor/right`.                                 |
| [`incremental-base/`](incremental-base/)               | 12-document ring graph                                  | Baseline layout for the pair below.                                                                                                                          |
| [`incremental-next/`](incremental-next/)               | `incremental-base` + one document + one link            | Open side by side with `incremental-base` to see how much of the layout moves for one small, local graph edit on the current router.                         |

## Phase scope

Phase 1 established the deterministic fixture/metric baseline the architecture-map routing epic's later phases are judged against. Phase 2 (this set) adds the explicit document port model and merges each node side's incoming/outgoing lanes into one congestion-aware ranking, so a side's ports are always mutually distinct regardless of edge direction; `fan-converge` is its dedicated many-to-few stress fixture. Invariants that only make sense once a later phase lands (chevrons, corridors/gateways, crossing markers, layout feedback) are still recorded as `skip`-marked tests naming that phase in `map-graph.test.mjs`.
