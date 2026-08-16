# Workspace Types

Source: `src/lib/types/workspace.ts`.

Frontend mirror of [Rust workspace models](../../../src-tauri/src/models/workspace.rs.md).

## Graph

- `FolderId`, `DocumentId` — stable string IDs.
- `Heading` — numeric level and visible text.
- `Folder` — normalized hierarchy and child/document IDs.
- `Document` — name/title/headings/path/parent.
- `DocumentLink` — stable source/optional target/raw target/resolution diagnostic.
- `ProjectModel` — root path, normalized records, canonical links.

## Synchronization

- `DocumentReadResult` — generation/revision/content/content token.
- `DocumentIndexUpdate` — saved document index, token/warning, catch-up patches.
- `WorkspaceSnapshot` — full project at generation/revision.
- `WorkspacePatch` — compact upsert/remove sets and externally changed document IDs.
- `WorkspaceEventError` — generation/revision-scoped watcher error.

These are data-only contracts; state semantics live in [`project-state`](../stores/project-state.ts.md).
