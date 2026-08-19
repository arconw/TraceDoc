import type { MapGraph, MapLink } from './project-graph';

export type MapSide = 'top' | 'right' | 'bottom' | 'left';

export interface MapPoint {
  x: number;
  y: number;
}

export interface MapBoundaryGateway {
  folderId: string;
  point: MapPoint;
  side: MapSide;
  regionId: string;
  laneIndex: number;
  laneCount: number;
}

export interface MapGatewayLane {
  linkId: string;
  laneIndex: number;
  point: MapPoint;
}

export interface MapGatewayRegion {
  id: string;
  folderId: string;
  side: MapSide;
  index: number;
  point: MapPoint;
  lanes: MapGatewayLane[];
}

export type MapCorridorAxis = 'horizontal' | 'vertical';

export interface MapCorridorLane {
  linkId: string;
  laneIndex: number;
  direction: MapChevronDirection;
}

export interface MapCorridor {
  id: string;
  axis: MapCorridorAxis;
  band: { start: number; end: number };
  extent: { start: number; end: number };
  lanes: MapCorridorLane[];
}

export interface MapCorridorAssignment {
  corridorId: string;
  axis: MapCorridorAxis;
  laneIndex: number;
  laneCount: number;
  direction: MapChevronDirection;
}

export interface MapPort {
  id: string;
  documentId: string;
  linkId: string;
  direction: 'source' | 'target';
  side: MapSide;
  index: number;
  count: number;
  point: MapPoint;
  offset: number;
}

export type MapChevronDirection = 'up' | 'down' | 'left' | 'right';

export interface MapChevron {
  point: MapPoint;
  direction: MapChevronDirection;
}

export interface MapRoute {
  points: MapPoint[];
  sourceSide: MapSide;
  targetSide: MapSide;
  sourcePort: MapPort;
  targetPort: MapPort;
  boundaryGateways: MapBoundaryGateway[];
  chevrons: MapChevron[];
  crossingGaps: MapPoint[];
  corridor: MapCorridorAssignment | null;
}

export interface RoutableMapNode {
  id: string;
  parentId?: string;
  position: MapPoint;
  width?: number;
  height?: number;
  data: { kind: 'folder' | 'document' };
}

export interface MapRect extends MapPoint {
  width: number;
  height: number;
}

interface RouteDescriptor {
  link: MapLink;
  source: MapRect;
  target: MapRect;
  sourceSide: MapSide;
  targetSide: MapSide;
  sourceBoundaries: string[];
  targetBoundaries: string[];
  lcaId: string;
}

interface RankedMember {
  id: string;
  order: number;
}

interface HeapEntry {
  state: number;
  cost: number;
}

const PORT_INSET = 10;
const PORT_SPACING = 8;
const GATEWAY_SPACING = 10;
const ROUTE_CLEARANCE = 8;
const FOLDER_INSET = 4;
const BEND_COST = 20;
const SHARED_SEGMENT_COST = 12;
const GEOMETRY_EPSILON = 1e-6;
const CHEVRON_MIN_ROUTE_LENGTH = 96;
const CHEVRON_END_MARGIN = 24;
const CHEVRON_SPACING = 64;
const CHEVRON_MAX_COUNT = 5;
const GATEWAY_REGION_GAP = 40;
const CORRIDOR_MIN_MEMBERS = 3;
const CORRIDOR_LANE_GAP = 24;
const CORRIDOR_MIN_OVERLAP = 32;
const CORRIDOR_MIN_SEGMENT_LENGTH = 24;
const CORRIDOR_LANE_OFFSET = 4;

export class RouteUnavailableError extends Error {
  constructor(
    readonly bounds: MapRect,
    readonly source: MapPoint,
    readonly target: MapPoint,
  ) {
    super(
      `Unable to route inside ${bounds.x},${bounds.y},${bounds.width},${bounds.height}`,
    );
    this.name = 'RouteUnavailableError';
  }
}

export function routeMapLinks(
  graph: MapGraph,
  nodes: RoutableMapNode[],
): Record<string, MapRoute> {
  const rectangles = absoluteRectangles(nodes);
  const descriptors = graph.links
    .map((link) => descriptorForLink(graph, rectangles, link))
    .filter((descriptor): descriptor is RouteDescriptor => descriptor !== null);
  const portGroups = buildPortGroups(descriptors);
  const gatewayGroups = buildGatewayGroups(descriptors);
  const zoneRouters = buildZoneRouters(graph, rectangles);
  const routes: Record<string, MapRoute> = {};

  for (const descriptor of descriptors) {
    try {
      routes[descriptor.link.id] = routeDescriptor(
        graph,
        rectangles,
        zoneRouters,
        portGroups,
        gatewayGroups,
        descriptor,
      );
    } catch (error) {
      reportUnroutableLink(descriptor, error);
    }
  }

  const allRects = Object.values(rectangles);

  attachGatewayRegions(routes);
  attachRoutingCorridors(routes, allRects);
  for (const route of Object.values(routes)) {
    route.chevrons = computeRouteChevrons(route.points);
  }
  attachCrossingGaps(routes);

  return routes;
}

function routeDescriptor(
  graph: MapGraph,
  rectangles: Record<string, MapRect>,
  zoneRouters: Record<string, ZoneRouter>,
  portGroups: Map<string, RankedMember[]>,
  gatewayGroups: Map<string, RankedMember[]>,
  descriptor: RouteDescriptor,
): MapRoute {
  const sourceRank = rankFor(
    portGroups,
    portKey(descriptor.link.sourceDocumentId, descriptor.sourceSide),
    descriptor.link.id,
  );
  const sourcePoint = portPoint(
    descriptor.source,
    descriptor.sourceSide,
    sourceRank,
  );
  const sourcePort = buildMapPort(
    descriptor.link.sourceDocumentId,
    descriptor.link.id,
    'source',
    descriptor.sourceSide,
    descriptor.source,
    sourceRank,
    sourcePoint,
  );
  const targetRank = rankFor(
    portGroups,
    portKey(descriptor.link.targetDocumentId, descriptor.targetSide),
    descriptor.link.id,
  );
  const targetPoint = portPoint(
    descriptor.target,
    descriptor.targetSide,
    targetRank,
  );
  const targetPort = buildMapPort(
    descriptor.link.targetDocumentId,
    descriptor.link.id,
    'target',
    descriptor.targetSide,
    descriptor.target,
    targetRank,
    targetPoint,
  );
  const points = [sourcePoint];
  const boundaryGateways: MapBoundaryGateway[] = [];
  const reservations: ZoneReservation[] = [];
  let current = movePoint(sourcePoint, descriptor.sourceSide, ROUTE_CLEARANCE);
  let currentZone = graph.documents[descriptor.link.sourceDocumentId].parentId;
  pushPoint(points, current);

  try {
    for (const folderId of descriptor.sourceBoundaries) {
      const gateway = routeGateway(
        rectangles[folderId],
        folderId,
        descriptor.sourceSide,
        gatewayGroups,
        descriptor.link.id,
      );
      const inside = movePoint(
        gateway.point,
        oppositeSide(gateway.side),
        ROUTE_CLEARANCE,
      );
      appendRoute(
        points,
        routeSegment(zoneRouters[currentZone], current, inside, reservations),
      );
      pushPoint(points, gateway.point);
      current = movePoint(gateway.point, gateway.side, ROUTE_CLEARANCE);
      pushPoint(points, current);
      boundaryGateways.push(gateway);
      currentZone = graph.folders[folderId].parentId!;
    }

    if (currentZone !== descriptor.lcaId) {
      throw new Error(`Unable to ascend route ${descriptor.link.id}`);
    }

    for (const folderId of [...descriptor.targetBoundaries].reverse()) {
      const gateway = routeGateway(
        rectangles[folderId],
        folderId,
        descriptor.targetSide,
        gatewayGroups,
        descriptor.link.id,
      );
      const outside = movePoint(gateway.point, gateway.side, ROUTE_CLEARANCE);
      appendRoute(
        points,
        routeSegment(zoneRouters[currentZone], current, outside, reservations),
      );
      pushPoint(points, gateway.point);
      current = movePoint(
        gateway.point,
        oppositeSide(gateway.side),
        ROUTE_CLEARANCE,
      );
      pushPoint(points, current);
      boundaryGateways.push(gateway);
      currentZone = folderId;
    }

    const targetLead = movePoint(
      targetPoint,
      descriptor.targetSide,
      ROUTE_CLEARANCE,
    );
    appendRoute(
      points,
      routeSegment(zoneRouters[currentZone], current, targetLead, reservations),
    );
    pushPoint(points, targetPoint);
  } catch (error) {
    // A link that routed cleanly through one or more zones before failing
    // in a later one must not leave those earlier zones' reservations
    // behind: they belong to an edge that will never be rendered, and
    // would otherwise bias/penalize every other link that later shares
    // those segments. Only a link whose descriptor completes in full keeps
    // its reservations.
    releaseReservations(reservations);
    throw error;
  }

  const finalPoints = deduplicatePoints(points);

  return {
    points: finalPoints,
    sourceSide: descriptor.sourceSide,
    targetSide: descriptor.targetSide,
    sourcePort,
    targetPort,
    boundaryGateways,
    chevrons: [],
    crossingGaps: [],
    corridor: null,
  };
}

interface ZoneReservation {
  router: ZoneRouter;
  points: MapPoint[];
}

function routeSegment(
  router: ZoneRouter,
  source: MapPoint,
  target: MapPoint,
  reservations: ZoneReservation[],
): MapPoint[] {
  const points = router.route(source, target);
  reservations.push({ router, points });
  return points;
}

function releaseReservations(reservations: ZoneReservation[]) {
  for (const { router, points } of reservations) router.release(points);
}

function reportUnroutableLink(descriptor: RouteDescriptor, error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  console.warn(
    `[map/routing] skipping unroutable link ${descriptor.link.id} (${descriptor.link.sourceDocumentId} -> ${descriptor.link.targetDocumentId}, lca ${descriptor.lcaId}): ${message}`,
  );
}

function manhattanDistance(source: MapPoint, target: MapPoint) {
  return Math.abs(target.x - source.x) + Math.abs(target.y - source.y);
}

function routeLength(points: MapPoint[]) {
  let total = 0;
  for (let index = 1; index < points.length; index += 1) {
    total += manhattanDistance(points[index - 1], points[index]);
  }
  return total;
}

function chevronDirection(from: MapPoint, to: MapPoint): MapChevronDirection {
  if (from.x === to.x) return to.y > from.y ? 'down' : 'up';
  return to.x > from.x ? 'right' : 'left';
}

function pointAlongRoute(points: MapPoint[], distance: number): MapChevron {
  let remaining = distance;
  for (let index = 1; index < points.length; index += 1) {
    const from = points[index - 1];
    const to = points[index];
    const length = manhattanDistance(from, to);
    if (length <= 0) continue;
    if (remaining <= length) {
      const fraction = remaining / length;
      return {
        point: {
          x: from.x + (to.x - from.x) * fraction,
          y: from.y + (to.y - from.y) * fraction,
        },
        direction: chevronDirection(from, to),
      };
    }
    remaining -= length;
  }
  const last = points.at(-1)!;
  const beforeLast = points.at(-2) ?? last;
  return { point: last, direction: chevronDirection(beforeLast, last) };
}

function computeRouteChevrons(points: MapPoint[]): MapChevron[] {
  const total = routeLength(points);
  if (total < CHEVRON_MIN_ROUTE_LENGTH) return [];
  const usable = total - CHEVRON_END_MARGIN * 2;
  if (usable <= 0) return [];
  const count = Math.min(
    CHEVRON_MAX_COUNT,
    Math.max(1, Math.floor(usable / CHEVRON_SPACING)),
  );
  const chevrons: MapChevron[] = [];
  for (let index = 0; index < count; index += 1) {
    const distance = CHEVRON_END_MARGIN + (usable * (index + 1)) / (count + 1);
    chevrons.push(pointAlongRoute(points, distance));
  }
  return chevrons;
}

function attachCrossingGaps(routes: Record<string, MapRoute>) {
  const gapsByLink = computeCrossingGaps(routes);
  for (const [linkId, route] of Object.entries(routes)) {
    route.crossingGaps = gapsByLink.get(linkId) ?? [];
  }
}

function computeCrossingGaps(routes: Record<string, MapRoute>) {
  const linkIds = Object.keys(routes);
  const gaps = new Map<string, { point: MapPoint; distance: number }[]>();

  for (let first = 0; first < linkIds.length; first += 1) {
    for (let second = first + 1; second < linkIds.length; second += 1) {
      recordRouteCrossings(routes, linkIds[first], linkIds[second], gaps);
    }
  }

  const result = new Map<string, MapPoint[]>();
  for (const [linkId, entries] of gaps) {
    entries.sort((left, right) => left.distance - right.distance);
    result.set(linkId, distinctPoints(entries.map((entry) => entry.point)));
  }
  return result;
}

function recordRouteCrossings(
  routes: Record<string, MapRoute>,
  firstLinkId: string,
  secondLinkId: string,
  gaps: Map<string, { point: MapPoint; distance: number }[]>,
) {
  const firstPoints = routes[firstLinkId].points;
  const secondPoints = routes[secondLinkId].points;
  let firstDistance = 0;

  for (let firstIndex = 1; firstIndex < firstPoints.length; firstIndex += 1) {
    const firstStart = firstPoints[firstIndex - 1];
    const firstEnd = firstPoints[firstIndex];
    let secondDistance = 0;

    for (
      let secondIndex = 1;
      secondIndex < secondPoints.length;
      secondIndex += 1
    ) {
      const secondStart = secondPoints[secondIndex - 1];
      const secondEnd = secondPoints[secondIndex];
      const crossing = segmentCrossingPoint(
        firstStart,
        firstEnd,
        secondStart,
        secondEnd,
      );
      if (crossing) {
        const ownerLinkId = crossing.firstIsHorizontal
          ? firstLinkId
          : secondLinkId;
        const distance = crossing.firstIsHorizontal
          ? firstDistance + manhattanDistance(firstStart, crossing.point)
          : secondDistance + manhattanDistance(secondStart, crossing.point);
        const entries = gaps.get(ownerLinkId) ?? [];
        entries.push({ point: crossing.point, distance });
        gaps.set(ownerLinkId, entries);
      }
      secondDistance += manhattanDistance(secondStart, secondEnd);
    }
    firstDistance += manhattanDistance(firstStart, firstEnd);
  }
}

function segmentCrossingPoint(
  firstStart: MapPoint,
  firstEnd: MapPoint,
  secondStart: MapPoint,
  secondEnd: MapPoint,
) {
  const firstIsHorizontal = firstStart.y === firstEnd.y;
  const secondIsHorizontal = secondStart.y === secondEnd.y;
  if (firstIsHorizontal === secondIsHorizontal) return null;

  const horizontal = firstIsHorizontal
    ? { y: firstStart.y, start: firstStart.x, end: firstEnd.x }
    : { y: secondStart.y, start: secondStart.x, end: secondEnd.x };
  const vertical = firstIsHorizontal
    ? { x: secondStart.x, start: secondStart.y, end: secondEnd.y }
    : { x: firstStart.x, start: firstStart.y, end: firstEnd.y };
  const horizontalMin = Math.min(horizontal.start, horizontal.end);
  const horizontalMax = Math.max(horizontal.start, horizontal.end);
  const verticalMin = Math.min(vertical.start, vertical.end);
  const verticalMax = Math.max(vertical.start, vertical.end);

  if (
    vertical.x <= horizontalMin + GEOMETRY_EPSILON ||
    vertical.x >= horizontalMax - GEOMETRY_EPSILON ||
    horizontal.y <= verticalMin + GEOMETRY_EPSILON ||
    horizontal.y >= verticalMax - GEOMETRY_EPSILON
  ) {
    return null;
  }

  return {
    point: { x: vertical.x, y: horizontal.y },
    firstIsHorizontal,
  };
}

function distinctPoints(points: MapPoint[]) {
  const seen = new Set<string>();
  const result: MapPoint[] = [];
  for (const point of points) {
    const key = `${point.x},${point.y}`;
    if (seen.has(key)) continue;
    seen.add(key);
    result.push(point);
  }
  return result;
}

interface GatewayCrossingMember {
  linkId: string;
  folderId: string;
  side: MapSide;
  point: MapPoint;
  coordinate: number;
}

export function computeGatewayRegions(
  routes: Record<string, MapRoute>,
): MapGatewayRegion[] {
  const byKey = new Map<string, GatewayCrossingMember[]>();

  for (const [linkId, route] of Object.entries(routes)) {
    for (const gateway of route.boundaryGateways) {
      const key = gatewayKey(gateway.folderId, gateway.side);
      const members = byKey.get(key) ?? [];
      members.push({
        linkId,
        folderId: gateway.folderId,
        side: gateway.side,
        point: gateway.point,
        coordinate: perpendicularCoordinate(gateway.side, gateway.point),
      });
      byKey.set(key, members);
    }
  }

  const regions: MapGatewayRegion[] = [];

  for (const key of [...byKey.keys()].sort(compare)) {
    const members = [...byKey.get(key)!].sort(
      (left, right) =>
        left.coordinate - right.coordinate ||
        compare(left.linkId, right.linkId),
    );
    let cluster: GatewayCrossingMember[] = [];
    let regionIndex = 0;

    const flush = () => {
      if (cluster.length === 0) return;
      regions.push(buildGatewayRegion(cluster, regionIndex));
      regionIndex += 1;
      cluster = [];
    };

    for (const member of members) {
      const previous = cluster.at(-1);
      if (
        previous &&
        member.coordinate - previous.coordinate > GATEWAY_REGION_GAP
      ) {
        flush();
      }
      cluster.push(member);
    }
    flush();
  }

  return regions;
}

function buildGatewayRegion(
  members: GatewayCrossingMember[],
  index: number,
): MapGatewayRegion {
  const { folderId, side } = members[0];
  const lanes: MapGatewayLane[] = members.map((member, laneIndex) => ({
    linkId: member.linkId,
    laneIndex,
    point: member.point,
  }));
  const centroid =
    members.reduce((sum, member) => sum + member.coordinate, 0) /
    members.length;
  const point =
    side === 'left' || side === 'right'
      ? { x: members[0].point.x, y: centroid }
      : { x: centroid, y: members[0].point.y };

  return {
    id: `${folderId}:${side}:${index}`,
    folderId,
    side,
    index,
    point,
    lanes,
  };
}

function attachGatewayRegions(routes: Record<string, MapRoute>) {
  const regions = computeGatewayRegions(routes);
  const byMember = new Map<
    string,
    { regionId: string; laneIndex: number; laneCount: number }
  >();

  for (const region of regions) {
    for (const lane of region.lanes) {
      byMember.set(`${region.folderId}:${region.side}:${lane.linkId}`, {
        regionId: region.id,
        laneIndex: lane.laneIndex,
        laneCount: region.lanes.length,
      });
    }
  }

  for (const [linkId, route] of Object.entries(routes)) {
    for (const gateway of route.boundaryGateways) {
      const info = byMember.get(
        `${gateway.folderId}:${gateway.side}:${linkId}`,
      );
      if (!info) continue;
      gateway.regionId = info.regionId;
      gateway.laneIndex = info.laneIndex;
      gateway.laneCount = info.laneCount;
    }
  }
}

function pointsEqual(first: MapPoint, second: MapPoint) {
  return first.x === second.x && first.y === second.y;
}

function segmentAvoidsInteriors(
  source: MapPoint,
  target: MapPoint,
  obstacles: MapRect[],
) {
  return !obstacles.some((rect) =>
    segmentIntersectsRectInterior(source, target, rect),
  );
}

function perpendicularCoordinate(side: MapSide, point: MapPoint) {
  return side === 'left' || side === 'right' ? point.y : point.x;
}

interface CorridorCandidate {
  linkId: string;
  axis: MapCorridorAxis;
  perpendicular: number;
  start: number;
  end: number;
  direction: MapChevronDirection;
  from: MapPoint;
  to: MapPoint;
}

interface BuiltCorridor {
  corridor: MapCorridor;
  segments: Map<string, CorridorCandidate>;
}

export function computeRoutingCorridors(
  routes: Record<string, MapRoute>,
): MapCorridor[] {
  return computeBuiltCorridors(routes).map(({ corridor }) => corridor);
}

function computeBuiltCorridors(
  routes: Record<string, MapRoute>,
): BuiltCorridor[] {
  const candidates = collectCorridorCandidates(routes);
  const clusters = clusterCorridorCandidates(candidates).filter(
    (cluster) =>
      new Set(cluster.map((candidate) => candidate.linkId)).size >=
      CORRIDOR_MIN_MEMBERS,
  );
  const built = clusters.map((cluster) => buildCorridor(cluster));

  built.sort(
    (left, right) =>
      (left.corridor.axis === right.corridor.axis
        ? 0
        : left.corridor.axis < right.corridor.axis
          ? -1
          : 1) ||
      left.corridor.extent.start - right.corridor.extent.start ||
      left.corridor.band.start - right.corridor.band.start ||
      left.corridor.band.end - right.corridor.band.end,
  );

  return built.map(({ corridor, segments }, index) => ({
    corridor: { ...corridor, id: `corridor:${corridor.axis}:${index}` },
    segments,
  }));
}

function collectCorridorCandidates(
  routes: Record<string, MapRoute>,
): CorridorCandidate[] {
  const candidates: CorridorCandidate[] = [];

  for (const [linkId, route] of Object.entries(routes)) {
    for (let index = 1; index < route.points.length; index += 1) {
      const from = route.points[index - 1];
      const to = route.points[index];
      if (from.x === to.x && from.y === to.y) continue;
      const axis: MapCorridorAxis = from.y === to.y ? 'horizontal' : 'vertical';
      const start =
        axis === 'horizontal' ? Math.min(from.x, to.x) : Math.min(from.y, to.y);
      const end =
        axis === 'horizontal' ? Math.max(from.x, to.x) : Math.max(from.y, to.y);
      if (end - start < CORRIDOR_MIN_SEGMENT_LENGTH) continue;
      candidates.push({
        linkId,
        axis,
        perpendicular: axis === 'horizontal' ? from.y : from.x,
        start,
        end,
        direction: chevronDirection(from, to),
        from: { x: from.x, y: from.y },
        to: { x: to.x, y: to.y },
      });
    }
  }

  return candidates;
}

function corridorCandidatesAdjacent(
  left: CorridorCandidate,
  right: CorridorCandidate,
) {
  if (left.axis !== right.axis) return false;
  if (Math.abs(left.perpendicular - right.perpendicular) > CORRIDOR_LANE_GAP) {
    return false;
  }
  const overlap =
    Math.min(left.end, right.end) - Math.max(left.start, right.start);
  return overlap >= CORRIDOR_MIN_OVERLAP;
}

function clusterCorridorCandidates(
  candidates: CorridorCandidate[],
): CorridorCandidate[][] {
  const parent = candidates.map((_, index) => index);

  function find(index: number): number {
    while (parent[index] !== index) {
      parent[index] = parent[parent[index]];
      index = parent[index];
    }
    return index;
  }

  function union(left: number, right: number) {
    const rootLeft = find(left);
    const rootRight = find(right);
    if (rootLeft === rootRight) return;
    parent[Math.max(rootLeft, rootRight)] = Math.min(rootLeft, rootRight);
  }

  for (let first = 0; first < candidates.length; first += 1) {
    for (let second = first + 1; second < candidates.length; second += 1) {
      if (candidates[first].linkId === candidates[second].linkId) continue;
      if (!corridorCandidatesAdjacent(candidates[first], candidates[second])) {
        continue;
      }
      union(first, second);
    }
  }

  const groups = new Map<number, CorridorCandidate[]>();
  for (let index = 0; index < candidates.length; index += 1) {
    const root = find(index);
    const group = groups.get(root) ?? [];
    group.push(candidates[index]);
    groups.set(root, group);
  }

  return [...groups.entries()]
    .sort((left, right) => left[0] - right[0])
    .map(([, group]) => group);
}

function buildCorridor(candidates: CorridorCandidate[]): BuiltCorridor {
  const byLink = new Map<string, CorridorCandidate>();
  for (const candidate of candidates) {
    const existing = byLink.get(candidate.linkId);
    if (
      !existing ||
      candidate.end - candidate.start > existing.end - existing.start
    ) {
      byLink.set(candidate.linkId, candidate);
    }
  }

  const members = [...byLink.values()].sort(
    (left, right) =>
      left.perpendicular - right.perpendicular ||
      compare(left.linkId, right.linkId),
  );
  const lanes: MapCorridorLane[] = members.map((member, laneIndex) => ({
    linkId: member.linkId,
    laneIndex,
    direction: member.direction,
  }));

  return {
    corridor: {
      id: '',
      axis: members[0].axis,
      band: {
        start: Math.min(...members.map((member) => member.perpendicular)),
        end: Math.max(...members.map((member) => member.perpendicular)),
      },
      extent: {
        start: Math.min(...members.map((member) => member.start)),
        end: Math.max(...members.map((member) => member.end)),
      },
      lanes,
    },
    segments: byLink,
  };
}

function attachRoutingCorridors(
  routes: Record<string, MapRoute>,
  allRects: MapRect[],
) {
  const built = computeBuiltCorridors(routes);
  const offsetApplied = new Set<string>();
  for (const { corridor, segments } of built) {
    for (const lane of corridor.lanes) {
      const route = routes[lane.linkId];
      if (!route) continue;
      route.corridor = {
        corridorId: corridor.id,
        axis: corridor.axis,
        laneIndex: lane.laneIndex,
        laneCount: corridor.lanes.length,
        direction: lane.direction,
      };
      const segment = segments.get(lane.linkId);
      if (segment && !offsetApplied.has(lane.linkId)) {
        const applied = applyCorridorLaneOffset(
          route,
          lane.linkId,
          corridor,
          lane,
          segment,
          allRects,
          routes,
        );
        if (applied) offsetApplied.add(lane.linkId);
      }
    }
  }
}

function foreignObstacles(
  allRects: MapRect[],
  from: MapPoint,
  to: MapPoint,
): MapRect[] {
  return allRects.filter(
    (rect) =>
      !(containsPointInclusive(rect, from) && containsPointInclusive(rect, to)),
  );
}

function containsPointInclusive(rect: MapRect, point: MapPoint) {
  return (
    point.x >= rect.x &&
    point.x <= rect.x + rect.width &&
    point.y >= rect.y &&
    point.y <= rect.y + rect.height
  );
}

function applyCorridorLaneOffset(
  route: MapRoute,
  linkId: string,
  corridor: MapCorridor,
  lane: MapCorridorLane,
  segment: CorridorCandidate,
  allRects: MapRect[],
  routes: Record<string, MapRoute>,
): boolean {
  const laneCount = corridor.lanes.length;
  const delta = (lane.laneIndex - (laneCount - 1) / 2) * CORRIDOR_LANE_OFFSET;
  if (delta === 0) return false;

  const points = route.points;
  const index = findSegmentIndex(points, segment.from, segment.to);
  if (index < 2 || index > points.length - 2) return false;

  const prev = points[index - 2];
  const next = points[index + 1];
  if (
    route.boundaryGateways.some(
      (gateway) =>
        pointsEqual(gateway.point, prev) || pointsEqual(gateway.point, next),
    )
  ) {
    return false;
  }

  const shift = (point: MapPoint): MapPoint =>
    corridor.axis === 'horizontal'
      ? { x: point.x, y: point.y + delta }
      : { x: point.x + delta, y: point.y };

  const newFrom = shift(points[index - 1]);
  const newTo = shift(points[index]);

  if (!segmentAligned(prev, newFrom) || !segmentAligned(newTo, next)) {
    return false;
  }
  if (pointsEqual(prev, newFrom) || pointsEqual(newTo, next)) return false;

  const obstacles = foreignObstacles(allRects, segment.from, segment.to);
  const segments: [MapPoint, MapPoint][] = [
    [prev, newFrom],
    [newFrom, newTo],
    [newTo, next],
  ];
  if (
    !segments.every(([from, to]) => segmentAvoidsInteriors(from, to, obstacles))
  ) {
    return false;
  }
  if (
    segments.some(([from, to]) =>
      introducesNewCrossing(from, to, linkId, routes),
    )
  ) {
    return false;
  }

  points[index - 1] = newFrom;
  points[index] = newTo;
  return true;
}

function introducesNewCrossing(
  from: MapPoint,
  to: MapPoint,
  ownLinkId: string,
  routes: Record<string, MapRoute>,
) {
  for (const [otherLinkId, otherRoute] of Object.entries(routes)) {
    if (otherLinkId === ownLinkId) continue;
    const otherPoints = otherRoute.points;
    for (let index = 1; index < otherPoints.length; index += 1) {
      if (
        segmentCrossingPoint(
          from,
          to,
          otherPoints[index - 1],
          otherPoints[index],
        )
      ) {
        return true;
      }
    }
  }
  return false;
}

function segmentAligned(first: MapPoint, second: MapPoint) {
  return first.x === second.x || first.y === second.y;
}

function findSegmentIndex(points: MapPoint[], from: MapPoint, to: MapPoint) {
  for (let index = 1; index < points.length; index += 1) {
    if (
      pointsEqual(points[index - 1], from) &&
      pointsEqual(points[index], to)
    ) {
      return index;
    }
  }
  return -1;
}

export function segmentIntersectsRectInterior(
  source: MapPoint,
  target: MapPoint,
  rect: MapRect,
) {
  if (source.x === target.x) {
    if (
      source.x <= rect.x + GEOMETRY_EPSILON ||
      source.x >= rect.x + rect.width - GEOMETRY_EPSILON
    ) {
      return false;
    }
    const minimum = Math.min(source.y, target.y);
    const maximum = Math.max(source.y, target.y);
    return (
      maximum > rect.y + GEOMETRY_EPSILON &&
      minimum < rect.y + rect.height - GEOMETRY_EPSILON
    );
  }
  if (source.y === target.y) {
    if (
      source.y <= rect.y + GEOMETRY_EPSILON ||
      source.y >= rect.y + rect.height - GEOMETRY_EPSILON
    ) {
      return false;
    }
    const minimum = Math.min(source.x, target.x);
    const maximum = Math.max(source.x, target.x);
    return (
      maximum > rect.x + GEOMETRY_EPSILON &&
      minimum < rect.x + rect.width - GEOMETRY_EPSILON
    );
  }
  return true;
}

function descriptorForLink(
  graph: MapGraph,
  rectangles: Record<string, MapRect>,
  link: MapLink,
): RouteDescriptor | null {
  const source = rectangles[link.sourceDocumentId];
  const target = rectangles[link.targetDocumentId];
  const sourceDocument = graph.documents[link.sourceDocumentId];
  const targetDocument = graph.documents[link.targetDocumentId];
  if (!source || !target || !sourceDocument || !targetDocument) return null;
  const sides =
    link.sourceDocumentId === link.targetDocumentId
      ? ({ source: 'right', target: 'bottom' } as const)
      : facingSides(source, target);
  const boundaries = boundaryChains(
    graph,
    sourceDocument.parentId,
    targetDocument.parentId,
  );

  return {
    link,
    source,
    target,
    sourceSide: sides.source,
    targetSide: sides.target,
    sourceBoundaries: boundaries.source,
    targetBoundaries: boundaries.target,
    lcaId: boundaries.lcaId,
  };
}

function absoluteRectangles(nodes: RoutableMapNode[]) {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const positions = new Map<string, MapPoint>();

  function absolutePosition(id: string): MapPoint {
    const cached = positions.get(id);
    if (cached) return cached;
    const node = byId.get(id);
    if (!node) return { x: 0, y: 0 };
    const parent = node.parentId
      ? absolutePosition(node.parentId)
      : { x: 0, y: 0 };
    const position = {
      x: parent.x + node.position.x,
      y: parent.y + node.position.y,
    };
    positions.set(id, position);
    return position;
  }

  return nodes.reduce<Record<string, MapRect>>((result, node) => {
    result[node.id] = {
      ...absolutePosition(node.id),
      width: node.width ?? 0,
      height: node.height ?? 0,
    };
    return result;
  }, {});
}

function facingSides(source: MapRect, target: MapRect) {
  const deltaX = centerX(target) - centerX(source);
  const deltaY = centerY(target) - centerY(source);
  if (Math.abs(deltaX) >= Math.abs(deltaY)) {
    return deltaX >= 0
      ? ({ source: 'right', target: 'left' } as const)
      : ({ source: 'left', target: 'right' } as const);
  }
  return deltaY >= 0
    ? ({ source: 'bottom', target: 'top' } as const)
    : ({ source: 'top', target: 'bottom' } as const);
}

function boundaryChains(
  graph: MapGraph,
  sourceFolderId: string,
  targetFolderId: string,
) {
  const source = ancestors(graph, sourceFolderId);
  const target = ancestors(graph, targetFolderId);
  let lcaId = source.at(-1)!;

  while (
    source.length > 0 &&
    target.length > 0 &&
    source.at(-1) === target.at(-1)
  ) {
    lcaId = source.pop()!;
    target.pop();
  }

  return { source, target, lcaId };
}

function ancestors(graph: MapGraph, folderId: string) {
  const result: string[] = [];
  let current: (typeof graph.folders)[string] | undefined =
    graph.folders[folderId];
  while (current) {
    result.push(current.id);
    current = current.parentId ? graph.folders[current.parentId] : undefined;
  }
  return result;
}

function buildZoneRouters(
  graph: MapGraph,
  rectangles: Record<string, MapRect>,
) {
  return Object.values(graph.folders).reduce<Record<string, ZoneRouter>>(
    (routers, folder) => {
      const obstacles = [
        ...folder.documentIds.map((id) => rectangles[id]),
        ...folder.childFolderIds.map((id) => rectangles[id]),
      ]
        .filter((rect): rect is MapRect => Boolean(rect))
        .map((rect) => inflateRect(rect, ROUTE_CLEARANCE));
      routers[folder.id] = new ZoneRouter(
        insetRect(rectangles[folder.id], FOLDER_INSET),
        obstacles,
      );
      return routers;
    },
    {},
  );
}

class ZoneRouter {
  private readonly usage = new Map<string, number>();

  constructor(
    private readonly bounds: MapRect,
    private readonly obstacles: MapRect[],
  ) {}

  route(source: MapPoint, target: MapPoint) {
    if (source.x === target.x || source.y === target.y) {
      if (this.segmentClear(source, target, this.obstacles)) {
        this.reserve([source, target]);
        return [source, target];
      }
    }

    const primary = this.attemptGridRoute(source, target, this.obstacles);
    if (primary) {
      this.reserve(primary);
      return primary;
    }

    const relaxedObstacles = this.obstacles.map((rect) =>
      insetRect(rect, ROUTE_CLEARANCE),
    );
    const relaxed = this.attemptGridRoute(source, target, relaxedObstacles);
    if (relaxed) {
      this.reserve(relaxed);
      return relaxed;
    }

    throw new RouteUnavailableError(this.bounds, source, target);
  }

  private attemptGridRoute(
    source: MapPoint,
    target: MapPoint,
    obstacles: MapRect[],
  ): MapPoint[] | null {
    const xs = uniqueNumbers([
      this.bounds.x,
      this.bounds.x + this.bounds.width,
      source.x,
      target.x,
      ...obstacles.flatMap((rect) => [rect.x, rect.x + rect.width]),
    ]);
    const ys = uniqueNumbers([
      this.bounds.y,
      this.bounds.y + this.bounds.height,
      source.y,
      target.y,
      ...obstacles.flatMap((rect) => [rect.y, rect.y + rect.height]),
    ]);
    const width = xs.length;
    const height = ys.length;
    const valid = new Uint8Array(width * height);

    for (let yIndex = 0; yIndex < height; yIndex += 1) {
      for (let xIndex = 0; xIndex < width; xIndex += 1) {
        const point = { x: xs[xIndex], y: ys[yIndex] };
        if (this.pointValid(point, obstacles))
          valid[yIndex * width + xIndex] = 1;
      }
    }

    const sourceIndex = ys.indexOf(source.y) * width + xs.indexOf(source.x);
    const targetIndex = ys.indexOf(target.y) * width + xs.indexOf(target.x);
    const stateCount = width * height * 3;
    const distances = new Float64Array(stateCount);
    distances.fill(Number.POSITIVE_INFINITY);
    const previous = new Int32Array(stateCount);
    previous.fill(-1);
    const heap = new MinHeap();
    const initialState = sourceIndex * 3;
    distances[initialState] = 0;
    heap.push({ state: initialState, cost: 0 });
    let finalState = -1;

    while (heap.size > 0) {
      const entry = heap.pop()!;
      if (entry.cost !== distances[entry.state]) continue;
      const nodeIndex = Math.floor(entry.state / 3);
      const direction = entry.state % 3;
      if (nodeIndex === targetIndex) {
        finalState = entry.state;
        break;
      }
      const xIndex = nodeIndex % width;
      const yIndex = Math.floor(nodeIndex / width);
      const neighbors = [
        [xIndex - 1, yIndex, 1],
        [xIndex + 1, yIndex, 1],
        [xIndex, yIndex - 1, 2],
        [xIndex, yIndex + 1, 2],
      ] as const;

      for (const [nextX, nextY, nextDirection] of neighbors) {
        if (nextX < 0 || nextX >= width || nextY < 0 || nextY >= height) {
          continue;
        }
        const nextIndex = nextY * width + nextX;
        if (!valid[nextIndex]) continue;
        const from = { x: xs[xIndex], y: ys[yIndex] };
        const to = { x: xs[nextX], y: ys[nextY] };
        if (!this.segmentClear(from, to, obstacles)) continue;
        const segment = segmentKey(from, to);
        const distance = Math.abs(to.x - from.x) + Math.abs(to.y - from.y);
        const cost =
          entry.cost +
          distance +
          (direction !== 0 && direction !== nextDirection ? BEND_COST : 0) +
          (this.usage.get(segment) ?? 0) * SHARED_SEGMENT_COST;
        const nextState = nextIndex * 3 + nextDirection;
        if (cost >= distances[nextState]) continue;
        distances[nextState] = cost;
        previous[nextState] = entry.state;
        heap.push({ state: nextState, cost });
      }
    }

    if (finalState < 0) return null;

    const reversed: MapPoint[] = [];
    let state = finalState;
    while (state >= 0) {
      const nodeIndex = Math.floor(state / 3);
      reversed.push({
        x: xs[nodeIndex % width],
        y: ys[Math.floor(nodeIndex / width)],
      });
      state = previous[state];
    }
    return compactPoints(reversed.reverse());
  }

  private pointValid(point: MapPoint, obstacles: MapRect[]) {
    if (
      point.x < this.bounds.x - GEOMETRY_EPSILON ||
      point.x > this.bounds.x + this.bounds.width + GEOMETRY_EPSILON ||
      point.y < this.bounds.y - GEOMETRY_EPSILON ||
      point.y > this.bounds.y + this.bounds.height + GEOMETRY_EPSILON
    ) {
      return false;
    }
    return !obstacles.some((rect) => pointInsideRect(point, rect));
  }

  private segmentClear(
    source: MapPoint,
    target: MapPoint,
    obstacles: MapRect[],
  ) {
    return !obstacles.some((rect) =>
      segmentIntersectsRectInterior(source, target, rect),
    );
  }

  private reserve(points: MapPoint[]) {
    for (let index = 1; index < points.length; index += 1) {
      const key = segmentKey(points[index - 1], points[index]);
      this.usage.set(key, (this.usage.get(key) ?? 0) + 1);
    }
  }

  release(points: MapPoint[]) {
    for (let index = 1; index < points.length; index += 1) {
      const key = segmentKey(points[index - 1], points[index]);
      const count = this.usage.get(key);
      if (count === undefined) continue;
      if (count <= 1) {
        this.usage.delete(key);
      } else {
        this.usage.set(key, count - 1);
      }
    }
  }
}

class MinHeap {
  private readonly values: HeapEntry[] = [];

  get size() {
    return this.values.length;
  }

  push(entry: HeapEntry) {
    this.values.push(entry);
    let index = this.values.length - 1;
    while (index > 0) {
      const parent = Math.floor((index - 1) / 2);
      if (!entryBefore(this.values[index], this.values[parent])) break;
      [this.values[index], this.values[parent]] = [
        this.values[parent],
        this.values[index],
      ];
      index = parent;
    }
  }

  pop() {
    if (this.values.length === 0) return undefined;
    const first = this.values[0];
    const last = this.values.pop()!;
    if (this.values.length === 0) return first;
    this.values[0] = last;
    let index = 0;
    while (true) {
      const left = index * 2 + 1;
      const right = left + 1;
      let smallest = index;
      if (
        left < this.values.length &&
        entryBefore(this.values[left], this.values[smallest])
      ) {
        smallest = left;
      }
      if (
        right < this.values.length &&
        entryBefore(this.values[right], this.values[smallest])
      ) {
        smallest = right;
      }
      if (smallest === index) break;
      [this.values[index], this.values[smallest]] = [
        this.values[smallest],
        this.values[index],
      ];
      index = smallest;
    }
    return first;
  }
}

function buildPortGroups(descriptors: RouteDescriptor[]) {
  const groups = new Map<string, RankedMember[]>();
  for (const descriptor of descriptors) {
    addRankedMember(
      groups,
      portKey(descriptor.link.sourceDocumentId, descriptor.sourceSide),
      descriptor.link.id,
      perpendicularCenter(descriptor.target, descriptor.sourceSide),
    );
    addRankedMember(
      groups,
      portKey(descriptor.link.targetDocumentId, descriptor.targetSide),
      descriptor.link.id,
      perpendicularCenter(descriptor.source, descriptor.targetSide),
    );
  }
  sortGroups(groups);
  return groups;
}

function buildGatewayGroups(descriptors: RouteDescriptor[]) {
  const groups = new Map<string, RankedMember[]>();
  for (const descriptor of descriptors) {
    for (const folderId of descriptor.sourceBoundaries) {
      addRankedMember(
        groups,
        gatewayKey(folderId, descriptor.sourceSide),
        descriptor.link.id,
        perpendicularCenter(descriptor.source, descriptor.sourceSide),
      );
    }
    for (const folderId of descriptor.targetBoundaries) {
      addRankedMember(
        groups,
        gatewayKey(folderId, descriptor.targetSide),
        descriptor.link.id,
        perpendicularCenter(descriptor.target, descriptor.targetSide),
      );
    }
  }
  sortGroups(groups);
  return groups;
}

function routeGateway(
  rect: MapRect,
  folderId: string,
  side: MapSide,
  groups: Map<string, RankedMember[]>,
  edgeId: string,
): MapBoundaryGateway {
  return {
    folderId,
    side,
    point: gatewayPoint(
      rect,
      side,
      rankFor(groups, gatewayKey(folderId, side), edgeId),
    ),
    regionId: '',
    laneIndex: 0,
    laneCount: 1,
  };
}

function addRankedMember(
  groups: Map<string, RankedMember[]>,
  key: string,
  id: string,
  order: number,
) {
  const group = groups.get(key) ?? [];
  group.push({ id, order });
  groups.set(key, group);
}

function sortGroups(groups: Map<string, RankedMember[]>) {
  for (const group of groups.values()) {
    group.sort(
      (left, right) => left.order - right.order || compare(left.id, right.id),
    );
  }
}

function rankFor(groups: Map<string, RankedMember[]>, key: string, id: string) {
  const group = groups.get(key) ?? [];
  return {
    index: Math.max(
      0,
      group.findIndex((member) => member.id === id),
    ),
    count: Math.max(1, group.length),
  };
}

function portPoint(
  rect: MapRect,
  side: MapSide,
  rank: { index: number; count: number },
) {
  const horizontalSide = side === 'left' || side === 'right';
  const length = horizontalSide ? rect.height : rect.width;
  return pointOnRect(
    rect,
    side,
    distributedOffset(length / 2, length - PORT_INSET * 2, rank, PORT_SPACING),
  );
}

function buildMapPort(
  documentId: string,
  linkId: string,
  direction: 'source' | 'target',
  side: MapSide,
  rect: MapRect,
  rank: { index: number; count: number },
  point: MapPoint,
): MapPort {
  return {
    id: `${documentId}:${side}:${rank.index}`,
    documentId,
    linkId,
    direction,
    side,
    index: rank.index,
    count: rank.count,
    point,
    offset: sideOffset(rect, side, point),
  };
}

function sideOffset(rect: MapRect, side: MapSide, point: MapPoint) {
  const horizontalSide = side === 'left' || side === 'right';
  const length = horizontalSide ? rect.height : rect.width;
  if (length <= 0) return 0.5;
  const local = horizontalSide ? point.y - rect.y : point.x - rect.x;
  return Math.min(1, Math.max(0, local / length));
}

function gatewayPoint(
  rect: MapRect,
  side: MapSide,
  rank: { index: number; count: number },
) {
  const horizontalSide = side === 'left' || side === 'right';
  const start = horizontalSide ? Math.min(52, rect.height / 3) : 14;
  const length = horizontalSide ? rect.height : rect.width;
  const available = Math.max(0, length - start - 14);
  return pointOnRect(
    rect,
    side,
    distributedOffset(start + available / 2, available, rank, GATEWAY_SPACING),
  );
}

function pointOnRect(rect: MapRect, side: MapSide, offset: number): MapPoint {
  if (side === 'left') return { x: rect.x, y: rect.y + offset };
  if (side === 'right') {
    return { x: rect.x + rect.width, y: rect.y + offset };
  }
  if (side === 'top') return { x: rect.x + offset, y: rect.y };
  return { x: rect.x + offset, y: rect.y + rect.height };
}

function distributedOffset(
  center: number,
  available: number,
  rank: { index: number; count: number },
  preferredSpacing: number,
) {
  const spacing =
    rank.count <= 1
      ? 0
      : Math.min(preferredSpacing, available / (rank.count - 1));
  return center + (rank.index - (rank.count - 1) / 2) * spacing;
}

function movePoint(point: MapPoint, side: MapSide, distance: number) {
  if (side === 'left') return { x: point.x - distance, y: point.y };
  if (side === 'right') return { x: point.x + distance, y: point.y };
  if (side === 'top') return { x: point.x, y: point.y - distance };
  return { x: point.x, y: point.y + distance };
}

function appendRoute(points: MapPoint[], route: MapPoint[]) {
  for (const point of route) pushPoint(points, point);
}

function compactPoints(points: MapPoint[]) {
  const result: MapPoint[] = [];
  for (const point of points) {
    const previous = result.at(-1);
    if (previous?.x === point.x && previous.y === point.y) continue;
    const beforePrevious = result.at(-2);
    if (
      beforePrevious &&
      previous &&
      ((beforePrevious.x === previous.x && previous.x === point.x) ||
        (beforePrevious.y === previous.y && previous.y === point.y))
    ) {
      result[result.length - 1] = point;
    } else {
      result.push(point);
    }
  }
  return result;
}

function deduplicatePoints(points: MapPoint[]) {
  return points.filter(
    (point, index) =>
      index === 0 ||
      point.x !== points[index - 1].x ||
      point.y !== points[index - 1].y,
  );
}

function pushPoint(points: MapPoint[], point: MapPoint) {
  const previous = points.at(-1);
  if (previous?.x !== point.x || previous.y !== point.y) points.push(point);
}

function inflateRect(rect: MapRect, amount: number): MapRect {
  return {
    x: rect.x - amount,
    y: rect.y - amount,
    width: rect.width + amount * 2,
    height: rect.height + amount * 2,
  };
}

function insetRect(rect: MapRect, amount: number): MapRect {
  return {
    x: rect.x + amount,
    y: rect.y + amount,
    width: Math.max(0, rect.width - amount * 2),
    height: Math.max(0, rect.height - amount * 2),
  };
}

function pointInsideRect(point: MapPoint, rect: MapRect) {
  return (
    point.x > rect.x + GEOMETRY_EPSILON &&
    point.x < rect.x + rect.width - GEOMETRY_EPSILON &&
    point.y > rect.y + GEOMETRY_EPSILON &&
    point.y < rect.y + rect.height - GEOMETRY_EPSILON
  );
}

function uniqueNumbers(values: number[]) {
  return [...new Set(values)].sort((left, right) => left - right);
}

function segmentKey(source: MapPoint, target: MapPoint) {
  const first =
    source.x < target.x || (source.x === target.x && source.y <= target.y)
      ? source
      : target;
  const second = first === source ? target : source;
  return `${first.x},${first.y}:${second.x},${second.y}`;
}

function portKey(documentId: string, side: MapSide) {
  return `${documentId}:${side}`;
}

function gatewayKey(folderId: string, side: MapSide) {
  return `${folderId}:${side}`;
}

function perpendicularCenter(rect: MapRect, side: MapSide) {
  return side === 'left' || side === 'right' ? centerY(rect) : centerX(rect);
}

function centerX(rect: MapRect) {
  return rect.x + rect.width / 2;
}

function centerY(rect: MapRect) {
  return rect.y + rect.height / 2;
}

function oppositeSide(side: MapSide): MapSide {
  if (side === 'left') return 'right';
  if (side === 'right') return 'left';
  if (side === 'top') return 'bottom';
  return 'top';
}

function entryBefore(left: HeapEntry, right: HeapEntry) {
  return (
    left.cost < right.cost ||
    (left.cost === right.cost && left.state < right.state)
  );
}

function compare(left: string, right: string) {
  return left < right ? -1 : left > right ? 1 : 0;
}
