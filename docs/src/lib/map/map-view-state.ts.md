# Map View State

Source: `src/lib/map/map-view-state.ts`.

Pure reducers for trace priority, layout result gating, and single-flight/deferred layout requests.

## Edge trace

- `MapEdgeTraceState`, `MapEdgeTraceEvent` — independent pointer/focus sources.
- `createMapEdgeTraceState()` — null initial trace.
- `reduceMapEdgeTrace(state,event)` — updates only the event source.
- `effectiveMapEdgeTraceId(state)` — pointer trace wins, then keyboard focus.
- `resolveTracedEdge(state,documentTraceActive,layoutEdges)` — the trace-priority gate: returns `null` outright when `documentTraceActive` is true (a document is actively hovered/focused), otherwise resolves `effectiveMapEdgeTraceId(state)` against `layoutEdges` (or `null` if there is no traced ID or it no longer resolves). This is the single source of truth for "does a document or an edge win the projection right now"; [`MapView`](../views/MapView.svelte.md) calls it directly from its reactive `tracedEdge`, and `map-graph.test.mjs` exercises the same exported function rather than a reimplementation.
- `clearMapEdgeTraceForDocument(state,documentId)` — returns a fully reset edge trace when `documentId` is non-null, `state` unchanged otherwise. Retained as a pure full-reset reducer and exercised directly by tests as the "no edge trace can survive" oracle; [`MapView`](../views/MapView.svelte.md) no longer calls it from `traceDocument` (see that file's trace-priority invariant for why: clearing the live pointer/focus sources here would erase a still-active edge trace instead of merely deprioritizing it).
- `MapTraceState`, `resetMapTraceOnFlowUnmount()` — the flow-unmount reset: a fresh `{ hoveredDocumentId: null, edgeTraceState: createMapEdgeTraceState() }`. Unlike the edge trace's own sources, `hoveredDocumentId` and `edgeTraceState` cannot outlive an `SvelteFlow` unmount — their only sources are `onpointerleave`/`onblur`/keyboard-focus callbacks on elements that unmounting does not reliably invoke — so [`MapView`](../views/MapView.svelte.md) calls this from `scheduleFlowMount` on every mount-cycle transition, not only on a genuine leave/blur event.
- `clearMapDocumentTraceIfActive(hoveredDocumentId,documentId)` — clears a single active document trace, not the whole flow: returns `null` only when `documentId` is non-null and still equals `hoveredDocumentId`, otherwise returns `hoveredDocumentId` unchanged. This is the per-node counterpart to `resetMapTraceOnFlowUnmount`: `onlyRenderVisibleElements` can unmount an individual, still-hovered/focused [`MapDocumentNode`](../components/MapDocumentNode.svelte.md) (e.g. panned out of the viewport) without firing its `onblur`/`onpointerleave` first, and without tearing down the flow itself, so `resetMapTraceOnFlowUnmount` never runs. The equality guard exists because that node's own destroy callback fires unconditionally with its own ID; if a different document has already become the active trace by the time the stale destroy runs, the guard leaves it untouched instead of erasing a valid, unrelated trace.

## Trace projection

- `connectedDocuments(edges,documentId)` — one-hop neighbor IDs reachable from `documentId` by scanning resolved `MapFlowEdge.source`/`target`; identity is always the document ID, never name or path, so duplicate filenames across folders cannot collide.
- `MapNodeEmphasis`, `MapEdgeEmphasis` — `'normal' | 'active' | 'connected' | 'muted'` and `'normal' | 'active' | 'muted'`.
- `nodeEmphasis(documentId,activeDocumentId,activeEdge,connected)` — an active edge takes priority over the active document; otherwise active/connected/muted by ID membership.
- `edgeEmphasis(edge,activeDocumentId,activeEdge)` — mirrors `nodeEmphasis` for edges.

## Layout session

- `MapLayoutStatus`, `MapLayoutSession<Layout>` — request ID, status, layout, error.
- `createMapLayoutSession()` — loading/null session.
- `beginMapLayout(session,status)` — increments request and clears stale layout.
- `completeMapLayout`, `completeEmptyMapLayout`, `failMapLayout` — request-ID-gated terminal transitions.
- `mapLayoutIsInteractive(session,visible,flowReady)` — control enablement predicate.

## Request queue

- `MapLayoutRequestState` — active/pending signatures, request count, running flag.
- `createMapLayoutRequestState()` — idle queue.
- `queueMapLayout(state,signature,visible)` — defers hidden or superseding signatures.
- `beginQueuedMapLayout(state)` — promotes pending signature.
- `completeQueuedMapLayout(state)` — clears running.
- `cancelQueuedMapLayout(state)` — invalidates active running work.
- `retryQueuedMapLayout(state)` — requeues the active signature as pending when not currently running.

Consumer: [`MapView`](../views/MapView.svelte.md).
