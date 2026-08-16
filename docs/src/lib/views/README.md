# Views

Feature orchestration components.

- [`WorkspaceStatus.svelte`](WorkspaceStatus.svelte.md) — root empty/loading/error/summary states.
- [`WorkspaceView.svelte`](WorkspaceView.svelte.md) — sidebar/editor/map shell, navigation guard, lazy map, preferences.
- [`MapView.svelte`](MapView.svelte.md) — worker layout, Svelte Flow lifecycle, trace emphasis, viewport state.

Views compose components and pure modules; they do not duplicate ProjectModel state.
