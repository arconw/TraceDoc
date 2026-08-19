# Orthogonal Router

Source: `src/lib/map/routing.ts`.

Deterministic obstacle-aware router over absolute compound geometry. Allocates an explicit, structured `MapPort` per edge endpoint on the four document sides, folder-boundary gateways, independent lanes, and Manhattan paths.

## Contracts

- `MapSide`, `MapPoint`, `MapRect` — geometry primitives.
- `MapBoundaryGateway` — folder/side/point crossing record.
- `MapPort` — the explicit, structured per-edge port model: `id` (`<documentId>:<side>:<index>`, unique within that node/side), `documentId`, `linkId`, `direction` (`'source' | 'target'`), `side`, `index`/`count` (this port's rank and the total port count sharing that node/side), absolute `point`, and `offset` (0–1 fraction along the side, for CSS placement without recomputing geometry). One `MapPort` exists per edge endpoint; there is no reserved/unused port, so an idle node side with no edges has none.
- `MapChevron` / `MapChevronDirection` — one small intermediate direction marker: an absolute `point` on the route and the `'up' | 'down' | 'left' | 'right'` facing of the segment it sits on.
- `MapRoute` — points, endpoint sides, `sourcePort`/`targetPort`, ordered gateways, `chevrons` (repeated direction markers along this route), `crossingGaps` (points on this route's own path where it must render a small visual gap because another, unrelated route crosses it).
- `RoutableMapNode` — flow node geometry input.
- `RouteDescriptor`, `RankedMember`, `HeapEntry` — internal route/group/search records.
- `ZoneReservation` — a `ZoneRouter`/points pair recorded while one link's descriptor is being built, so a later failure in that same descriptor can undo exactly the reservations it made.
- `RouteUnavailableError` — thrown by `ZoneRouter.route` when a source/target pair has no path even after the relaxed-clearance retry; carries the zone `bounds` and the `source`/`target` points for diagnostics.

## Routing orchestration

- `routeMapLinks(graph, nodes)` — builds absolute geometry/descriptors, ranks port/gateway groups, routes each link through `routeDescriptor` (which also computes that route's own `chevrons`), then calls `attachCrossingGaps` once every route exists, and returns an edge-ID map. A link whose zone routing throws (boundary-ascent mismatch or `RouteUnavailableError`) is skipped and reported via `reportUnroutableLink` instead of aborting the remaining links — one impossible edge never blanks the whole map.
- `routeDescriptor(...)` — builds one link's full point sequence (ports, gateway crossings, zone-router segments), computes its `chevrons` from that final point list, and returns its `MapRoute` (with `crossingGaps` left as `[]` until `attachCrossingGaps` runs); throws on the first unroutable segment. A cross-folder link can route cleanly through one or more zones - each successful `ZoneRouter.route()` call along the way reserving its segments in that zone - before failing in a later one; every `route()` call is recorded as a `ZoneReservation` as it succeeds, and if the descriptor then throws, all of that link's reservations are undone via `releaseReservations` before the error propagates, so a link that will never be rendered never leaves other links penalized for segments only it ever used.
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
- `buildMapPort(documentId,linkId,direction,side,rect,rank,point)` — assembles one `MapPort` from a resolved rank/point; called once per endpoint by `routeDescriptor`.
- `sideOffset(rect,side,point)` — converts an absolute border point back to a 0–1 fraction of that side's length, clamped; feeds `MapPort.offset`.
- `portKey(documentId,side)` — the port congestion-group key: **only** `documentId`+`side`, deliberately not direction. `buildPortGroups` registers both a link's source-role and target-role membership under this key, so every edge touching one side of one node — whether it originates or terminates there — is ranked in a single shared sequence and gets a strictly distinct `index`/offset. This is what makes a high-degree node's combined incoming+outgoing boundary collision-free (see the `mixed-hub` fixture's 16 distinct endpoints): ranking source and target separately, as an earlier version of this router did, let an edge of one direction land on the exact point of an edge of the other direction whenever both independently chose the same side.

## Readability markers (chevrons and crossing gaps)

Pure, deterministic post-processing over already-routed geometry; neither step changes a single route's points, avoids a crossing, or reroutes anything - router preference for avoiding/rerouting a crossing is a later-phase concern (see Invariants below).

- `computeRouteChevrons(points)` — repeated intermediate direction markers along one route, independent of every other route. `routeLength` sums the route's own Manhattan segment lengths; below `CHEVRON_MIN_ROUTE_LENGTH` (96) the route renders none. Otherwise the usable span (`total - 2 * CHEVRON_END_MARGIN`, margin 24 on each end so markers never crowd a port or the final arrowhead) is divided into `count = clamp(floor(usable / CHEVRON_SPACING), 1, CHEVRON_MAX_COUNT)` evenly-spaced interior positions (spacing preference 64, hard cap 5) - this is what keeps a very long route's chevron count "restrained" rather than growing without bound. `pointAlongRoute` walks the point list by cumulative distance to place each chevron exactly on the segment it falls in and derive its `MapChevronDirection` (`chevronDirection`) from that segment's own axis and sign, so orientation is always correct on both horizontal and vertical segments, including across a multi-bend route.
- `attachCrossingGaps(routes)` / `computeCrossingGaps(routes)` / `recordRouteCrossings(...)` — after every link in the graph has a route, every pair of *different* edges' segment lists is checked for a real geometric crossing via `segmentCrossingPoint`, which requires one segment horizontal and the other vertical and the intersection strictly interior to both (the same `GEOMETRY_EPSILON`-tolerant interior test style as `segmentIntersectsRectInterior`) - a shared endpoint (a port, a corner, a gateway point) is therefore never mistaken for a crossing. Ownership of the resulting gap is a fixed convention, not a per-instance choice: the edge whose segment is horizontal at that crossing always owns the gap, so a genuine connection (two segments only ever meeting exactly at a shared endpoint) can never accidentally render one. Each owning route's gap points are recorded with their cumulative distance along that route (so multiple gaps on one route are already caller-ready in path order) and deduplicated by coordinate.
- Crossing vs. junction: TraceDoc's router never intentionally joins two different links' geometry at an interior point - every edge is independently routed end to end - so a "genuine junction" (the explicit-marker case the parent issue describes) cannot occur under the current router and is not modeled as a distinct type; only the non-connected crossing gap exists today. `crossingGaps` staying empty for every edge in a fixture with zero real crossings, and never containing a point that coincides with any route's own endpoint list, is the invariant that stands in for "no accidental junction marker."
- Router preference (avoid the crossing, reroute the edge, or nudge the layout, before ever rendering a gap) is explicitly out of scope for this phase: `attachCrossingGaps` only renders a gap for whatever crossing the existing router output already contains: it never changes routing to reduce crossings.

## Geometry helpers

- `appendRoute`, `compactPoints`, `deduplicatePoints`, `pushPoint` — join and normalize polylines.
- `inflateRect`, `insetRect`, `pointInsideRect` — obstacle/container geometry.
- `uniqueNumbers` — sorted coordinate grid.
- `segmentKey`, `gatewayKey` — stable grouping/usage keys (`portKey` is documented above, under lane allocation).
- `perpendicularCenter`, `centerX`, `centerY`, `oppositeSide` — side geometry.
- `entryBefore`, `compare` — deterministic heap/string ordering.

Invariants: all segments orthogonal/finite; unrelated document interiors are never crossed; duplicate/self links remain independent; boundary order follows hierarchy; every edge endpoint on a document resolves to a distinct `MapPort` — no two edges on the same node side, regardless of direction, ever share an `index`/point; a single genuinely unroutable link is skipped with a logged diagnostic rather than aborting `routeMapLinks` for the rest of the graph; a link that fails partway through a multi-zone route never leaves behind reservations for the zones it did successfully cross, so later links sharing those zones are never penalized or detoured for segments belonging to an edge that is never rendered; a route under `CHEVRON_MIN_ROUTE_LENGTH` always has zero `chevrons`, and every `MapChevron`'s `direction` always matches the axis/sign of the exact segment its `point` lies on; a `crossingGaps` point is always strictly interior to both crossing segments, so it never coincides with a `MapPort`, a gateway point, or any other route corner.
