<script lang="ts">
  import {
    Background,
    BackgroundVariant,
    Controls,
    SvelteFlow,
    type Node,
  } from '@xyflow/svelte';
  import ELK from 'elkjs/lib/elk-api.js';
  import ElkWorker from 'elkjs/lib/elk-worker.min.js?worker';
  import { onDestroy, tick } from 'svelte';
  import MapDocumentNode from '../components/MapDocumentNode.svelte';
  import MapFolderNode from '../components/MapFolderNode.svelte';
  import {
    layoutMapGraph,
    type MapFlowEdge,
    type MapFlowNode,
  } from '../map/elk-layout';
  import { projectToMapGraph } from '../map/project-graph';
  import type { DocumentId, ProjectModel } from '../types/workspace';

  export let project: ProjectModel;
  export let selectedDocumentId: DocumentId | null;
  export let visible: boolean;
  export let onOpenDocument: (documentId: DocumentId) => void;

  const nodeTypes = {
    mapFolder: MapFolderNode,
    mapDocument: MapDocumentNode,
  };
  const elk = new ELK({ workerFactory: () => new ElkWorker() });

  let requestedProject: ProjectModel | null = null;
  let layoutNodes: MapFlowNode[] = [];
  let layoutEdges: MapFlowEdge[] = [];
  let nodes: MapFlowNode[] = [];
  let status: 'loading' | 'ready' | 'empty' | 'error' = 'loading';
  let message: string | null = null;
  let layoutVersion = 0;
  let layoutRevision = 0;
  let flowMountVersion = 0;
  let flowReady = false;
  let mapCanvas: HTMLDivElement;
  let sizeObserver: ResizeObserver | null = null;

  $: if (project !== requestedProject) {
    requestedProject = project;
    void updateLayout(project);
  }

  $: nodes = layoutNodes.map((node) =>
    node.data.kind === 'document'
      ? { ...node, selected: node.id === selectedDocumentId }
      : node,
  );

  $: scheduleFlowMount(visible, status, layoutRevision);

  onDestroy(() => {
    layoutVersion += 1;
    flowMountVersion += 1;
    sizeObserver?.disconnect();
    elk.terminateWorker();
  });

  function scheduleFlowMount(
    nextVisible: boolean,
    nextStatus: typeof status,
    revision: number,
  ) {
    const version = ++flowMountVersion;
    flowReady = false;
    sizeObserver?.disconnect();
    sizeObserver = null;

    if (!nextVisible || nextStatus !== 'ready') return;
    void mountFlowWhenSized(version, revision);
  }

  async function mountFlowWhenSized(version: number, revision: number) {
    await tick();
    await new Promise<void>((resolve) =>
      requestAnimationFrame(() => resolve()),
    );

    if (
      version !== flowMountVersion ||
      revision !== layoutRevision ||
      !visible ||
      status !== 'ready'
    ) {
      return;
    }

    if (mapCanvas.clientWidth > 0 && mapCanvas.clientHeight > 0) {
      flowReady = true;
      return;
    }

    const observer = new ResizeObserver(() => {
      if (
        version !== flowMountVersion ||
        revision !== layoutRevision ||
        !visible ||
        status !== 'ready'
      ) {
        observer.disconnect();
        if (sizeObserver === observer) sizeObserver = null;
        return;
      }

      if (mapCanvas.clientWidth > 0 && mapCanvas.clientHeight > 0) {
        observer.disconnect();
        if (sizeObserver === observer) sizeObserver = null;
        requestAnimationFrame(() => {
          if (
            version === flowMountVersion &&
            revision === layoutRevision &&
            visible &&
            status === 'ready'
          ) {
            flowReady = true;
          }
        });
      }
    });
    sizeObserver = observer;
    observer.observe(mapCanvas);
  }

  async function updateLayout(nextProject: ProjectModel) {
    const version = ++layoutVersion;
    const graph = projectToMapGraph(nextProject);
    message = null;

    if (Object.keys(graph.documents).length === 0) {
      layoutNodes = [];
      layoutEdges = [];
      status = 'empty';
      return;
    }

    status = 'loading';

    try {
      const layout = await layoutMapGraph(graph, elk);
      if (version !== layoutVersion) return;
      layoutNodes = layout.nodes;
      layoutEdges = layout.edges;
      layoutRevision += 1;
      status = 'ready';
    } catch (error) {
      if (version !== layoutVersion) return;
      message = error instanceof Error ? error.message : String(error);
      status = 'error';
    }
  }

  function handleNodeClick({ node }: { node: Node }) {
    if (node.data.kind === 'document') {
      onOpenDocument(node.id);
    }
  }
</script>

<section class="map-view" aria-label="Architecture map">
  {#if status === 'empty'}
    <div class="map-state">
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <rect x="3.5" y="4" width="7" height="6" rx="1" />
        <rect x="13.5" y="14" width="7" height="6" rx="1" />
        <path d="M10.5 7h3v10h0" />
      </svg>
      <p>No Markdown documents to map</p>
      <span>Add Markdown files to this workspace to see its architecture.</span>
    </div>
  {:else if status === 'error'}
    <div class="map-state map-state--error" role="alert">
      <p>Unable to lay out the architecture map</p>
      <span>{message}</span>
      <button type="button" onclick={() => updateLayout(project)}>Retry</button>
    </div>
  {:else}
    <div
      class="map-canvas"
      aria-busy={status === 'loading'}
      bind:this={mapCanvas}
    >
      {#if flowReady && layoutNodes.length > 0}
        {#key layoutRevision}
          <SvelteFlow
            {nodes}
            edges={layoutEdges}
            {nodeTypes}
            fitView
            fitViewOptions={{ padding: 0.12, maxZoom: 1 }}
            minZoom={0.08}
            maxZoom={2}
            nodesDraggable={false}
            nodesConnectable={false}
            elementsSelectable={false}
            nodesFocusable={false}
            edgesFocusable={false}
            deleteKey={null}
            onlyRenderVisibleElements
            onnodeclick={handleNodeClick}
            aria-label="Interactive architecture map"
          >
            <Background
              variant={BackgroundVariant.Dots}
              gap={22}
              size={1}
              patternColor="#2c323d"
            />
            <Controls
              showLock={false}
              fitViewOptions={{ padding: 0.12, maxZoom: 1 }}
              aria-label="Map zoom and fit controls"
            />
          </SvelteFlow>
        {/key}
      {/if}
      {#if status === 'loading'}
        <div class="map-loading" role="status" aria-live="polite">
          Laying out map…
        </div>
      {/if}
    </div>
  {/if}
</section>

<style>
  .map-view,
  .map-canvas {
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
  }

  .map-view {
    position: relative;
    overflow: hidden;
    background: var(--color-map-background);
  }

  .map-canvas {
    position: relative;
  }

  .map-state {
    display: grid;
    width: 100%;
    height: 100%;
    align-content: center;
    justify-items: center;
    gap: var(--space-2);
    padding: var(--space-6);
    color: var(--color-muted);
    text-align: center;
  }

  .map-state svg {
    width: 2rem;
    height: 2rem;
    margin-bottom: var(--space-1);
    fill: none;
    stroke: var(--color-border-strong);
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-width: 1.2;
  }

  .map-state p,
  .map-state span {
    margin: 0;
  }

  .map-state p {
    color: var(--color-foreground-subtle);
    font-size: var(--font-size-sm);
    font-weight: 500;
  }

  .map-state span {
    max-width: 34rem;
    color: var(--color-muted-dim);
    font-size: 0.75rem;
  }

  .map-state--error span {
    color: var(--color-error);
  }

  .map-state button {
    margin-top: var(--space-2);
    padding: 0.35rem var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    color: var(--color-foreground);
    cursor: pointer;
    font-size: 0.75rem;
  }

  .map-state button:focus-visible {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }

  .map-loading {
    position: absolute;
    top: var(--space-3);
    left: 50%;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-full);
    background: color-mix(in srgb, var(--color-surface) 92%, transparent);
    color: var(--color-muted);
    font-size: 0.6875rem;
    transform: translateX(-50%);
  }

  .map-view :global(.svelte-flow__edge-path) {
    stroke: var(--color-map-edge);
    stroke-width: 1.25;
  }

  .map-view :global(.svelte-flow__controls) {
    overflow: hidden;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--radius-sm);
    box-shadow: 0 0.35rem 1rem rgb(0 0 0 / 22%);
  }

  .map-view :global(.svelte-flow__controls-button) {
    border: 0;
    border-bottom: 1px solid var(--color-border);
    background: var(--color-surface-raised);
    color: var(--color-foreground-subtle);
  }

  .map-view :global(.svelte-flow__controls-button:hover) {
    background: var(--color-surface-hover);
  }

  .map-view :global(.svelte-flow__controls-button:focus-visible) {
    outline: 2px solid var(--color-focus);
    outline-offset: -2px;
  }

  .map-view :global(.svelte-flow__controls-button svg) {
    fill: currentColor;
  }
</style>
