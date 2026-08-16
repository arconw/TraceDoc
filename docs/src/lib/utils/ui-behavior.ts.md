# Shared UI Behavior

Source: `src/lib/utils/ui-behavior.ts`.

Cross-feature pure policies listed in [`SHARED`](../../../SHARED.md).

## Lazy view

- `LazyViewStatus`, `LazyViewState` — request ID/status/message.
- `createLazyViewState()` — idle state.
- `beginLazyViewLoad(state)` — increments request and enters loading.
- `completeLazyViewLoad(state,id)` / `failLazyViewLoad(state,id,message)` — stale-request-safe terminal transitions.
- `lazyViewPresentation(active,state)` — hidden/loading/ready-hidden/ready-visible/error rendering state.
- `lazyViewShouldLoad(state)` — true only before first load.

## Shortcuts and motion

- `SaveShortcutModifiers` — testable keyboard subset.
- `saveShortcutAction(event,enabled)` — ignore unrelated, block Shift/disabled, otherwise save.
- `mapFitDuration(reducedMotion)` — `0` or `180ms`.
