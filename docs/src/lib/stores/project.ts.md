# Project Store

Source: `src/lib/stores/project.ts`.

Single frontend owner of workspace state and Tauri-facing side effects. Pure transitions are delegated to [`project-state`](project-state.ts.md).

## Operations

- `ProjectStoreDependencies` — injectable `open`/`invoke`/`listen` boundary, defaulted to the real `@tauri-apps/plugin-dialog` and `@tauri-apps/api` bindings; lets tests substitute the native dialog/IPC layer.
- `errorMessage(error)` — normalizes unknown failures.
- `createProjectStore(dependencies?)` — creates the discriminated Svelte writable and returns actions:
  - `subscribe` — standard store subscription.
  - `openFolder()` — native directory dialog, loading state, `open_workspace` invoke, buffered-event completion. A `null` picker result (cancel) or a picker error leaves the current state as-is/reports an actionable error without ever entering `loading`.
  - `selectDocument(documentId)` — validates existence and updates the one selected ID.
  - `closeDocument(documentId)` — delegates guarded close reducer.
  - `applyDocumentIndex(update)` — applies revisioned save/index response.
  - `refreshWorkspace()` — captures generation/revision, invokes refresh, gates result/error.
  - `listenForWorkspaceUpdates()` — subscribes to `workspace-patch` and `workspace-watch-error`; returns combined unlisten.
- `projectStore` — singleton (`createProjectStore()` with the real dependencies) consumed by root, workspace, editor, and status views.

Invariant: no component owns a second ProjectModel copy.

Tests: [`project.test.mjs`](project.test.mjs.md) exercises `openFolder()` against injected dependencies.
