# Map Route Edge

Source: `src/lib/components/MapRouteEdge.svelte`.

Custom orthogonal polyline renderer for [`MapEdgeData`](../map/elk-layout.ts.md), including its repeated direction chevrons and non-connected crossing gaps.

## Operations

- Reactive `interactionPath` serializes the route's raw, unbroken points into SVG `M/L` commands — this is what the wide invisible hit path uses, so hover/keyboard tracing stays correct across a visual crossing gap.
- Reactive `visiblePath` (`buildVisiblePath(points, crossingGaps)`) is the same points but with a small literal break cut into the path at each of `data.crossingGaps`: the two path fragments on either side of a gap are emitted as separate `M/L` subpaths, so the visible line truly has no stroke there rather than being masked. Because the "under" line simply never paints those pixels, whichever edge is drawn "over" always shows through the gap correctly regardless of SVG paint order between the two edges. `segmentGaps`/`isOnSegment` locate which of the route's own segments each gap point belongs to (by exact axis + strict interior range, since a gap is never at a segment endpoint); `shiftAlongSegment`/`clampedHalfWidth` compute the two break points around it, clamped so the gap can never overrun a very short segment.
- Reactive `chevrons` is `data.chevrons` (or `[]`); each renders as a small `<path>` (`CHEVRON_SHAPE`) translated to `chevron.point` and rotated by `CHEVRON_ANGLES[chevron.direction]` (`right`/`down`/`left`/`up` → `0/90/180/270`) — a separate small SVG element per marker, not the `marker-end` mechanism, since SVG markers only attach at path vertices/ends, not arbitrary interior points. Chevrons are `aria-hidden`, non-interactive, and styled smaller/dimmer than the final `markerEnd` arrowhead; they still respond to the edge's own `active`/`muted` emphasis classes.
- `setHovered(value)` updates pointer state.
- `updateTrace()` emits this edge ID or null through `onTracePointerEdge`.

Renders a visible (possibly gapped) path, its chevrons, plus a wider transparent hit path that always follows the full, ungapped route. Edge keyboard controls are owned by [`MapView`](../views/MapView.svelte.md); SVG paths stay out of the accessibility tree.
