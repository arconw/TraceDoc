<script lang="ts">
  import { Handle, Position } from '@xyflow/svelte';
  import { onDestroy } from 'svelte';
  import { mapHandleId, type MapNodeData } from '../map/elk-layout';
  import type { MapPort } from '../map/routing';

  export let data: MapNodeData;
  export let selected: boolean;

  let hovered = false;
  let focused = false;

  const SIDE_POSITIONS = {
    top: Position.Top,
    right: Position.Right,
    bottom: Position.Bottom,
    left: Position.Left,
  } as const;

  $: ports = data.ports ?? [];
  $: nodeEmphasized =
    data.emphasis === 'active' || data.emphasis === 'connected';
  $: relationshipSummary = `${data.incomingCount ?? 0} incoming, ${data.outgoingCount ?? 0} outgoing links`;

  function portClass(port: MapPort) {
    if (port.linkId === data.activeEdgeId) return 'map-port map-port-active';
    if (nodeEmphasized) return 'map-port map-port-emphasized';
    return 'map-port';
  }

  function portStyle(port: MapPort) {
    const percent = `${(port.offset * 100).toFixed(3)}%`;
    return port.side === 'top' || port.side === 'bottom'
      ? `left: ${percent}`
      : `top: ${percent}`;
  }

  function openDocument() {
    if (data.documentId) data.onOpenDocument?.(data.documentId);
  }

  function updateTrace() {
    data.onTraceDocument?.(
      hovered || focused ? (data.documentId ?? null) : null,
    );
  }

  function setHovered(value: boolean) {
    hovered = value;
    updateTrace();
  }

  function setFocused(value: boolean) {
    focused = value;
    updateTrace();
  }

  onDestroy(() => {
    if (data.documentId) data.onTraceDocumentUnmount?.(data.documentId);
  });
</script>

{#each ports as port (port.id)}
  <Handle
    id={mapHandleId(port)}
    class={portClass(port)}
    type={port.direction}
    position={SIDE_POSITIONS[port.side]}
    style={portStyle(port)}
    isConnectable={false}
    aria-hidden="true"
    role="presentation"
    tabindex={-1}
  />
{/each}
<button
  type="button"
  class:selected
  class:active={data.emphasis === 'active'}
  class:connected={data.emphasis === 'connected'}
  class:muted={data.emphasis === 'muted'}
  title={`${data.label} — ${data.path}`}
  aria-label={`Open ${data.label}, ${data.path}; ${relationshipSummary}`}
  aria-current={selected ? 'page' : undefined}
  onclick={openDocument}
  onpointerenter={() => setHovered(true)}
  onpointerleave={() => setHovered(false)}
  onfocus={() => setFocused(true)}
  onblur={() => setFocused(false)}
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

  button.active {
    border-color: var(--color-map-edge-active);
    background: var(--color-selection);
  }

  button.connected {
    border-color: color-mix(
      in srgb,
      var(--color-map-edge-active) 72%,
      var(--color-map-document-border)
    );
  }

  button.muted {
    opacity: 0.42;
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

  :global(.map-port) {
    width: 0.32rem;
    height: 0.32rem;
    border: 1px solid var(--color-map-edge);
    background: var(--color-map-document);
    opacity: 0.4;
    pointer-events: none;
    transition:
      opacity 90ms ease,
      width 90ms ease,
      height 90ms ease,
      background 90ms ease,
      border-color 90ms ease;
  }

  :global(.svelte-flow__node-mapDocument:hover .map-port),
  :global(.svelte-flow__node-mapDocument:focus-within .map-port),
  :global(.map-port-emphasized) {
    opacity: 0.85;
  }

  :global(.map-port-active) {
    width: 0.46rem;
    height: 0.46rem;
    border-color: var(--color-map-edge-active);
    background: var(--color-map-edge-active);
    opacity: 1;
  }

  @media (forced-colors: active) {
    :global(.map-port) {
      border-color: CanvasText;
      background: Canvas;
    }

    :global(.map-port-active) {
      border-color: Highlight;
      background: Highlight;
    }
  }
</style>
