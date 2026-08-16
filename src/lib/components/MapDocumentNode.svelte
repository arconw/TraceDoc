<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  import type { MapNodeData } from '../map/elk-layout';

  export let data: MapNodeData;
  export let selected: boolean;
</script>

<Handle
  class="map-handle"
  type="target"
  position={Position.Left}
  isConnectable={false}
  aria-hidden="true"
  role="presentation"
  tabindex={-1}
/>
<button
  type="button"
  class:selected
  title={`${data.label} — ${data.path}`}
  aria-label={`Open ${data.label}, ${data.path}`}
  aria-current={selected ? 'page' : undefined}
>
  <svg viewBox="0 0 16 16" aria-hidden="true">
    <path d="M4 1.75h5l3 3V14.25H4z" />
    <path d="M9 1.75v3h3" />
  </svg>
  <span class="document-copy">
    <strong>{data.label}</strong>
    <span>{data.path}</span>
  </span>
</button>
<Handle
  class="map-handle"
  type="source"
  position={Position.Right}
  isConnectable={false}
  aria-hidden="true"
  role="presentation"
  tabindex={-1}
/>

<style>
  button {
    display: flex;
    width: 100%;
    height: 100%;
    align-items: center;
    gap: var(--space-2);
    overflow: hidden;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-map-document-border);
    border-radius: var(--radius-sm);
    background: var(--color-map-document);
    color: var(--color-foreground);
    cursor: pointer;
    text-align: left;
    box-shadow: 0 0.25rem 0.8rem rgb(0 0 0 / 18%);
  }

  button:hover {
    border-color: var(--color-muted);
    background: var(--color-map-document-hover);
  }

  button.selected {
    border-color: var(--color-focus);
    background: var(--color-selection);
    box-shadow:
      0 0 0 1px color-mix(in srgb, var(--color-focus) 22%, transparent),
      0 0.25rem 0.8rem rgb(0 0 0 / 18%);
  }

  button:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }

  svg {
    width: 1rem;
    min-width: 1rem;
    height: 1rem;
    fill: none;
    stroke: var(--color-muted);
    stroke-linejoin: round;
    stroke-width: 1.1;
  }

  .document-copy {
    display: grid;
    min-width: 0;
    gap: 0.15rem;
  }

  strong,
  .document-copy > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  strong {
    font-size: 0.75rem;
    font-weight: 650;
  }

  .document-copy > span {
    color: var(--color-muted-dim);
    font-family: var(--font-family-mono);
    font-size: 0.625rem;
  }

  :global(.map-handle) {
    width: 0.4rem;
    height: 0.4rem;
    border: 1px solid var(--color-map-edge);
    background: var(--color-map-document);
    pointer-events: none;
  }
</style>
