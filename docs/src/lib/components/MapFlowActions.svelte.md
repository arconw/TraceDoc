# Map Flow Actions

Source: `src/lib/components/MapFlowActions.svelte`.

Bridge from the Svelte Flow context to [`MapView`](../views/MapView.svelte.md).

## API

- `MapFlowApi.fit()` — fits all nodes with fixed padding/max zoom and reduced-motion-aware duration.
- `MapFlowApi.getViewport()` — reads `{x,y,zoom}`.
- `MapFlowApi.restore(viewport)` — restores immediately with zero animation.
- `update()` — tracks the `prefers-reduced-motion` media query; registered/unregistered in mount lifecycle.
- Reactive `onReady(api)` — publishes wrappers when the flow context is available.

Uses [`mapFitDuration`](../utils/ui-behavior.ts.md).
