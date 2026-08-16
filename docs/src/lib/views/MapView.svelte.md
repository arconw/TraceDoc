# Map View

Source: `src/lib/views/MapView.svelte`.

Architecture-map orchestrator. Converts the project, schedules/cancels worker-backed ELK layout, mounts Svelte Flow only into a visible nonzero canvas, derives trace emphasis, and captures/restores viewport state.

## Inputs and state

- Props: project, selected document, visibility, `onOpenDocument`.
- Layout state: ELK worker, pending project/signature, request queue/session, nodes/edges, versions/revision.
- Flow state: mount/API/init versions, readiness, resize observer, saved viewport, explicit-fit flag.
- Trace state: hovered document, independent pointer/focus edge IDs, connected IDs, live summary.

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
- `scheduleFlowMount(visible,status,revision)` — saves viewport on hide and invalidates stale mount callbacks.
- `mountFlowWhenSized(version,revision)` — tick/RAF/ResizeObserver gate for nonzero canvas.
- `updateLayout(project)` — projects graph, handles empty, awaits [`layoutMapGraph`](../map/elk-layout.ts.md), and request-gates success/error.
- `traceDocument(id)`, `tracePointerEdge(id)`, `traceFocusedEdge(id)` — update trace sources.
- `connectedDocuments(edges,id)` — derives adjacent document set.
- `nodeEmphasis(...)` / `edgeEmphasis(...)` — active/connected/muted projection.
- `traceSummaryForDocument(...)` — accessible incoming/outgoing summary.
- destroy lifecycle — invalidates versions, observers/timers, viewport, and worker.

## Rendering contract

Uses custom [document](../components/MapDocumentNode.svelte.md), [folder](../components/MapFolderNode.svelte.md), and [route](../components/MapRouteEdge.svelte.md) types. HTML edge buttons provide stable keyboard tracing; SVG routes remain pointer-only/presentation.
