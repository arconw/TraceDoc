# Document Commands

Source: `src-tauri/src/commands/document.rs`.

Thin async adapters over [`WorkspaceSession`](../services/document.rs.md). Each captures a generation-bound lease before `spawn_blocking` and maps typed errors to IPC strings.

- `read_document(session,path,generation)` — returns content/token/revision.
- `write_document(session,path,content,expectedToken,expectedRevision,generation)` — optimistic save plus index/catch-up patches.
- `acknowledge_save_conflict(session,path,generation)` — clears bounded transaction quarantine after explicit UI resolution.

Frontend caller: [`MarkdownEditor`](../../../src/lib/components/MarkdownEditor.svelte.md).

## Tests

Built on `tauri::test::mock_app` (dev-only `tauri` `test` feature) so each command runs through its real `State<WorkspaceSession>` extraction and `spawn_blocking` error mapping, not just the underlying service call.

- `write_document_persists_content_and_updates_the_index` — a writable document saves through the native command and the index update reports the new content.
- `write_document_reports_the_external_change_conflict_distinctly` — a stale content token surfaces a `changed externally` message, not a generic failure.
- `write_document_reports_an_actionable_message_after_the_workspace_changes` — a stale workspace generation surfaces a `workspace changed` message and is never confused with an external-change conflict.
- `acknowledge_save_conflict_command_resolves_without_a_pending_conflict`.
