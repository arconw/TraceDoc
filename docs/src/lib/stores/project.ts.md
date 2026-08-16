# Project Store

Source: `src/lib/stores/project.ts`.

Single frontend owner of workspace state and Tauri-facing side effects. Pure transitions are delegated to [`project-state`](project-state.ts.md).

## Operations

- `errorMessage(error)` — normalizes unknown failures.
- `createProjectStore()` — creates the discriminated Svelte writable and returns actions:
  - `subscribe` — standard store subscription.
  - `openFolder()` — native directory dialog, loading state, `open_workspace` invoke, buffered-event completion.
  - `selectDocument(documentId)` — validates existence and updates the one selected ID.
  - `closeDocument(documentId)` — delegates guarded close reducer.
  - `applyDocumentIndex(update)` — applies revisioned save/index response.
  - `refreshWorkspace()` — captures generation/revision, invokes refresh, gates result/error.
  - `listenForWorkspaceUpdates()` — subscribes to `workspace-patch` and `workspace-watch-error`; returns combined unlisten.
- `projectStore` — singleton consumed by root, workspace, editor, and status views.

Invariant: no component owns a second ProjectModel copy.
