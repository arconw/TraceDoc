# Orthogonal Router

Source: `src/lib/map/routing.ts`.

Deterministic obstacle-aware router over absolute compound geometry. Allocates four-sided document ports, folder-boundary gateways, independent lanes, and Manhattan paths.

## Contracts

- `MapSide`, `MapPoint`, `MapRect` — geometry primitives.
- `MapBoundaryGateway` — folder/side/point crossing record.
- `MapRoute` — points, endpoint sides, ordered gateways.
- `RoutableMapNode` — flow node geometry input.
- `RouteDescriptor`, `RankedMember`, `HeapEntry` — internal route/group/search records.
- `ZoneReservation` — a `ZoneRouter`/points pair recorded while one link's descriptor is being built, so a later failure in that same descriptor can undo exactly the reservations it made.
- `RouteUnavailableError` — thrown by `ZoneRouter.route` when a source/target pair has no path even after the relaxed-clearance retry; carries the zone `bounds` and the `source`/`target` points for diagnostics.

## Routing orchestration

- `routeMapLinks(graph, nodes)` — builds absolute geometry/descriptors, ranks port/gateway groups, routes each link through `routeDescriptor`, and returns an edge-ID map. A link whose zone routing throws (boundary-ascent mismatch or `RouteUnavailableError`) is skipped and reported via `reportUnroutableLink` instead of aborting the remaining links — one impossible edge never blanks the whole map.
- `routeDescriptor(...)` — builds one link's full point sequence (ports, gateway crossings, zone-router segments) and returns its `MapRoute`; throws on the first unroutable segment. A cross-folder link can route cleanly through one or more zones - each successful `ZoneRouter.route()` call along the way reserving its segments in that zone - before failing in a later one; every `route()` call is recorded as a `ZoneReservation` as it succeeds, and if the descriptor then throws, all of that link's reservations are undone via `releaseReservations` before the error propagates, so a link that will never be rendered never leaves other links penalized for segments only it ever used.
- `routeSegment(router, source, target, reservations)` — thin wrapper around `ZoneRouter.route` used by `routeDescriptor` that also appends the call's router/points pair to the in-flight `reservations` list.
- `releaseReservations(reservations)` — calls `ZoneRouter.release` for every recorded reservation; used by `routeDescriptor`'s catch block to undo a failed link's partial progress.
- `reportUnroutableLink(descriptor, error)` — logs a `console.warn` diagnostic naming the link ID, its source/target documents, and its LCA zone.
- `segmentIntersectsRectInterior(source, target, rect)` — axis-aligned obstacle intersection predicate, tolerant of boundary points within `GEOMETRY_EPSILON` of a rect edge.
- `descriptorForLink(...)` — derives endpoint rectangles/sides, ancestor chains, LCA zones, and boundary sequence.
- `absoluteRectangles(nodes)` / nested `absolutePosition(id)` — flatten compound-relative node positions.
- `facingSides(source, target)` — chooses primary source/target sides.
- `boundaryChains(graph, sourceFolderId, targetFolderId)` — returns ordered ascent/descent gateway steps.
- `ancestors(graph, folderId)` — folder-to-root chain.
- `buildZoneRouters(...)` — creates one obstacle router per folder/root zone.

## Search structures

- `ZoneRouter.constructor(bounds, obstacles)` — initializes per-zone segment usage.
- `ZoneRouter.route(source, target)` — direct-route fast path, then visibility-grid Dijkstra with bend/shared-segment costs against the full-clearance obstacles; on failure, retries once against the same obstacles inset by the clearance amount (trading buffer for connectivity in tight zones) before throwing `RouteUnavailableError`.
- `ZoneRouter.attemptGridRoute(source, target, obstacles)` — builds the visibility grid and runs Dijkstra for one obstacle set; returns `null` instead of throwing when the target is unreachable.
- `ZoneRouter.pointValid(point, obstacles)` — bounds/obstacle test; bounds are checked with `GEOMETRY_EPSILON` tolerance so accumulated floating-point drift never misclassifies a boundary point as outside the zone.
- `ZoneRouter.segmentClear(source, target, obstacles)` — obstacle-interior test.
- `ZoneRouter.reserve(points)` — increments deterministic segment usage.
- `ZoneRouter.release(points)` — decrements (and, at zero, removes) the segment usage `reserve` recorded for `points`; the exact inverse, called by `releaseReservations` to undo a failed link's already-successful hops through this zone.
- `MinHeap.size` — queued entry count.
- `MinHeap.push(entry)` / `pop()` — stable binary min-heap for routing states.

## Floating-point tolerance

Lead/gateway points are constructed by moving a port outward by the same clearance amount used to inflate that document's own obstacle rectangle, so in exact arithmetic they land exactly on the obstacle's boundary. The two values are computed through different addition orders (`(x - clearance) + (width + 2 * clearance)` for the obstacle edge vs. `(x + width) + clearance` for the point), which can disagree by up to a few ULPs for large accumulated coordinates. `pointInsideRect` and `segmentIntersectsRectInterior` therefore treat a point within `GEOMETRY_EPSILON` (`1e-6`, far below any real layout spacing) of a rect edge as on the boundary, not inside — this is the fix for `Unable to route inside` on large graphs, where deep compound-position summation produces exactly this class of boundary drift.

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

Invariants: all segments orthogonal/finite; unrelated document interiors are never crossed; duplicate/self links remain independent; boundary order follows hierarchy; a single genuinely unroutable link is skipped with a logged diagnostic rather than aborting `routeMapLinks` for the rest of the graph; a link that fails partway through a multi-zone route never leaves behind reservations for the zones it did successfully cross, so later links sharing those zones are never penalized or detoured for segments belonging to an edge that is never rendered.
