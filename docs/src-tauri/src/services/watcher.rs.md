# Workspace Watcher

Source: `src-tauri/src/services/watcher.rs`.

Generation-bound native recursive filesystem watcher. Debounces/coalesces raw events, reconciles the final filesystem state into one project model, and emits compact revisioned patches.

## Runtime

- `DEBOUNCE` — 120ms coalescing window.
- `ChangeKind` — upsert/remove/rename intent.
- `WorkspaceChange` — normalized relative path and kind.
- `PendingChange` — unioned intent accumulated before final re-stat.
- `ArmedWatcher` — a native watch registered with the OS but not yet delivering to a session; returned by `arm`, consumed by `finish`. Carries `root_identity`, the watched root's filesystem-object identity (device+inode on Unix, volume serial+file index on Windows) captured at `arm` time.
- `WorkspaceWatcher` — stateless (unit struct); carries no watcher of its own. Installation is arbitrated entirely by `WorkspaceSession::install_watcher` under the session's own generation lock, so two overlapping `arm`/`finish` pairs never contend over a shared "active" slot here.
- `WorkspaceWatcher::arm(root)` — registers the native recursive watch on `root` immediately, *before* the caller scans the workspace, so changes made during the scan are queued in the returned `ArmedWatcher` instead of being missed. Deliberately does **not** touch any watcher already installed on the session: if the scan or activation that follows fails, the caller drops the `ArmedWatcher` and the previously active workspace (if any) keeps its own watcher and live updates uninterrupted. Also captures `root`'s filesystem-object identity via `capture_root_identity` for `finish` to revalidate later.
- `WorkspaceWatcher::finish(armed,root,app,session,generation)` — first calls `revalidate_root_identity(armed,root)`: re-captures `root`'s current identity and compares it to `armed.root_identity`. Path-based watch backends (Linux inotify, and Windows' directory-handle-based `ReadDirectoryChangesW`) stay bound to the object they resolved `root` to at watch-registration time, not to whatever the path names afterward. If, between `arm` and `finish`, something external renamed that object away and moved a *different* directory into `root`'s exact canonical path, `armed` is silently watching the vanished original while the scan that ran in between (path-based, so it read whatever was at `root` at scan time) already picked up the replacement — and the root's own rename event doesn't self-heal this, since reconciliation ignores root-relative empty paths. When a mismatch is detected, `armed` is discarded and a fresh watch is armed on `root` in its place (falling back to the stale `armed` if re-arming itself fails, rather than leaving no watcher at all); if identity can't be determined on either side, no divergence is assumed and `armed` is kept as-is. **This is a narrow, low-probability race** — it requires an external directory swap at the exact canonical path during the scan window — not a concern in ordinary single-writer use. Only then does `finish` drain whatever `arm` buffered (both changes *and* watcher errors — discarded if a re-arm just happened, since they belong to the superseded object), then calls `session.install_watcher(generation, watcher)` to install atomically, gated on `generation` still being current. If a newer request has already activated a later generation (two overlapping `open_workspace` calls), installation is rejected and this watcher — along with everything buffered for it — is torn down without disturbing the watcher the newer request installed; the caller's subsequent `session.snapshot(generation)` surfaces that loss to the stale caller. On success, buffered changes are folded in via `apply_external_changes` *first*, then buffered errors are emitted as `workspace-watch-error` (matching what `watch_loop` emits for the same errors live), reading `session.current_revision` only after that reconciliation; a buffered error alongside a nonempty patch is therefore stamped with the revision the patch just advanced to, not the pre-reconciliation value — the frontend's `applyWorkspaceError` in [`project-state.ts`](../../../src/lib/stores/project-state.ts.md) discards an error whose revision is older than the workspace's current revision as stale, so stamping it with the value that is about to be superseded would silently swallow the warning. Finally `watch_loop` is spawned for live delivery.
- `capture_root_identity(root)` — best-effort, cross-platform (Unix and Windows; `None` elsewhere) filesystem-object identity probe for `root`, following symlinks/reparse points the same way the native watch backends resolve the path. Mirrors the identity-comparison pattern [`services/document.rs`](document.rs.md) already uses for save-path revalidation (`unix_file_identity`/`windows_file_identity`/`windows_path_identity`), applied here to the workspace root instead of a document's save path.
- `watch_loop(...)` — receives notify events, coalesces by path over the debounce window, applies a nonempty batch of changes first (emitting `workspace-patch` or, on a non-generation apply failure, `workspace-watch-error`), then emits any watcher error batched alongside those changes — reading the revision only after that reconciliation, for the same stale-error reason as `finish` above. A batch whose apply fails because the workspace generation moved on returns from the loop without emitting the batch's watcher error, since the frontend has already moved to a different generation and would discard it by generation regardless.
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
- A stale `finish` from an earlier, slower `open_workspace` request that arrives after a later request has already activated and installed its own watcher is a no-op: the later (winning) workspace's watcher keeps delivering live updates.
- A failed reopen — armed but never activated/finished — leaves the existing active workspace's watcher untouched and still live.
- Errors buffered by the native backend during the scan window (before `finish` drains the channel) reach `workspace-watch-error`, the same notification `watch_loop` emits for live errors, instead of being silently dropped.
- A buffered error paired with a scan-window change that produces a nonempty patch is stamped, once `finish` returns, with the post-reconciliation revision — not the pre-reconciliation one `apply_external_changes` is about to bump past.
- A live debounce batch that coalesces a watcher error together with a real filesystem change is, by the same reasoning, stamped by `watch_loop` with the revision the accompanying patch just advanced to.
- (Unix-only) If the directory `arm` watched is renamed away and a different directory is created at that exact same path before `finish` runs, `finish` re-arms on the replacement instead of installing a watcher still bound to the renamed-away original — live changes to the replacement are observed, not silently dropped.

Test functions: `incrementally_creates_modifies_and_deletes_documents`, `incrementally_reconciles_folder_moves_and_ignores_hidden_content`, `ignores_events_outside_the_workspace`, `contradictory_events_converge_to_the_final_filesystem_state`, `file_and_folder_type_flips_converge_to_a_full_scan`, `native_watcher_observes_markdown_creation_without_polling`, `reconciles_changes_made_between_arming_the_watcher_and_the_initial_scan`, `a_slow_stale_finish_does_not_disturb_a_faster_winning_watcher`, `a_failed_reopen_does_not_disturb_the_existing_active_watcher`, `buffered_watcher_errors_from_the_scan_window_reach_workspace_watch_error`, `buffered_watcher_errors_paired_with_a_nonempty_patch_are_stamped_with_the_post_reconciliation_revision`, `watch_loop_stamps_a_live_batch_error_with_the_post_reconciliation_revision_when_paired_with_a_nonempty_patch`, `incremental_reconcile_does_not_follow_symlink_components`, `finish_re_arms_when_the_watched_root_is_replaced_at_the_same_path_during_the_scan_window`.

Shares policies with [`workspace`](workspace.rs.md) and mutations with [`WorkspaceSession`](document.rs.md).
