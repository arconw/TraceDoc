<script lang="ts">
  import type { MapNodeData } from '../map/elk-layout';

  export let data: MapNodeData;
</script>

<section
  class="map-folder"
  aria-label={`Folder ${data.label}`}
  title={data.path}
>
  <header>
    <svg viewBox="0 0 16 16" aria-hidden="true">
      <path d="M1.75 4.25h4l1.25 1.5h7.25v7.5H1.75z" />
      <path d="M1.75 4.25V2.75h4l1.25 1.5h7.25v1.5" />
    </svg>
    <span>{data.label}</span>
  </header>
</section>

<style>
  :global(.svelte-flow__node-mapFolder) {
    pointer-events: none;
  }

  .map-folder {
    width: 100%;
    height: 100%;
    overflow: hidden;
    border: 1px solid var(--color-map-folder-border);
    border-radius: var(--radius-md);
    background: var(--color-map-folder);
    box-shadow: inset 0 1px rgb(255 255 255 / 2%);
  }

  header {
    display: flex;
    height: 2.25rem;
    align-items: center;
    gap: var(--space-2);
    overflow: hidden;
    padding: 0 var(--space-3);
    border-bottom: 1px solid var(--color-map-folder-border);
    color: var(--color-foreground-subtle);
    font-size: 0.75rem;
    font-weight: 650;
    letter-spacing: 0.015em;
  }

  svg {
    width: 0.875rem;
    min-width: 0.875rem;
    height: 0.875rem;
    fill: none;
    stroke: var(--color-muted);
    stroke-linejoin: round;
    stroke-width: 1.1;
  }

  span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
