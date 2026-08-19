# Map View

Source: `src/lib/views/MapView.svelte`.

Architecture-map orchestrator. Converts the project, schedules/cancels worker-backed ELK layout, mounts Svelte Flow only into a visible nonzero canvas, derives trace emphasis, and captures/restores viewport state.

## Inputs and state

- Props: project, selected document, visibility, `onOpenDocument`.
- Layout state: ELK worker, pending project/signature, request queue/session, nodes/edges, versions/revision.
- Flow state: mount/API/init versions, readiness, resize observer, saved viewport, explicit-fit flag.
- Trace state: hovered document, `documentTraceActive` (whether a document is actively hovered/focused), independent pointer/focus edge IDs, connected IDs, live summary.

## Operations

- `fit()` — clears saved viewport and fits now or on next ready mount.
- `registerFlowApi(api,mountVersion,revision)` — stale-safe action bridge registration.
- `handleFlowInit(mountVersion,revision)` — stale-safe Svelte Flow initialization signal.
- `applyInitialViewport()` — once per current mount, restore identical signature/revision or fit.
- `saveCurrentViewport()` — captures valid pan/zoom before hide.
- `scheduleLayout(project,signature,visible)` — 40ms latest-only queue; cancels superseded/hidden work.
- `createElk()` — creates worker-backed ELK engine.
- `cancelActiveLayout()` — terminates worker, recreates engine, invalidates running request.
- `retryLayout()` — queues the failed active signature through normal scheduling.
- `scheduleFlowMount(visible,status,revision)` — saves viewport on hide, invalidates stale mount callbacks, and resets `hoveredDocumentId`/`edgeTraceState` via `resetMapTraceOnFlowUnmount` (see the unmount invariant below).
- `mountFlowWhenSized(version,revision)` — tick/RAF/ResizeObserver gate for nonzero canvas.
- `updateLayout(project)` — projects graph, handles empty, awaits [`layoutMapGraph`](../map/elk-layout.ts.md), and request-gates success/error.
- `traceDocument(id)` — sets the hovered/focused document id. Leaves `edgeTraceState` untouched; document-over-edge priority is applied at projection time (see the invariant below), not by mutating trace state.
- `traceDocumentUnmount(id)` — a [`MapDocumentNode`](../components/MapDocumentNode.svelte.md) tells MapView its own node is being destroyed; clears `hoveredDocumentId` only if it still equals `id`, via `clearMapDocumentTraceIfActive` (see the per-node unmount invariant below).
- `tracePointerEdge(id)`, `traceFocusedEdge(id)` — update the independent edge trace sources.
- `traceSummaryForDocument(...)` — accessible incoming/outgoing summary.
- destroy lifecycle — invalidates versions, observers/timers, viewport, and worker.

`connectedDocuments`, `nodeEmphasis`, `edgeEmphasis`, and `resolveTracedEdge` are pure projections imported from [`map-view-state.ts`](../map/map-view-state.ts.md); the view only wraps `connectedDocuments`'s result in a `SvelteSet` for reactive lookups. The reactive `tracedEdge` is exactly `resolveTracedEdge(edgeTraceState, documentTraceActive, layoutEdges)` — MapView does not reimplement the priority gate inline, it calls the exported function, so tests exercising `resolveTracedEdge` directly are exercising the same code path the component runs.

The reactive `nodes` mapping also passes `activeEdgeId: tracedEdge?.id ?? null` into every document node's `data`, alongside `emphasis`. [`MapDocumentNode`](../components/MapDocumentNode.svelte.md) compares each of its own `MapPort`s' `linkId` against this value to decide which physical port to render as the traced edge's highlighted endpoint — the same `tracedEdge` this file already derives for edge/node emphasis, not a separate computation.

## Invariant: an actively traced edge always renders above document nodes

`SvelteFlow` here uses `zIndexMode="manual"`, and [`elkGraphToFlow`](../map/elk-layout.ts.md) assigns static stacking order at layout time: folders `zIndex: 0`, edges `zIndex: 1`, documents `zIndex: 2`. Under that ordering alone, every document node's opaque box (and its `box-shadow`) permanently sits above every edge, including the short segment of an edge immediately next to its own port — the traced/highlighted portion of a route could visually disappear right where a user would look first, at the node it connects to. The reactive `edges` mapping corrects this for exactly the edge currently being traced: when `edgeEmphasis(...)` resolves to `'active'`, that edge's `zIndex` is overridden to `ACTIVE_EDGE_Z_INDEX` (`3`, above the document layer) instead of its static layout-time value; every other edge keeps the `zIndex` `elkGraphToFlow` gave it, so idle edges still render under nodes as before.

## Invariant: trace source priority without data loss

A document node's pointer or focus trace always wins, in the _projection_, over any concurrently active edge trace — but the edge trace's underlying sources are never discarded to achieve this. `documentTraceActive` (`hoveredDocumentId !== null`) gates `resolveTracedEdge`, which the reactive `tracedEdge` calls: while a document is actively hovered/focused, `resolveTracedEdge` returns `null` regardless of `edgeTraceState`, so `nodeEmphasis`/`edgeEmphasis` see no edge and resolve purely from the document (its own id and one-hop neighbors). This is what stops a pointer-hovered/keyboard-focused edge left behind by earlier interaction (a missed `pointerleave`, a virtualization-driven unmount under `onlyRenderVisibleElements`, or lingering keyboard focus on an off-screen edge button) from ever keeping `nodeEmphasis`/`edgeEmphasis` on the stale edge's source/target pair once a document becomes active — including in graphs with duplicate filenames across folders, where the highlighted set must still be derived from document IDs alone, never from names or paths.

Because `edgeTraceState` itself is never reset by this priority (unlike the superseded approach that called `clearMapEdgeTraceForDocument` from `traceDocument`), a genuinely live source — the pointer still physically over an edge, or keyboard focus still on an edge's hidden button — survives a document's hover/focus window intact. The moment `hoveredDocumentId` reverts to `null` (the document's hover/focus ends), `documentTraceActive` flips back to `false` and the very next reactive pass recomputes `tracedEdge` by calling `resolveTracedEdge` again against the untouched `edgeTraceState`: a still-live edge trace becomes effective again immediately, with no requirement to leave and re-enter/re-focus the edge. The edge keyboard button's `aria-pressed` is bound directly to `edgeTraceState.focusedEdgeId` (not to `tracedEdge`), so it always reflects real focus and never falsely reports `false` while a document is merely taking projection priority.

`clearMapEdgeTraceForDocument` still exists in [`map-view-state.ts`](../map/map-view-state.ts.md) as a pure full-reset reducer used directly by tests as a "no trace can survive" oracle, but `MapView` no longer calls it.

## Invariant: no trace state survives a flow unmount

`hoveredDocumentId` and `edgeTraceState` are driven entirely by DOM callbacks (`onpointerenter`/`onpointerleave`/`onfocus`/`onblur` on document nodes and the hidden edge keyboard buttons) attached to elements owned by the mounted `SvelteFlow` instance. Unmounting those elements — status leaving `ready`, `visible` going `false` (e.g. `Ctrl`/`Cmd`+`1` switching away from the map while a document button is focused), or a fresh `{#key layoutRevision}` remount — does not reliably fire their `onblur`/`onpointerleave` callbacks, so a document's hover/focus can go stale instead of completing its enter/leave cycle. Left alone, `hoveredDocumentId` (and therefore `documentTraceActive`) would still read `true` after the flow remounts, and the trace-priority gate above would keep gating `tracedEdge` to `null` until some _other_ document completes a full enter/leave cycle — masking edge tracing on the freshly remounted map for no visible reason.

`scheduleFlowMount` closes this gap: on every mount-cycle transition it calls `resetMapTraceOnFlowUnmount()` (exported from [`map-view-state.ts`](../map/map-view-state.ts.md)) and assigns both `hoveredDocumentId` and `edgeTraceState` from the result, unconditionally — the same reset a real, fully-completed leave/blur cycle would have produced. This runs alongside the flow-instance-scoped resets (`flowApi`, `flowInitialized`, the resize observer) already in that function, since none of that state can outlive the mount it belongs to either. Because the reset always lands before a new `SvelteFlow` instance can mount (`flowReady` only flips back to `true` later, once `mountFlowWhenSized` confirms a nonzero canvas), a document or edge hover/focus applied right after remount is a clean first interaction, not a continuation of stale state — no unrelated document needs to be hovered first to "flush" it.

## Invariant: a single unmounted document node cannot wedge the trace either

`resetMapTraceOnFlowUnmount` only covers the whole flow being torn down or remounted. `onlyRenderVisibleElements` (set on the `SvelteFlow` instance) can also unmount a single, still-hovered/focused [`MapDocumentNode`](../components/MapDocumentNode.svelte.md) on its own — e.g. the user pans it out of the viewport while it holds pointer or keyboard focus — without the flow itself remounting and without that node's `onblur`/`onpointerleave` ever firing. Left unhandled, `hoveredDocumentId` would stay pinned to that document's ID forever: `documentTraceActive` stays `true`, `resolveTracedEdge` keeps gating `tracedEdge` to `null`, and no other document's genuine enter/leave cycle is guaranteed to happen to overwrite it.

Every `MapDocumentNode` calls `onTraceDocumentUnmount` with its own `documentId` from its `onDestroy` hook, unconditionally, regardless of whether it was hovered/focused at the time. MapView wires this to `traceDocumentUnmount`, which runs it through `clearMapDocumentTraceIfActive(hoveredDocumentId, documentId)`: `hoveredDocumentId` is only cleared when it still equals the destroyed node's own ID. This guard matters because the destroy callback carries no ordering guarantee relative to a new hover — if the user has already moved on to a different document by the time a stale node's destroy callback runs, `hoveredDocumentId` no longer equals that node's ID, so the guard leaves the new, genuinely active trace untouched instead of erasing it.

## Rendering contract

Uses custom [document](../components/MapDocumentNode.svelte.md), [folder](../components/MapFolderNode.svelte.md), and [route](../components/MapRouteEdge.svelte.md) types. HTML edge buttons provide stable keyboard tracing; SVG routes remain pointer-only/presentation.
