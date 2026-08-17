# Document Session and Safe I/O

Source: `src-tauri/src/services/document.rs`.

Authoritative workspace session, optimistic document I/O, revision history, watcher reconciliation, and cross-platform recovery transaction implementation. Commands and watcher share one lock-protected state.

## Session model

- `HISTORY_LIMIT` — 512 contiguous patches retained for save catch-up.
- `RECOVERY_DIRECTORY`, `RECOVERY_OWNER_SENTINEL`, `RECOVERY_OWNER_MAGIC` — private recovery namespace ownership contract.
- `WorkspaceSession` — cloneable `RwLock<SessionState>` owner.
- `SessionState` — generation counter and optional active workspace.
- `ActiveWorkspace` — canonical root/project/revision/history/self-write state, plus the currently installed native `watcher` (if any). Keeping the watcher inside the same lock-protected struct as `generation`/`root` is what lets `install_watcher` gate installation on the generation atomically, and lets a fresh `activate` (which replaces this struct wholesale) be the only thing that ever retires a previous watcher.
- `SelfWrite` — internal-save echo suppression token.
- `WorkspaceLease` — captured root+generation used before blocking work.

### `WorkspaceSession` methods

- `current_revision(generation)` — returns active revision only for matching generation.
- `activate(root,project)` — cleans owned crash artifacts, increments generation, installs new active state at revision one (with no watcher yet). Replacing `SessionState.workspace` here is what drops any previous generation's watcher — and only happens once scanning the new root has already succeeded.
- `install_watcher(expectedGeneration,watcher)` — installs a native watcher for the active workspace, gated atomically (same write lock as `activate`) on `expectedGeneration` still being current; rejects without touching session state if a newer `activate` already won the race, so a slow, stale `WorkspaceWatcher::finish` can never displace a faster, later request's watcher.
- `snapshot(expectedGeneration)` — returns the currently active project/revision as a `WorkspaceSnapshot` directly from session state, without rescanning disk.
- `capture(expectedGeneration)` — returns immutable root/generation lease or workspace-changed error.
- `read_document(lease,path)` — read-lock validates lease, reads exact content/token, returns current revision.
- `save_document(lease,path,content,expectedToken,expectedRevision)` — write-lock optimistic save, clone/reindex, precompute revision/history/catch-up, durable commit, publish state, set watcher suppression.
- `acknowledge_save_conflict(lease,path)` — generation/path-gated cleanup of bounded quarantine/recovery state.
- `apply_external_changes(generation,changes)` — applies incremental watcher reconciliation, suppresses matching self-write echo, publishes next patch/revision.
- `refresh_workspace(generation)` — scans outside the session write lock, rechecks revision, diffs/publishes a monotonic snapshot.

### Revision helpers

- `push_history(workspace,patch)` — append and cap at `HISTORY_LIMIT`.
- `ensure_history_available(workspace,expectedRevision)` — rejects future/evicted baseline.
- `patches_from_history(workspace,expectedRevision,nextPatch)` — returns contiguous catch-up through the proposed save.
- `validate_workspace(state,generation,root)` — shared generation/root predicate.

## Errors and public file API

- `DocumentError` — string error; `new`, `workspace_changed`, `fmt`/`Display`, and `Error` implementations.
- `open_workspace(root)` — resolves/canonicalizes root then scans it (via [`resolve_workspace_root`/`scan_canonical_root`](workspace.rs.md)), returning the project paired with its canonical root.
- free `read_document(root,path)` — validated direct read.
- free `write_document(root,path,content)` — direct optimistic write using current disk token.
- `write_document_if_current(...)` / `write_document_if_current_with_hook(...)` — resolve/read/token-check, preserve line endings, transactional replace, return token/warning; hook enables deterministic race tests.
- `WrittenDocument` — resulting token and optional durability/cleanup warning.
- `external_change_error()` — normalized conflict error.

## Recovery ownership and startup cleanup

- `remove_empty_workspace_recovery_directory(root)` — removes owned sentinel/directory only when isolated and empty.
- `cleanup_workspace_recovery_directory(root)` — all-or-nothing preflight; only valid private owned regular artifacts are removed.
- `is_safe_recovery_directory_metadata` — platform-specific real-directory/private-owner/reparse policy.
- `is_safe_recovery_file_metadata` — platform-specific regular/no-reparse/private-owner policy.
- `is_safe_recovery_owner_metadata` — validates sentinel ownership/type/mode.
- `is_owned_recovery_entry_name(name)` — accepts deterministic lowercase-hash `.recovery/.staging/.lock/.external/.blocked` plus sentinel.
- `acknowledge_transaction_quarantine` — Unix and Windows implementations for explicit removal after UI resolution.
- `remove_acknowledged_slot(directory,name)` — Unix no-follow identity-checked removal.
- `remove_windows_acknowledged_slot(path)` — Windows no-reparse regular-file removal.

## Transaction entry points

- `transactional_replace` — cfg-selected Linux, macOS, Windows, or unsupported-platform implementation.
- `TransactionPhase` — deterministic hook phases: recovery prepared, staging prepared, pre-exchange, installed, pre-restore.
- `transactional_replace_with_phase_hook` — Linux transaction state machine with injected race hook.
- `RecoveredMutationError` + `Display` — recovery-gated mutation failure stage.
- `run_recovered_mutation(recoveryReady,mutate,verify)` — shared Windows/macOS rule: never mutate without recovery; surface write/verification failure.

## Windows transaction

- Windows `transactional_replace` — holds ancestor/target/recovery/lock handles, persists recovery first, mutates with bounded deterministic artifacts, post-verifies identity/content/root/parent.
- `WindowsRecoveryArtifact` — directory/path/identity, lock, slot file/identity/bytes, root identity and relative path.
- `persist_windows_recovery(...)` — creates/opens owned recovery directory, serializes transaction lock, classifies/refreshes/installs deterministic artifact.
- `ensure_windows_recovery_owner(directory,created)` — creates or validates sentinel and rejects reparse ownership changes.
- `cleanup_successful_windows_transaction(...)` — verifies handles/paths then removes staging/recovery/lock and empty owned directory.
- `windows_recovery_matches(artifact)` — re-derives directory and recovery-slot identity from their current on-disk paths (not the held handles) and compares them against the captured baseline alongside slot bytes, so a slot replaced out from under the held handle is detected instead of trivially matching itself.
- `windows_recovery_message(...)` — user-facing verified recovery reference.
- `windows_move_file(source,destination)` — write-through `MoveFileExW` without replacement.
- `acquire_windows_transaction_lock(directory,path)` — deterministic no-share-write/delete lock file plus `LockFileEx`.
- `lock_windows_handle(file)` — nonblocking exclusive Win32 byte-range lock.
- `WindowsInstallGuard::new` / `Drop` — removes only still-owned partial install path.
- `recreate_windows_recovery(...)` — reinstalls held bytes to bounded normal/conflict path.
- `WindowsAncestorGuard::open(path)` / `matches()` — held no-reparse ancestor chain and path identity verification.
- `windows_path_identity(path)` — opens path with backup semantics/open-reparse-point and reads handle identity.
- `windows_file_identity(file)` — stable `GetFileInformationByHandle` volume serial + 64-bit file index.
- `read_file_bytes_portable(file)` — rewind/read helper.

## Recovery artifact format

- `RECOVERY_MAGIC` — `TDREC001`.
- `encode_recovery_artifact(path,timestamp,content)` — magic + normalized path + timestamp + length + BLAKE3 + exact bytes.
- `decode_recovery_artifact(bytes)` — strict bounds/UTF-8/hash validation; returns path/timestamp/content.
- `recovery_slot_name(path)` — deterministic BLAKE3 `.recovery`.
- `recovery_auxiliary_name(path,suffix)` — deterministic `.lock/.staging/.external/.blocked`.
- `recovery_sibling_name(slot,suffix)` — derives bounded sibling from valid hash slot.
- `RecoveryReferenceTarget` / `recovery_reference_target(pathMatches,pathAbsent)` — current/absent/substituted reference decision.

## Unix recovery and ownership

- `RecoveryArtifact` — held root/recovery/lock/file identities and encoded bytes.
- `OpenedRecoverySlot` — classified existing slot file/bytes/identity.
- `held_recovery_matches(artifact)` — held/path/byte consistency.
- `recovery_timestamp()` — Unix epoch seconds.
- `open_recovery_directory(rootFd)` — open/create private `.tracedoc-recovery` relative to held root.
- `ensure_unix_recovery_owner(directory,created)` — effective UID, mode 0700, regular sentinel, magic, sync.
- `acquire_unix_transaction_lock(directory,path)` — deterministic no-follow regular lock with exclusive nonblocking flock.
- `cleanup_successful_unix_transaction(...)` — identity-checked artifact/lock removal and empty owned directory cleanup.
- `remove_unix_transaction_entry(directory,name,expectedIdentity)` — no-follow owned unlink.
- `recovery_artifact_matches(directory,artifact)` — held/path identity and byte token.
- `open_regular_recovery_slot(directory,name)` — no-follow/nonblocking regular classification and lock.
- `preserve_unknown_recovery_slot(directory,slotName)` — moves unknown entry to bounded `.conflict` with no replacement.
- `refresh_recovery_artifact(...)` — rewrites recognized held slot before target mutation.
- `persist_recovery_artifact(...)` — Linux/macOS cfg wrappers.
- `persist_recovery_artifact_with_hook(...)` — creates/refreshes self-contained artifact with race hook.
- `MacInstallGuard::drop` — removes only owned incomplete macOS install.
- `ensure_recovery_reference(...)` — preserves verified recovery under deterministic normal/conflict path.
- `recreate_unix_recovery(...)` — platform variants for rebuilding reference from held content.

## Attachment and low-level filesystem guards

- `recovery_directory_attached(root,directory)` — verifies recovery dir is still the root child.
- `workspace_path_matches(rootFd,rootPath,identity)` — root path attachment.
- `relative_directory_matches(...)` — reopens relative parent chain and compares identity.
- `open_relative_directory_no_follow(root,relative)` — component-by-component directory walk.
- `recovery_message`, `conflict_with_recovery` — verified/generic recovery diagnostics.
- `directory_path_matches(directory,path)`, `unix_file_identity(file)` — device/inode attachment.
- `open_unnamed_temp(dirFd)` — Linux `O_TMPFILE` held staging/recovery inode.
- `read_file_bytes`, `read_named_bytes`, `read_named_token` — held/named reads.
- `content_token_bytes(bytes)` / `content_token(text)` — BLAKE3 optimistic token.
- `captured_baseline_matches(...)` — exact identity/token state classification after exchange.
- `ensure_no_pending_quarantine(directory,path)` — fail-closed ordinary save while `.external/.blocked` exists.
- `quarantine_after_exchange(...)` — creates `.blocked`, moves verified staging to `.external` without replacement, syncs.

## Atomic exchange and staging

- `rename_noreplace_between` — Linux/macOS cfg implementations of cross-dir exclusive rename.
- `rename_exchange_between` — Linux `renameat2(EXCHANGE)` and macOS `renameatx_np(SWAP)` variants.
- `prepare_exchange_staging(...)` — Linux deterministic reusable `.staging`; refreshes metadata/content while live target remains untouched.
- `prepare_macos_exchange_staging(...)` — macOS held/locked deterministic staging.
- `link_unnamed_temp(dirFd,file,name)` — `linkat(AT_EMPTY_PATH)` no-replace install.
- `copy_unix_metadata(source,target)` — owner/mode/xattr/ACL-backed attribute copy; removes stale xattrs.
- `unix_entry_matches_identity`, `unix_entry_identity` — no-follow named identity.
- `open_directory_no_follow(path)` / `openat_file(...)` — safe held opens.

## Generic path/content helpers

- `next_revision(revision)` — checked monotonic increment.
- `resolve_document_path(root,documentPath)` — normalized relative `.md`, no traversal/symlink escape, canonical parent/file validation.
- `has_forbidden_backslash(path,isWindows)` — Windows separator policy; Unix filename allowance.
- `preserve_line_endings(existing,new)` — converts new editor LF content to existing CRLF/CR convention.

## Tests

Shared helpers: `empty_patch`; `TestDirectory::new/path/drop`; `create_owned_recovery_directory`.

State-machine/unit cases:

- `recovery_reference_state_preserves_substitutions_and_reuses_absent_slots`.
- `swap_state_requires_the_exact_captured_baseline_identity_and_token`.
- `recovered_mutation_never_writes_without_recovery_and_reports_all_failures`.

Read/save/index cases:

- Unicode read/write; invalid/non-Markdown path rejection; platform backslash semantics.
- CRLF/LF preservation; single-document save/reindex; Unicode links do not panic/poison session.
- Internal-save watcher echo suppression; pending external write rejects dirty save.
- Moderate large Markdown; Unix backslash filename; symlink escape rejection.

Transaction/race cases:

- External replacement, metadata/xattr preservation, held-temp source substitution.
- Final pre-exchange in-place/atomic external writes restored live.
- Pre-exchange/installed/pre-restore quarantine; occupied `.external` produces bounded `.blocked`.
- Simultaneous transaction serialization; interrupted staging never goes live.
- Concurrent recovery restoration failure preserves content; moved displacement remains panic-free.
- Terminal symlink, parent swap, post-install parent move, workspace-root move.

Recovery ownership/cleanup cases:

- Self-contained/path-safe artifact codec.
- Repeated successful/no-op saves leave no artifacts, old text, or git changes.
- Startup removes owned crash secrets.
- Top-level unowned file/symlink and unsafe inner symlink/FIFO/nested/unknown content are fully preserved.
- Malformed/special recovery entries are preserved safely.
- Occupied conflict and concurrent slot appearance fail before mutation.
- Recovery directory detach before/after exchange never reports success.

Revision/session cases:

- Canonicalized root/scanned project pairing and missing-root rejection for `open_workspace`.
- Activation snapshot reflects the just-scanned project and never rescans disk.
- Revision 511/512 history boundary and overflow-before-commit.
- Watcher/refresh overflow leaves model/history unchanged.
- Save/watcher/refresh monotonic ordering and save catch-up of delayed unrelated patch.
- Pending save/read/capture rejection after workspace generation change.

Windows-only cases (`#[cfg(windows)]`, compiled and run only on the Windows portable build):

- `windows_path_identity` resolves the file currently at a path (not a cached/held value), so a path whose file was deleted and recreated reports a different identity.
- A writable document saves normally through the Windows transaction path and persists the exact edited content.
- A stale content token is rejected as an external-change conflict, distinct from an ordinary I/O failure, and leaves the on-disk document untouched.

Exact test functions: `open_workspace_pairs_a_canonical_root_with_the_scanned_project`, `activating_a_workspace_returns_its_snapshot_without_rescanning_disk`, `reads_and_writes_unicode_markdown`, `rejects_invalid_and_non_markdown_paths`, `treats_backslash_as_a_separator_only_on_windows`, `preserves_crlf_line_endings_when_saving`, `preserves_lf_line_endings_when_saving`, `saves_and_reindexes_only_the_active_document`, `unicode_link_targets_do_not_panic_or_poison_the_workspace_session`, `suppresses_the_watcher_echo_of_a_successful_internal_save`, `rejects_a_dirty_save_when_an_external_write_is_pending_debounce`, `preserves_an_external_replacement_between_validation_and_commit`, `preserves_mode_and_supported_extended_attributes`, `installs_the_held_temp_when_a_named_source_is_substituted`, `restores_external_writes_from_the_final_preexchange_window`, `quarantines_preexchange_and_installed_external_versions`, `quarantines_external_change_before_restore_exchange`, `occupied_external_slot_blocks_bounded_transaction_state`, `serializes_simultaneous_transactions_with_one_bounded_lock`, `interrupted_staging_content_never_reaches_the_live_path`, `preserves_concurrent_content_when_recovery_cannot_be_restored`, `remains_panic_free_when_the_displaced_entry_is_moved`, `recovery_artifact_is_self_contained_and_slot_names_are_path_safe`, `repeated_successful_saves_leave_no_workspace_artifacts_or_prior_text`, `repeated_noop_saves_keep_a_git_workspace_clean`, `opening_workspace_removes_crash_artifacts_and_deleted_secrets`, `startup_cleanup_preserves_unowned_top_level_entries`, `startup_cleanup_preflights_and_preserves_every_entry_on_unsafe_content`, `malformed_slot_is_preserved_under_a_safe_conflict_name`, `recovery_slot_classification_never_blocks_or_follows_special_entries`, `occupied_bounded_conflict_fails_before_document_mutation`, `concurrent_slot_appearance_is_preserved_and_install_fails_closed`, `detached_recovery_directory_fails_before_target_exchange`, `detached_recovery_directory_after_exchange_never_publishes_success`, `never_follows_a_symlink_swapped_before_commit`, `rejects_a_parent_directory_swap_without_writing_outside`, `rejects_success_when_the_parent_moves_after_install`, `avoids_a_false_recovery_path_when_the_workspace_root_moves`, `preserves_contiguous_history_at_the_511_and_512_revision_boundaries`, `rejects_revision_overflow_before_disk_commit`, `watcher_and_refresh_overflow_leave_model_and_history_unchanged`, `orders_save_watcher_and_refresh_mutations_monotonically`, `save_response_catches_up_an_unrelated_pending_watcher_patch`, `rejects_a_pending_save_after_the_workspace_changes`, `rejects_pending_reads_and_late_capture_after_the_workspace_changes`, `opens_scanned_unix_markdown_filename_with_backslash`, `handles_a_moderately_large_markdown_document`, `rejects_symlinks_that_escape_the_workspace`, `windows_path_identity_reflects_the_file_currently_at_the_path`, `windows_workspace_session_saves_a_writable_document`, `windows_workspace_session_reports_a_distinguishable_conflict_on_a_stale_token`.

## Cross-component flow

[`document commands`](../commands/document.rs.md) capture a lease → session reads/saves → [`markdown`](markdown.rs.md) refreshes index → session emits/returns [`WorkspacePatch`](../models/workspace.rs.md) → frontend [`project-state`](../../../src/lib/stores/project-state.ts.md) applies contiguous revisions.
