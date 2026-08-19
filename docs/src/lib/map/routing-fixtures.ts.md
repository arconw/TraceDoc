# Routing Stress Fixtures

Source: `src/lib/map/routing-fixtures.ts`.

Deterministic `ProjectModel`-shaped stress graphs for the architecture map router/layout, and the single source of truth both `map-graph.test.mjs`'s stress-fixture assertions and `scripts/generate-map-fixtures.mjs`'s on-disk debug workspaces build from. Each fixture models a named real-world map scenario (fan-in, fan-out, a many-to-few fan-converge, corridors, cross-folder highway, nesting, crossing-heavy, a mixed hub, noise near a corridor, and an incremental-change pair) instead of a generic synthetic benchmark.

## Types

- `RoutingFixtureDocument` — one document's `path`, `title`, and outgoing `links` (relative target paths within the same fixture).
- `RoutingFixture` — `slug` (also the generated folder name), human `name`, one-sentence `invariant` describing the intended geometric/visual property, and its `documents`.

## Functions

- `buildFixtureProject(fixture)` — converts a fixture's flat document list into a full `ProjectModel`: infers the folder hierarchy from path segments (root folder id `folder:.`, path `''`), assigns deterministic `folder:<path>`/`document:<path>` IDs, sorts each folder's `childFolderIds`/`documentIds`, and resolves each declared link against the fixture's own document set (`resolved: false` with `unresolvedReason` set when a link's target path is not one of the fixture's documents, mirroring `DocumentLink`). This is the fixture-side analog of the ad hoc `nestedProject`/`syntheticProject`/`routableGraph` builders already local to `map-graph.test.mjs`; it is exported because the on-disk generator needs the same graph shape the tests do.
- `routingFixtureBySlug(slug)` — looks up one fixture by `slug`; throws for an unknown slug. Used by the generator script and available to any future fixture-scoped test.

## Fixtures (`ROUTING_FIXTURES`)

| Slug                      | Scenario                                                | Invariant under test                                                                                                   |
| ------------------------- | ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `fan-in`                  | 20 sources → 1 target                                   | Independent lanes and endpoints per edge; none crosses an unrelated document.                                          |
| `fan-out`                 | 1 source → 20 targets                                   | Independent lanes and endpoints per edge; none crosses an unrelated document.                                          |
| `fan-converge`            | 20 sources → 5 shared targets (4 sources each)          | Every source keeps a distinct port even when several share a target; the shared corridor reads as one organized trunk. |
| `dense-corridor`          | 20 same-direction west→east links                       | All 20 edges share one corridor region without losing orthogonality or an independent lane.                            |
| `bidirectional-corridor`  | 10 left→right + 10 right→left pairs                     | The two opposite edges between one pair never merge into a single logical edge.                                        |
| `cross-folder-highway`    | 16 frontend→backend links                               | Deterministic, hierarchy-ordered boundary gateways; no traversal of an unrelated folder.                               |
| `nested-to-external`      | 3-deep nested source → similarly nested external target | Boundary ascent/descent follows hierarchy order through every intermediate folder.                                     |
| `crossing-heavy`          | Reversed-order left/right pairing                       | Real geometric crossings are tolerated; orthogonality, finiteness, and interior-avoidance still hold.                  |
| `mixed-hub`               | 1 hub with 8 in + 8 out edges                           | Incoming and outgoing lanes on the same boundary stay independently ordered and collision-free.                        |
| `unrelated-near-corridor` | A 5-link corridor beside an unrelated link chain        | Neither graph's edges cross the other's document interiors.                                                            |
| `incremental-base`        | 12-document ring graph                                  | Deterministic across repeated runs; baseline for the next fixture.                                                     |
| `incremental-next`        | `incremental-base` + one document + one link            | Measures (rather than hard-asserts) how much of the layout changes for a small, local edit on the current router.      |

Consumers: [`map-graph.test.mjs`](map-graph.test.mjs.md) (stress-fixture assertions), `scripts/generate-map-fixtures.mjs` (writes the fixtures below as real Markdown workspaces under `test-fixtures/map-routing/<slug>/` — see that folder's `README.md`).
