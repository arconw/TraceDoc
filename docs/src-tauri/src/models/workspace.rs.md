# Workspace Domain Model

Source: `src-tauri/src/models/workspace.rs`.

All structs derive clone/debug/equality/serialization and emit camelCase fields. Maps use `BTreeMap` for deterministic output.

## Graph records

- `FolderId`, `DocumentId` — string aliases.
- `Heading` — level/text.
- `Folder` — stable ID, name/path, optional parent, ordered child/document IDs.
- `Document` — stable ID, name/title/headings/path/parent.
- `DocumentLink` — stable source/optional target/raw target/resolution diagnostic.
- `ProjectModel` — root path and normalized folder/document/link graph.

## Synchronization records

- `DocumentReadResult` — generation/revision/content/BLAKE3 token.
- `DocumentIndexUpdate` — generation/revision/token/warning/document/links/catch-up patches.
- `WorkspaceSnapshot` — full project at one generation/revision.
- `WorkspacePatch` — compact upserts/removals and externally changed document IDs.
- `WorkspaceEventError` — generation/revision-scoped watcher failure.

Frontend mirror: [`types/workspace.ts`](../../../src/lib/types/workspace.ts.md).
