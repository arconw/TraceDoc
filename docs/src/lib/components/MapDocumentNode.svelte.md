# Map Document Node

Source: `src/lib/components/MapDocumentNode.svelte`.

Accessible Svelte Flow document node driven by [`MapNodeData`](../map/elk-layout.ts.md).

## Operations

- `openDocument()` — emits the stable document ID through `onOpenDocument`.
- `updateTrace()` — traces while pointer or keyboard focus is active.
- `setHovered(value)` / `setFocused(value)` — maintain independent local interaction sources and recompute trace.
- `ports` — this document's own `data.ports` (`MapPort[]`, empty when it has no incident edges), one noninteractive `Handle` rendered per port at its `side`/`offset`, keyed by `port.id`. This is the explicit port model from [`routing`](../map/routing.ts.md): every edge gets its own physical position rather than sharing one dot per side.
- `SIDE_POSITIONS` — maps a `MapSide` to the `@xyflow/svelte` `Position` the `Handle` needs; the port's `offset` (0–1 along that side) is then applied as an inline `left`/`top` percentage via `portStyle`, since `Position` alone only anchors a handle to its side, not to a position along it.
- `mapHandleId(port)` (from [`elk-layout`](../map/elk-layout.ts.md)) — builds this `Handle`'s `id` (`<direction>-<side>-<index>`); the same helper builds the matching `sourceHandle`/`targetHandle` on the edge, so the two always resolve to the same DOM handle.
- `portClass(port)` / `portStyle(port)` — per-port CSS state and placement. A port whose `linkId` matches `data.activeEdgeId` (the currently hover/keyboard-traced edge, set by [`MapView`](../views/MapView.svelte.md)) gets `map-port-active` — larger, full opacity, the active edge color, in both normal and `forced-colors` themes. Otherwise, while this node itself is `emphasis: 'active'` (hovered, selected, or an endpoint of the traced edge), its own ports get `map-port-emphasized` for a lighter visibility bump. Idle ports stay small and partially visible rather than fully hidden, since a rendered port always represents a real connection; a local `:hover`/`:focus-within` CSS rule on the node also raises all its own ports' opacity immediately, without waiting on the `data.emphasis` round-trip through `MapView`.
- `relationshipSummary` — produces incoming/outgoing accessible text.
- `onDestroy` — calls `onTraceDocumentUnmount` with this node's own `documentId` whenever the component is torn down, hovered/focused or not. This covers `SvelteFlow`'s `onlyRenderVisibleElements`: it can unmount an individual, still-hovered/focused node (panned out of the viewport) without ever firing `onblur`/`onpointerleave`, which is the only other path that would clear its trace. [`MapView`](../views/MapView.svelte.md) wires this to `traceDocumentUnmount`, which only clears the trace if this node's `documentId` is still the active one — see `map-view-state.ts`'s `clearMapDocumentTraceIfActive`.

Emphasis (`normal`, `active`, `connected`, `muted`) and `activeEdgeId` are computed by [`MapView`](../views/MapView.svelte.md), not locally. Ports, like the rest of this node's handles, stay `aria-hidden`/`tabindex="-1"`/noninteractive; keyboard tracing of an edge (and therefore of its ports) is driven entirely through `MapView`'s own hidden edge-focus targets, consistent with how edge hover/focus already works.
