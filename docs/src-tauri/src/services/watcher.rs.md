# Workspace Watcher

Source: `src-tauri/src/services/watcher.rs`.

Generation-bound native recursive filesystem watcher. Debounces/coalesces raw events, reconciles the final filesystem state into one project model, and emits compact revisioned patches.

## Runtime

- `DEBOUNCE` — 120ms coalescing window.
- `ChangeKind` — upsert/remove/rename intent.
- `WorkspaceChange` — normalized relative path and kind.
- `PendingChange` — unioned intent accumulated before final re-stat.
- `WorkspaceWatcher::start(app,session,root,generation)` — replaces previous watcher and spawns worker.
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
- `diff_project(before,after,generation,revision,externallyChanged)` — compact canonical `WorkspacePatch`.
- `is_markdown(path)`, `first_symlink_component(root,relative)`, `parent_path(path)`, `folder_id(path)`, `document_id(path)` — policy/identity helpers.

## Tests

- `TestWorkspace::new/path/change/drop` fixture.
- Create/modify/delete with link re-resolution.
- Folder moves, hidden content, outside-root rejection.
- Contradictory events converge to final stat.
- File↔folder type flips match full scan.
- Real native creation event without polling.
- Incremental reconciliation never follows symlink ancestors.

Test functions: `incrementally_creates_modifies_and_deletes_documents`, `incrementally_reconciles_folder_moves_and_ignores_hidden_content`, `ignores_events_outside_the_workspace`, `contradictory_events_converge_to_the_final_filesystem_state`, `file_and_folder_type_flips_converge_to_a_full_scan`, `native_watcher_observes_markdown_creation_without_polling`, `incremental_reconcile_does_not_follow_symlink_components`.

Shares policies with [`workspace`](workspace.rs.md) and mutations with [`WorkspaceSession`](document.rs.md).
