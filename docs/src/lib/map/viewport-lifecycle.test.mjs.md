# Viewport Lifecycle Tests

Source: `src/lib/map/viewport-lifecycle.test.mjs`.

## Fixture

- fake `flow` records `fit`, `restore`, and current viewport calls.
- `viewport` is a nontrivial valid pan/zoom value.
- `apply(action)` executes the selected mount action.

## Cases

- Capture and exact zero-duration restore for the same graph revision.
- Changed revision/signature or explicit request selects fit.
- Invalid viewports and stale/hidden remount callbacks are rejected.

Target: [`viewport-lifecycle.ts`](viewport-lifecycle.ts.md).
