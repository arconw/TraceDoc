# Project State Reducers

Source: `src/lib/stores/project-state.ts`.

Pure state machine for empty/loading/loaded/error workspace state with generation and contiguous revision ordering.

## API

- `ProjectState` — discriminated state; loading buffers events, loaded owns project/selection/revisions/change versions/gap patches/watch error.
- `applyDocumentIndex(state, update)` — rejects stale/wrong saves; applies catch-up patches or replaces one document's index.
- `applyWorkspacePatch(state, patch)` — buffers during open, rejects stale/wrong generations, queues revision gaps, applies only `N+1`.
- `applyContiguousPatch(state, patch)` — immutable folder/document/link mutations and external-change counters.
- `drainPendingPatches(state)` — repeatedly applies newly contiguous buffered revisions.
- `applyWorkspaceSnapshot(state, snapshot)` — revision-gated refresh; invalidates active document read and drains newer gaps.
- `applyWorkspaceError(state, error)` — buffers during open or exposes only current/nonolder watcher errors.
- `closeSelectedDocument(state, documentId)` — clears selection only when IDs match.
- `completeWorkspaceOpen(state, snapshot)` — constructs loaded state, then replays buffered patches/errors.
- `canonicalLinks(links)` — source-ID then link-ID stable ordering.
- `compareCodePoints(left,right)` — locale-independent Unicode ordering.

Consumes serialized contracts from [`types/workspace`](../types/workspace.ts.md).
