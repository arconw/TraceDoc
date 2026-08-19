# Editor Request State

Source: `src/lib/editor/request-state.ts`.

Pure guards for asynchronous reads and conflict resolution used by [`MarkdownEditor`](../components/MarkdownEditor.svelte.md).

## API

- `EditorReadRequest` — request version, document ID, workspace generation.
- `editorReadIsCurrent(...)` — accepts a result only when request/current IDs, versions, and generations all agree.
- `retainedLocalBaseline(localContent, diskContent, diskContentToken)` — records new disk baseline/token while deriving whether retained local text is dirty.
- `closesDeletedBufferWithoutDiskAccess(conflict)` — identifies deletion conflicts that must close without resolving the missing path.
- `writeResultIsStale(resultWorkspaceRevision, requestRevision)` — flags a `write_document` result as a lost conflict only when the backend revision failed to advance past the request-time snapshot. Must never be compared against a live/reactive revision value, since that travels over a separate `workspace-patch` event channel and can advance mid-flight for reasons unrelated to this save (e.g. an edit to a different document), which previously produced false "external conflict" results on saves that had already succeeded.
