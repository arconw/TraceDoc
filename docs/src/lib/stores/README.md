# Stores

Single frontend workspace authority and pure state transitions.

- [`project.ts`](project.ts.md) — native dialog, IPC, event subscriptions, imperative actions.
- [`project.test.mjs`](project.test.mjs.md) — open/cancel/error state-transition regressions against injected dialog/IPC dependencies.
- [`project-state.ts`](project-state.ts.md) — pure generation/revision reducers.
- [`project-state.test.mjs`](project-state.test.mjs.md) — ordering, stale-response, open, close, and patch regressions.

All editor/sidebar/map consumers derive from the same loaded `ProjectState`.
