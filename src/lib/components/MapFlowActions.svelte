<script lang="ts">
  import { useSvelteFlow } from '@xyflow/svelte';
  import { onMount } from 'svelte';
  import type { MapViewport } from '../map/viewport-lifecycle';
  import { mapFitDuration } from '../utils/ui-behavior';

  interface MapFlowApi {
    fit: () => void;
    getViewport: () => MapViewport;
    restore: (viewport: MapViewport) => void;
  }

  export let onReady: (api: MapFlowApi) => void;

  const { fitView, getViewport, setViewport } = useSvelteFlow();
  let reducedMotion = false;

  onMount(() => {
    const media = window.matchMedia('(prefers-reduced-motion: reduce)');
    const update = () => (reducedMotion = media.matches);
    update();
    media.addEventListener('change', update);
    return () => media.removeEventListener('change', update);
  });

  $: onReady({
    fit: () => {
      void fitView({
        padding: 0.12,
        maxZoom: 1,
        duration: mapFitDuration(reducedMotion),
      });
    },
    getViewport,
    restore: (viewport) => {
      void setViewport(viewport, { duration: 0 });
    },
  });
</script>
