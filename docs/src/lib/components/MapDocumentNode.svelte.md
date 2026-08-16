# Map Document Node

Source: `src/lib/components/MapDocumentNode.svelte`.

Accessible Svelte Flow document node driven by [`MapNodeData`](../map/elk-layout.ts.md).

## Operations

- `openDocument()` — emits the stable document ID through `onOpenDocument`.
- `updateTrace()` — traces while pointer or keyboard focus is active.
- `setHovered(value)` / `setFocused(value)` — maintain independent local interaction sources and recompute trace.
- `ports` — declares noninteractive source/target handles on all four sides for routed edges.
- `relationshipSummary` — produces incoming/outgoing accessible text.

Emphasis (`normal`, `active`, `connected`, `muted`) is computed by [`MapView`](../views/MapView.svelte.md), not locally.
