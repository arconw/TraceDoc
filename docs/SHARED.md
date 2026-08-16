# Shared API Registry

Use this registry before introducing cross-feature code. Feature-local helpers are not listed.

## Frontend contracts

| API                     | Source                                                          | Contract                                                                            | Consumers                               |
| ----------------------- | --------------------------------------------------------------- | ----------------------------------------------------------------------------------- | --------------------------------------- |
| `AppInfo`               | [`types/app.ts`](src/lib/types/app.ts.md)                       | Runtime product identity DTO.                                                       | App identity bootstrap.                 |
| Workspace DTOs          | [`types/workspace.ts`](src/lib/types/workspace.ts.md)           | Canonical frontend shape of Rust snapshots, patches, documents, and links.          | Store, editor, sidebar, inspector, map. |
| `projectStore`          | [`stores/project.ts`](src/lib/stores/project.ts.md)             | Single Tauri-facing workspace state owner.                                          | App, workspace view/status, editor.     |
| Project reducers        | [`stores/project-state.ts`](src/lib/stores/project-state.ts.md) | Pure generation/revision-gated snapshot, patch, save, error, and close transitions. | `projectStore`; reducer tests.          |
| Lazy-view state helpers | [`utils/ui-behavior.ts`](src/lib/utils/ui-behavior.ts.md)       | Reject stale lazy imports and derive hidden/loading/ready/error presentation.       | Workspace map loader.                   |
| `saveShortcutAction`    | [`utils/ui-behavior.ts`](src/lib/utils/ui-behavior.ts.md)       | Shared Mod-S/Shift/modal policy.                                                    | Editor and workspace shortcut layers.   |
| `mapFitDuration`        | [`utils/ui-behavior.ts`](src/lib/utils/ui-behavior.ts.md)       | Reduced-motion-aware fit duration.                                                  | Svelte Flow action bridge.              |
| UI preference helpers   | [`utils/ui-preferences.ts`](src/lib/utils/ui-preferences.ts.md) | Validate, clamp, load, and persist active view/sidebar width.                       | Workspace shell.                        |
| `applyAppInfo`          | [`utils/app-identity.ts`](src/lib/utils/app-identity.ts.md)     | Applies backend product name/version to header and window title.                    | Root app.                               |
| Theme tokens            | [`styles/theme.css`](src/lib/styles/theme.css.md)               | Global semantic color, spacing, typography, map, focus, and status tokens.          | All frontend UI.                        |

There is currently no generic shared visual component. Components under `components` are owned by editor, workspace-tree, index-inspector, or map behavior; reuse them only within those contracts.

## Backend contracts

| API                                      | Source                                                            | Contract                                                                                                 | Consumers                                             |
| ---------------------------------------- | ----------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| Workspace model types                    | [`models/workspace.rs`](src-tauri/src/models/workspace.rs.md)     | Serialized canonical project, snapshot, patch, read, save, and error DTOs.                               | Commands and all services.                            |
| `WorkspaceSession` / `WorkspaceLease`    | [`services/document.rs`](src-tauri/src/services/document.rs.md)   | Generation/revision authority, history, optimistic I/O, save serialization, and external reconciliation. | Document/workspace commands and watcher.              |
| `scan_workspace`                         | [`services/workspace.rs`](src-tauri/src/services/workspace.rs.md) | Deterministic full workspace scan and initial index.                                                     | Workspace open/refresh and watcher convergence tests. |
| Ignore/path policy                       | [`services/workspace.rs`](src-tauri/src/services/workspace.rs.md) | Shared hidden/system/symlink/relative-path rules.                                                        | Scanner and incremental watcher.                      |
| Markdown index APIs                      | [`services/markdown.rs`](src-tauri/src/services/markdown.rs.md)   | Full and single-document heading/link indexing with canonical resolution.                                | Scanner, save, watcher.                               |
| `apply_project_changes` / `diff_project` | [`services/watcher.rs`](src-tauri/src/services/watcher.rs.md)     | Incremental filesystem reconciliation and compact model diff.                                            | Native watcher and session refresh/save flows.        |

## Reuse rule

Add an API here only when at least two feature boundaries consume it or when it is the canonical serialized/system boundary. Shared code must not depend on feature UI or feature orchestration.
