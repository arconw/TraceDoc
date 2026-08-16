<script lang="ts">
  import type { MapEdgeData } from '../map/elk-layout';

  export let id = '';
  export let data: MapEdgeData | undefined = undefined;
  export let markerEnd: string | undefined = undefined;
  export let interactionWidth = 12;

  let hovered = false;

  $: path = (data?.points ?? [])
    .map((point, index) => `${index === 0 ? 'M' : 'L'} ${point.x} ${point.y}`)
    .join(' ');
  function updateTrace() {
    data?.onTracePointerEdge?.(hovered ? id : null);
  }

  function setHovered(value: boolean) {
    hovered = value;
    updateTrace();
  }
</script>

<path
  d={path}
  class:active={data?.emphasis === 'active'}
  class:muted={data?.emphasis === 'muted'}
  class="map-route"
  marker-end={markerEnd}
  aria-hidden="true"
/>
<path
  d={path}
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

  .map-route.active {
    stroke: var(--color-map-edge-active);
    stroke-width: 1.8;
  }

  .map-route.muted {
    opacity: 0.12;
  }

  .map-route-interaction {
    fill: none;
    stroke: transparent;
    pointer-events: stroke;
  }
</style>
