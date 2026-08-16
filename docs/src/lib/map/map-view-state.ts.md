# Map View State

Source: `src/lib/map/map-view-state.ts`.

Pure reducers for trace priority, layout result gating, and single-flight/deferred layout requests.

## Edge trace

- `MapEdgeTraceState`, `MapEdgeTraceEvent` — independent pointer/focus sources.
- `createMapEdgeTraceState()` — null initial trace.
- `reduceMapEdgeTrace(state,event)` — updates only the event source.
- `effectiveMapEdgeTraceId(state)` — pointer trace wins, then keyboard focus.

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
- `retryQueuedMapLayout(state)` — requeues the last failed signature.

Consumer: [`MapView`](../views/MapView.svelte.md).
