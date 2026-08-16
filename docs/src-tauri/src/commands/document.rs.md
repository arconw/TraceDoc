# Document Commands

Source: `src-tauri/src/commands/document.rs`.

Thin async adapters over [`WorkspaceSession`](../services/document.rs.md). Each captures a generation-bound lease before `spawn_blocking` and maps typed errors to IPC strings.

- `read_document(session,path,generation)` — returns content/token/revision.
- `write_document(session,path,content,expectedToken,expectedRevision,generation)` — optimistic save plus index/catch-up patches.
- `acknowledge_save_conflict(session,path,generation)` — clears bounded transaction quarantine after explicit UI resolution.

Frontend caller: [`MarkdownEditor`](../../../src/lib/components/MarkdownEditor.svelte.md).
