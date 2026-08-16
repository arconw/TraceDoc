# Map Viewport Lifecycle

Source: `src/lib/map/viewport-lifecycle.ts`.

Pure capture/restore policy for Svelte Flow remounts.

## API

- `MapViewport` — `{x,y,zoom}`.
- `SavedMapViewport` — viewport keyed by graph signature and layout revision.
- `MapViewportMountAction` — exact restore or fit.
- `validMapViewport(viewport)` — finite coordinates and positive zoom.
- `captureMapViewport(signature,revision,viewport)` — clones valid state or returns null.
- `mapViewportMountAction(saved,signature,revision,explicitFit)` — restores only identical layout; otherwise fits.
- `mapViewportRequestIsCurrent(requestMount,requestRevision,currentMount,currentRevision,visible)` — rejects hidden/stale callbacks.

Consumers: [`MapView`](../views/MapView.svelte.md), [`MapFlowActions`](../components/MapFlowActions.svelte.md).
