# Editor Request State

Source: `src/lib/editor/request-state.ts`.

Pure guards for asynchronous reads and conflict resolution used by [`MarkdownEditor`](../components/MarkdownEditor.svelte.md).

## API

- `EditorReadRequest` — request version, document ID, workspace generation.
- `editorReadIsCurrent(...)` — accepts a result only when request/current IDs, versions, and generations all agree.
- `retainedLocalBaseline(localContent, diskContent, diskContentToken)` — records new disk baseline/token while deriving whether retained local text is dirty.
- `closesDeletedBufferWithoutDiskAccess(conflict)` — identifies deletion conflicts that must close without resolving the missing path.
