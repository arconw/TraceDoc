<script lang="ts">
  import type { MapEdgeData } from '../map/elk-layout';
  import type { MapChevronDirection, MapPoint } from '../map/routing';

  export let id = '';
  export let data: MapEdgeData | undefined = undefined;
  export let markerEnd: string | undefined = undefined;
  export let interactionWidth = 12;

  const CROSSING_GAP_WIDTH = 6;
  const CHEVRON_SHAPE = 'M -3 -3.5 L 2.5 0 L -3 3.5';
  const CHEVRON_ANGLES: Record<MapChevronDirection, number> = {
    right: 0,
    down: 90,
    left: 180,
    up: 270,
  };

  let hovered = false;

  function serializePoints(points: MapPoint[]) {
    return points
      .map((point, index) => `${index === 0 ? 'M' : 'L'} ${point.x} ${point.y}`)
      .join(' ');
  }

  function isOnSegment(from: MapPoint, to: MapPoint, point: MapPoint) {
    if (from.x === to.x) {
      if (Math.abs(point.x - from.x) > 0.01) return false;
      const minimum = Math.min(from.y, to.y);
      const maximum = Math.max(from.y, to.y);
      return point.y > minimum + 0.01 && point.y < maximum - 0.01;
    }
    if (Math.abs(point.y - from.y) > 0.01) return false;
    const minimum = Math.min(from.x, to.x);
    const maximum = Math.max(from.x, to.x);
    return point.x > minimum + 0.01 && point.x < maximum - 0.01;
  }

  function distanceFrom(from: MapPoint, point: MapPoint) {
    return Math.abs(point.x - from.x) + Math.abs(point.y - from.y);
  }

  function segmentGaps(from: MapPoint, to: MapPoint, gaps: MapPoint[]) {
    return gaps
      .filter((gap) => isOnSegment(from, to, gap))
      .sort(
        (left, right) => distanceFrom(from, left) - distanceFrom(from, right),
      );
  }

  function shiftAlongSegment(
    from: MapPoint,
    to: MapPoint,
    point: MapPoint,
    distance: number,
  ): MapPoint {
    if (from.x === to.x) {
      const direction = to.y > from.y ? 1 : -1;
      return { x: point.x, y: point.y - direction * distance };
    }
    const direction = to.x > from.x ? 1 : -1;
    return { x: point.x - direction * distance, y: point.y };
  }

  function clampedHalfWidth(
    from: MapPoint,
    to: MapPoint,
    point: MapPoint,
    desired: number,
  ) {
    return Math.max(
      0,
      Math.min(desired, distanceFrom(from, point), distanceFrom(point, to)),
    );
  }

  function buildVisiblePath(points: MapPoint[], gaps: MapPoint[]) {
    if (points.length < 2 || gaps.length === 0) return serializePoints(points);

    const subpaths: MapPoint[][] = [[points[0]]];
    for (let index = 1; index < points.length; index += 1) {
      const from = points[index - 1];
      const to = points[index];
      for (const gap of segmentGaps(from, to, gaps)) {
        const halfWidth = clampedHalfWidth(
          from,
          to,
          gap,
          CROSSING_GAP_WIDTH / 2,
        );
        subpaths.at(-1)!.push(shiftAlongSegment(from, to, gap, halfWidth));
        subpaths.push([shiftAlongSegment(from, to, gap, -halfWidth)]);
      }
      subpaths.at(-1)!.push(to);
    }
    return subpaths.map(serializePoints).join(' ');
  }

  $: interactionPath = serializePoints(data?.points ?? []);
  $: visiblePath = buildVisiblePath(
    data?.points ?? [],
    data?.crossingGaps ?? [],
  );
  $: chevrons = data?.chevrons ?? [];
  $: corridor = data?.corridor ?? null;
  $: corridorReverse = corridor
    ? corridor.axis === 'horizontal'
      ? corridor.direction === 'left'
      : corridor.direction === 'up'
    : false;

  function updateTrace() {
    data?.onTracePointerEdge?.(hovered ? id : null);
  }

  function setHovered(value: boolean) {
    hovered = value;
    updateTrace();
  }
</script>

<path
  d={visiblePath}
  class:active={data?.emphasis === 'active'}
  class:muted={data?.emphasis === 'muted'}
  class:corridor={Boolean(corridor)}
  class:corridor-reverse={corridorReverse}
  class="map-route"
  marker-end={markerEnd}
  data-corridor={corridor?.corridorId}
  aria-hidden="true"
/>
{#each chevrons as chevron, index (index)}
  <path
    d={CHEVRON_SHAPE}
    class:active={data?.emphasis === 'active'}
    class:muted={data?.emphasis === 'muted'}
    class="map-route-chevron"
    transform={`translate(${chevron.point.x} ${chevron.point.y}) rotate(${CHEVRON_ANGLES[chevron.direction]})`}
    aria-hidden="true"
  />
{/each}
<path
  d={interactionPath}
  class="map-route-interaction"
  stroke-width={interactionWidth}
  aria-hidden="true"
  onpointerenter={() => setHovered(true)}
  onpointerleave={() => setHovered(false)}
  onclick={() => setHovered(true)}
/>

<style>
  .map-route {
    fill: none;
    stroke: var(--color-map-edge);
    stroke-linecap: square;
    stroke-linejoin: miter;
    stroke-width: 1.1;
    vector-effect: non-scaling-stroke;
    transition:
      opacity 90ms ease,
      stroke 90ms ease,
      stroke-width 90ms ease;
  }

  .map-route.corridor {
    stroke-width: 1.3;
  }

  .map-route.corridor-reverse {
    stroke-dasharray: 3 2;
  }

  .map-route.active {
    stroke: var(--color-map-edge-active);
    stroke-width: 1.8;
  }

  .map-route.muted {
    opacity: 0.12;
  }

  .map-route-chevron {
    fill: none;
    stroke: var(--color-map-edge);
    stroke-width: 1;
    stroke-linecap: round;
    stroke-linejoin: round;
    opacity: 0.5;
    pointer-events: none;
    transition:
      opacity 90ms ease,
      stroke 90ms ease;
  }

  .map-route-chevron.active {
    stroke: var(--color-map-edge-active);
    opacity: 0.85;
  }

  .map-route-chevron.muted {
    opacity: 0.05;
  }

  .map-route-interaction {
    fill: none;
    stroke: transparent;
    pointer-events: stroke;
  }
</style>
