# Workspace Status View

Source: `src/lib/views/WorkspaceStatus.svelte`.

Root state renderer over [`projectStore`](../stores/project.ts.md).

- `empty` — No folder open.
- `loading` — opening progress.
- `error` — actionable store message.
- `loaded` — root path and folder/document counts.

No methods or mutations. `aria-live=polite` announces transitions.
