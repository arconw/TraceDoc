# Map Document Node

Source: `src/lib/components/MapDocumentNode.svelte`.

Accessible Svelte Flow document node driven by [`MapNodeData`](../map/elk-layout.ts.md).

## Operations

- `openDocument()` — emits the stable document ID through `onOpenDocument`.
- `updateTrace()` — traces while pointer or keyboard focus is active.
- `setHovered(value)` / `setFocused(value)` — maintain independent local interaction sources and recompute trace.
- `ports` — declares noninteractive source/target handles on all four sides for routed edges.
- `relationshipSummary` — produces incoming/outgoing accessible text.
- `onDestroy` — calls `onTraceDocumentUnmount` with this node's own `documentId` whenever the component is torn down, hovered/focused or not. This covers `SvelteFlow`'s `onlyRenderVisibleElements`: it can unmount an individual, still-hovered/focused node (panned out of the viewport) without ever firing `onblur`/`onpointerleave`, which is the only other path that would clear its trace. [`MapView`](../views/MapView.svelte.md) wires this to `traceDocumentUnmount`, which only clears the trace if this node's `documentId` is still the active one — see `map-view-state.ts`'s `clearMapDocumentTraceIfActive`.

Emphasis (`normal`, `active`, `connected`, `muted`) is computed by [`MapView`](../views/MapView.svelte.md), not locally.
