# Workspace Watcher

Source: `src-tauri/src/services/watcher.rs`.

Generation-bound native recursive filesystem watcher. Debounces/coalesces raw events, reconciles the final filesystem state into one project model, and emits compact revisioned patches.

## Runtime

- `DEBOUNCE` — 120ms coalescing window.
- `ChangeKind` — upsert/remove/rename intent.
- `WorkspaceChange` — normalized relative path and kind.
- `PendingChange` — unioned intent accumulated before final re-stat.
- `ArmedWatcher` — a native watch registered with the OS but not yet delivering to a session; returned by `arm`, consumed by `finish`.
- `WorkspaceWatcher::arm(root)` — replaces any previous watcher and registers the native recursive watch on `root` immediately, *before* the caller scans the workspace, so changes made during the scan are queued in the returned `ArmedWatcher` instead of being missed.
- `WorkspaceWatcher::finish(armed,app,session,generation)` — drains whatever `arm` buffered and folds it into the just-activated session via `apply_external_changes` (closing the scan-to-watch race without a second full scan), then spawns `watch_loop` for live delivery.
- `watch_loop(...)` — receives notify events, coalesces by path, applies changes under generation lease, emits `workspace-patch` or `workspace-watch-error`.
- `normalize_event(event)` — maps notify create/modify/remove/rename records into workspace changes.

## Incremental reconciliation

- `apply_project_changes(root,project,changes,generation)` — final-stat reconcile, selective parse/re-resolution, hierarchy rebuild, and patch result.
- `reconcile_directory(...)` / `scan_directory(...)` — add/update a subtree using full-scan ignore/symlink policy.
- `ensure_folder_chain(project,relative)` — creates missing normalized ancestors.
- `upsert_document(project,relative)` — inserts/updates one Markdown record.
- `remove_path(project,relative)` — removes matching document/folder subtree.
- `rebuild_hierarchy(project)` — recomputes ordered child/document lists.
- `reindex_path(root,project,path,generation)` — refreshes changed document metadata/links.
- `diff_project(before,after,generation,externallyChangedDocumentIds)` — compact canonical `WorkspacePatch`; revision is set by the caller afterward.
- `is_markdown(path)`, `first_symlink_component(root,relative)`, `parent_path(path)`, `folder_id(path)`, `document_id(path)` — policy/identity helpers.

## Tests

- `TestWorkspace::new/path/change/drop` fixture.
- Create/modify/delete with link re-resolution.
- Folder moves, hidden content, outside-root rejection.
- Contradictory events converge to final stat.
- File↔folder type flips match full scan.
- Real native creation event without polling.
- Changes made between `arm` and the (simulated) initial scan are still reconciled by `finish`, before a manual refresh or any later event.
- Incremental reconciliation never follows symlink ancestors.

Test functions: `incrementally_creates_modifies_and_deletes_documents`, `incrementally_reconciles_folder_moves_and_ignores_hidden_content`, `ignores_events_outside_the_workspace`, `contradictory_events_converge_to_the_final_filesystem_state`, `file_and_folder_type_flips_converge_to_a_full_scan`, `native_watcher_observes_markdown_creation_without_polling`, `reconciles_changes_made_between_arming_the_watcher_and_the_initial_scan`, `incremental_reconcile_does_not_follow_symlink_components`.

Shares policies with [`workspace`](workspace.rs.md) and mutations with [`WorkspaceSession`](document.rs.md).
