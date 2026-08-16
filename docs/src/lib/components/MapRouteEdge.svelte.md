# Map Route Edge

Source: `src/lib/components/MapRouteEdge.svelte`.

Custom orthogonal polyline renderer for [`MapEdgeData`](../map/elk-layout.ts.md).

## Operations

- Reactive `path` serializes route points into SVG `M/L` commands.
- `setHovered(value)` updates pointer state.
- `updateTrace()` emits this edge ID or null through `onTracePointerEdge`.

Renders a visible path plus a wider transparent hit path. Edge keyboard controls are owned by [`MapView`](../views/MapView.svelte.md); SVG paths stay out of the accessibility tree.
