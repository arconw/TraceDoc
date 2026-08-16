# Orthogonal Router

Source: `src/lib/map/routing.ts`.

Deterministic obstacle-aware router over absolute compound geometry. Allocates four-sided document ports, folder-boundary gateways, independent lanes, and Manhattan paths.

## Contracts

- `MapSide`, `MapPoint`, `MapRect` — geometry primitives.
- `MapBoundaryGateway` — folder/side/point crossing record.
- `MapRoute` — points, endpoint sides, ordered gateways.
- `RoutableMapNode` — flow node geometry input.
- `RouteDescriptor`, `RankedMember`, `HeapEntry` — internal route/group/search records.

## Routing orchestration

- `routeMapLinks(graph, nodes)` — builds absolute geometry/descriptors, ranks port/gateway groups, routes each zone and returns edge-ID map.
- `segmentIntersectsRectInterior(source, target, rect)` — strict axis-aligned obstacle intersection predicate.
- `descriptorForLink(...)` — derives endpoint rectangles/sides, ancestor chains, LCA zones, and boundary sequence.
- `absoluteRectangles(nodes)` / nested `absolutePosition(id)` — flatten compound-relative node positions.
- `facingSides(source, target)` — chooses primary source/target sides.
- `boundaryChains(graph, sourceFolderId, targetFolderId)` — returns ordered ascent/descent gateway steps.
- `ancestors(graph, folderId)` — folder-to-root chain.
- `buildZoneRouters(...)` — creates one obstacle router per folder/root zone.

## Search structures

- `ZoneRouter.constructor(bounds, obstacles)` — initializes per-zone segment usage.
- `ZoneRouter.route(source, target)` — direct-route fast path, then visibility-grid Dijkstra with bend/shared-segment costs.
- `ZoneRouter.pointValid(point)` — bounds/obstacle test.
- `ZoneRouter.segmentClear(source, target)` — obstacle-interior test.
- `ZoneRouter.reserve(points)` — increments deterministic segment usage.
- `MinHeap.size` — queued entry count.
- `MinHeap.push(entry)` / `pop()` — stable binary min-heap for routing states.

## Lane and gateway allocation

- `buildPortGroups(descriptors)` / `buildGatewayGroups(descriptors)` — collect ranked members by physical region.
- `routeGateway(...)` — routes to/from one boundary point.
- `addRankedMember(...)`, `sortGroups(groups)`, `rankFor(groups,key,id)` — deterministic lane ordering.
- `portPoint(...)`, `gatewayPoint(...)`, `pointOnRect(...)` — convert rank to concrete border coordinate.
- `distributedOffset(...)` — spreads lanes within available span.
- `movePoint(point,side,distance)` — moves outward/inward by side.

## Geometry helpers

- `appendRoute`, `compactPoints`, `deduplicatePoints`, `pushPoint` — join and normalize polylines.
- `inflateRect`, `insetRect`, `pointInsideRect` — obstacle/container geometry.
- `uniqueNumbers` — sorted coordinate grid.
- `segmentKey`, `portKey`, `gatewayKey` — stable grouping/usage keys.
- `perpendicularCenter`, `centerX`, `centerY`, `oppositeSide` — side geometry.
- `entryBefore`, `compare` — deterministic heap/string ordering.

Invariants: all segments orthogonal/finite; unrelated document interiors are never crossed; duplicate/self links remain independent; boundary order follows hierarchy.
