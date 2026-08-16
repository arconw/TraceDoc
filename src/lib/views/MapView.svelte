<script lang="ts">
  import {
    Background,
    BackgroundVariant,
    Controls,
    SvelteFlow,
  } from '@xyflow/svelte';
  import ELK from 'elkjs/lib/elk-api.js';
  import ElkWorker from 'elkjs/lib/elk-worker.min.js?worker';
  import { onDestroy, tick } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import MapFlowActions from '../components/MapFlowActions.svelte';
  import MapDocumentNode from '../components/MapDocumentNode.svelte';
  import MapFolderNode from '../components/MapFolderNode.svelte';
  import MapRouteEdge from '../components/MapRouteEdge.svelte';
  import {
    layoutMapGraph,
    type MapFlowEdge,
    type MapFlowNode,
    type MapLayout,
  } from '../map/elk-layout';
  import {
    beginMapLayout,
    completeEmptyMapLayout,
    completeMapLayout,
    beginQueuedMapLayout,
    cancelQueuedMapLayout,
    completeQueuedMapLayout,
    createMapLayoutRequestState,
    createMapEdgeTraceState,
    createMapLayoutSession,
    effectiveMapEdgeTraceId,
    failMapLayout,
    mapLayoutIsInteractive,
    queueMapLayout,
    reduceMapEdgeTrace,
    retryQueuedMapLayout,
  } from '../map/map-view-state';
  import { mapLayoutSignature, projectToMapGraph } from '../map/project-graph';
  import {
    captureMapViewport,
    mapViewportMountAction,
    mapViewportRequestIsCurrent,
    type MapViewport,
    type SavedMapViewport,
  } from '../map/viewport-lifecycle';
  import type { DocumentId, ProjectModel } from '../types/workspace';

  export let project: ProjectModel;
  export let selectedDocumentId: DocumentId | null;
  export let visible: boolean;
  export let onOpenDocument: (documentId: DocumentId) => void;

  const nodeTypes = {
    mapFolder: MapFolderNode,
    mapDocument: MapDocumentNode,
  };
  const edgeTypes = { mapRoute: MapRouteEdge };

  interface MapFlowApi {
    fit: () => void;
    getViewport: () => MapViewport;
    restore: (viewport: MapViewport) => void;
  }

  let elk = createElk();

  let pendingProject: ProjectModel | null = null;
  let layoutRequestState = createMapLayoutRequestState();
  let layoutTimer: ReturnType<typeof setTimeout> | null = null;
  let layoutSession = createMapLayoutSession<MapLayout>();
  let layoutNodes: MapFlowNode[] = [];
  let layoutEdges: MapFlowEdge[] = [];
  let nodes: MapFlowNode[] = [];
  let edges: MapFlowEdge[] = [];
  let hoveredDocumentId: DocumentId | null = null;
  let edgeTraceState = createMapEdgeTraceState();
  let tracedEdgeId: string | null = null;
  let tracedDocumentId: DocumentId | null = null;
  let tracedEdge: MapFlowEdge | null = null;
  let connectedDocumentIds = new SvelteSet<DocumentId>();
  let traceSummary = '';
  let status: 'loading' | 'ready' | 'empty' | 'error' = 'loading';
  let message: string | null = null;
  let layoutVersion = 0;
  let layoutRevision = 0;
  let flowMountVersion = 0;
  let flowReady = false;
  let flowInitialized = false;
  let flowInitializedMountVersion = -1;
  let flowInitializedLayoutRevision = -1;
  let flowApiMountVersion = -1;
  let flowApiLayoutRevision = -1;
  let appliedViewportMountVersion = -1;
  let previousFlowVisibility = false;
  let mapCanvas: HTMLDivElement;
  let sizeObserver: ResizeObserver | null = null;
  let flowApi: MapFlowApi | null = null;
  let fitWhenReady = false;
  let graphSignature = '';
  let savedViewport: SavedMapViewport | null = null;

  $: graphSignature = mapLayoutSignature(project);
  $: scheduleLayout(project, graphSignature, visible);

  $: layoutNodes = layoutSession.layout?.nodes ?? [];
  $: layoutEdges = layoutSession.layout?.edges ?? [];
  $: status = layoutSession.status;
  $: message = layoutSession.message;
  $: tracedDocumentId = hoveredDocumentId ?? selectedDocumentId;
  $: tracedEdgeId = effectiveMapEdgeTraceId(edgeTraceState);
  $: tracedEdge = tracedEdgeId
    ? (layoutEdges.find((edge) => edge.id === tracedEdgeId) ?? null)
    : null;
  $: connectedDocumentIds = connectedDocuments(layoutEdges, tracedDocumentId);
  $: nodes = layoutNodes.map((node) => {
    if (node.data.kind !== 'document') return node;
    return {
      ...node,
      selected: node.id === selectedDocumentId,
      data: {
        ...node.data,
        emphasis: nodeEmphasis(
          node.id,
          tracedDocumentId,
          tracedEdge,
          connectedDocumentIds,
        ),
        onOpenDocument,
        onTraceDocument: traceDocument,
      },
    };
  });
  $: edges = layoutEdges.map((edge): MapFlowEdge => ({
    ...edge,
    data: {
      ...edge.data!,
      emphasis: edgeEmphasis(edge, tracedDocumentId, tracedEdge),
      onTracePointerEdge: tracePointerEdge,
    },
  }));
  $: traceSummary = tracedEdge
    ? tracedEdge.data!.ariaLabel
    : tracedDocumentId
      ? traceSummaryForDocument(layoutEdges, tracedDocumentId, project)
      : '';

  $: scheduleFlowMount(visible, status, layoutRevision);

  onDestroy(() => {
    layoutVersion += 1;
    flowMountVersion += 1;
    sizeObserver?.disconnect();
    if (layoutTimer) clearTimeout(layoutTimer);
    savedViewport = null;
    elk.terminateWorker();
  });

  export function fit() {
    savedViewport = null;
    fitWhenReady = true;
    if (!flowApi || !flowInitialized || !visible) return;
    fitWhenReady = false;
    flowApi.fit();
  }

  function registerFlowApi(
    api: MapFlowApi,
    mountVersion: number,
    revision: number,
  ) {
    if (
      !mapViewportRequestIsCurrent(
        mountVersion,
        revision,
        flowMountVersion,
        layoutRevision,
        visible,
      )
    ) {
      return;
    }
    flowApi = api;
    flowApiMountVersion = mountVersion;
    flowApiLayoutRevision = revision;
    applyInitialViewport();
  }

  function handleFlowInit(mountVersion: number, revision: number) {
    if (
      !mapViewportRequestIsCurrent(
        mountVersion,
        revision,
        flowMountVersion,
        layoutRevision,
        visible,
      )
    ) {
      return;
    }
    flowInitialized = true;
    flowInitializedMountVersion = mountVersion;
    flowInitializedLayoutRevision = revision;
    applyInitialViewport();
  }

  function applyInitialViewport() {
    if (
      !flowApi ||
      !flowInitialized ||
      flowApiMountVersion !== flowMountVersion ||
      flowApiLayoutRevision !== layoutRevision ||
      flowInitializedMountVersion !== flowMountVersion ||
      flowInitializedLayoutRevision !== layoutRevision ||
      appliedViewportMountVersion === flowMountVersion ||
      !visible
    ) {
      return;
    }

    appliedViewportMountVersion = flowMountVersion;
    const action = mapViewportMountAction(
      savedViewport,
      graphSignature,
      layoutRevision,
      fitWhenReady,
    );
    fitWhenReady = false;
    if (action.kind === 'restore') flowApi.restore(action.viewport);
    else flowApi.fit();
  }

  function saveCurrentViewport() {
    if (!flowApi || !flowInitialized || status !== 'ready') return;
    savedViewport = captureMapViewport(
      graphSignature,
      layoutRevision,
      flowApi.getViewport(),
    );
  }

  function scheduleLayout(
    nextProject: ProjectModel,
    signature: string,
    nextVisible: boolean,
  ) {
    if (savedViewport?.graphSignature !== signature) savedViewport = null;
    if (
      layoutRequestState.running &&
      (!nextVisible || signature !== layoutRequestState.activeSignature)
    ) {
      cancelActiveLayout();
    }
    pendingProject = nextProject;
    layoutRequestState = queueMapLayout(
      layoutRequestState,
      signature,
      nextVisible,
    );
    if (layoutTimer) {
      clearTimeout(layoutTimer);
      layoutTimer = null;
    }
    if (!nextVisible || layoutRequestState.pendingSignature === null) return;
    layoutTimer = setTimeout(() => {
      layoutTimer = null;
      const next = pendingProject;
      layoutRequestState = beginQueuedMapLayout(layoutRequestState);
      if (next) void updateLayout(next);
    }, 40);
  }

  function createElk() {
    return new ELK({ workerFactory: () => new ElkWorker() });
  }

  function cancelActiveLayout() {
    layoutVersion += 1;
    elk.terminateWorker();
    elk = createElk();
    layoutRequestState = cancelQueuedMapLayout(layoutRequestState);
  }

  function retryLayout() {
    layoutRequestState = retryQueuedMapLayout(layoutRequestState);
    scheduleLayout(project, mapLayoutSignature(project), visible);
  }

  function scheduleFlowMount(
    nextVisible: boolean,
    nextStatus: typeof status,
    revision: number,
  ) {
    if (previousFlowVisibility && !nextVisible) saveCurrentViewport();
    previousFlowVisibility = nextVisible;
    const version = ++flowMountVersion;
    flowReady = false;
    flowInitialized = false;
    flowInitializedMountVersion = -1;
    flowInitializedLayoutRevision = -1;
    flowApiMountVersion = -1;
    flowApiLayoutRevision = -1;
    flowApi = null;
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
    savedViewport = null;
    layoutSession = beginMapLayout(layoutSession, 'loading');
    const version = layoutSession.requestId;
    layoutVersion = version;
    hoveredDocumentId = null;
    edgeTraceState = createMapEdgeTraceState();

    try {
      const graph = projectToMapGraph(nextProject);
      if (Object.keys(graph.documents).length === 0) {
        savedViewport = null;
        layoutSession = completeEmptyMapLayout(layoutSession, version);
        layoutRequestState = completeQueuedMapLayout(layoutRequestState);
        return;
      }
      const layout = await layoutMapGraph(graph, elk);
      if (version !== layoutVersion) return;
      layoutSession = completeMapLayout(layoutSession, version, layout);
      layoutRequestState = completeQueuedMapLayout(layoutRequestState);
      layoutRevision += 1;
    } catch (error) {
      if (version !== layoutVersion) return;
      savedViewport = null;
      layoutSession = failMapLayout(
        layoutSession,
        version,
        error instanceof Error ? error.message : String(error),
      );
      layoutRequestState = completeQueuedMapLayout(layoutRequestState);
    }
  }

  function traceDocument(documentId: string | null) {
    hoveredDocumentId = documentId;
  }

  function tracePointerEdge(edgeId: string | null) {
    edgeTraceState = reduceMapEdgeTrace(edgeTraceState, {
      source: 'pointer',
      edgeId,
    });
  }

  function traceFocusedEdge(edgeId: string | null) {
    edgeTraceState = reduceMapEdgeTrace(edgeTraceState, {
      source: 'focus',
      edgeId,
    });
  }

  function connectedDocuments(
    layout: MapFlowEdge[],
    documentId: DocumentId | null,
  ) {
    const connected = new SvelteSet<DocumentId>();
    if (!documentId) return connected;
    for (const edge of layout) {
      if (edge.source === documentId) connected.add(edge.target);
      if (edge.target === documentId) connected.add(edge.source);
    }
    return connected;
  }

  function nodeEmphasis(
    documentId: string,
    activeDocumentId: DocumentId | null,
    activeEdge: MapFlowEdge | null,
    connected: SvelteSet<DocumentId>,
  ): 'normal' | 'active' | 'connected' | 'muted' {
    if (activeEdge) {
      return activeEdge.source === documentId ||
        activeEdge.target === documentId
        ? 'active'
        : 'muted';
    }
    if (!activeDocumentId) return 'normal';
    if (documentId === activeDocumentId) return 'active';
    return connected.has(documentId) ? 'connected' : 'muted';
  }

  function edgeEmphasis(
    edge: MapFlowEdge,
    activeDocumentId: DocumentId | null,
    activeEdge: MapFlowEdge | null,
  ): 'normal' | 'active' | 'muted' {
    if (activeEdge) return edge.id === activeEdge.id ? 'active' : 'muted';
    if (!activeDocumentId) return 'normal';
    return edge.source === activeDocumentId || edge.target === activeDocumentId
      ? 'active'
      : 'muted';
  }

  function traceSummaryForDocument(
    layout: MapFlowEdge[],
    documentId: DocumentId,
    nextProject: ProjectModel,
  ) {
    let incoming = 0;
    let outgoing = 0;
    for (const edge of layout) {
      if (edge.source === documentId) outgoing += 1;
      if (edge.target === documentId) incoming += 1;
    }
    const document = nextProject.documents[documentId];
    return document
      ? `${document.title ?? document.name}: ${incoming} incoming and ${outgoing} outgoing links`
      : '';
  }
</script>

<section class="map-view" aria-label="Architecture map">
  <p class="visually-hidden" aria-live="polite">{traceSummary}</p>
  {#if mapLayoutIsInteractive(layoutSession, visible, flowReady)}
    <div class="edge-keyboard-targets">
      {#each layoutEdges as edge (edge.id)}
        <button
          type="button"
          aria-label={edge.data!.ariaLabel}
          aria-pressed={edgeTraceState.focusedEdgeId === edge.id}
          onfocus={() => traceFocusedEdge(edge.id)}
          onblur={() => traceFocusedEdge(null)}
          onclick={() => traceFocusedEdge(edge.id)}
        ></button>
      {/each}
    </div>
  {/if}
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
      <button type="button" onclick={retryLayout}>Retry</button>
    </div>
  {:else}
    <div
      class="map-canvas"
      aria-busy={status === 'loading'}
      bind:this={mapCanvas}
    >
      {#if flowReady && layoutNodes.length > 0}
        {@const mountedFlowVersion = flowMountVersion}
        {@const mountedLayoutRevision = layoutRevision}
        {#key layoutRevision}
          <SvelteFlow
            {nodes}
            {edges}
            {nodeTypes}
            {edgeTypes}
            oninit={() =>
              handleFlowInit(mountedFlowVersion, mountedLayoutRevision)}
            minZoom={0.08}
            maxZoom={2}
            nodesDraggable={false}
            nodesConnectable={false}
            elementsSelectable={false}
            nodesFocusable={false}
            edgesFocusable={false}
            zIndexMode="manual"
            deleteKey={null}
            onlyRenderVisibleElements
            aria-label="Interactive architecture map"
          >
            <MapFlowActions
              onReady={(api) =>
                registerFlowApi(api, mountedFlowVersion, mountedLayoutRevision)}
            />
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

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .edge-keyboard-targets {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    clip-path: inset(50%);
    white-space: nowrap;
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
